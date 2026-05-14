#!/usr/bin/env bash
set -euo pipefail

CHECK_NAME="${CARETTA_CI_CHECK_NAME:-Test}"
WORKFLOW_FILE="${CARETTA_CI_WORKFLOW:-ci.yml}"
TIMEOUT_SECONDS="${CARETTA_CI_TIMEOUT_SECONDS:-2700}"
POLL_SECONDS="${CARETTA_CI_POLL_SECONDS:-20}"
PR_LIMIT="${CARETTA_CI_PR_LIMIT:-100}"
ISSUES_JSON="${ISSUES_JSON:-[]}"

if [[ -z "${GH_REPO:-}" ]]; then
  GH_REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
fi

if ! jq -e . >/dev/null 2>&1 <<<"$ISSUES_JSON"; then
  echo "::error::ISSUES_JSON is not valid JSON."
  exit 1
fi

latest_check() {
  local sha="$1"
  gh api "repos/$GH_REPO/commits/$sha/check-runs?per_page=100" \
    | jq -c --arg name "$CHECK_NAME" '
      [.check_runs[]? | select(.name == $name)]
      | sort_by(.started_at // .created_at // "")
      | last // empty
    '
}

active_run_count() {
  local branch="$1"
  local sha="$2"
  local status="$3"

  gh run list \
    --workflow "$WORKFLOW_FILE" \
    --branch "$branch" \
    --status "$status" \
    --limit 50 \
    --json databaseId,headSha \
    | jq --arg sha "$sha" '[.[] | select(.headSha == $sha)] | length'
}

active_run_total() {
  local branch="$1"
  local sha="$2"
  local queued_runs
  local in_progress_runs

  queued_runs="$(active_run_count "$branch" "$sha" queued)"
  in_progress_runs="$(active_run_count "$branch" "$sha" in_progress)"
  echo $((queued_runs + in_progress_runs))
}

set_commit_status() {
  local sha="$1"
  local state="$2"
  local description="$3"
  local target_url="${4:-}"
  local args=(
    --method POST
    "repos/$GH_REPO/statuses/$sha"
    -f "state=$state"
    -f "context=$CHECK_NAME"
    -f "description=${description:0:140}"
  )

  if [[ -n "$target_url" ]]; then
    args+=(-f "target_url=$target_url")
  fi

  if ! gh api "${args[@]}" >/dev/null; then
    echo "::warning::Failed to write '$CHECK_NAME' commit status '$state' for $sha."
  fi
}

append_summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$@" >>"$GITHUB_STEP_SUMMARY"
  fi
}

open_prs_json="$(gh pr list \
  --state open \
  --limit "$PR_LIMIT" \
  --json number,headRefName,headRefOid,isDraft,mergeStateStatus,url)"

if [[ "$(jq 'length' <<<"$ISSUES_JSON")" -eq 0 ]]; then
  eligible_json="$(jq -c '
    [
      .[]
      | select(
          .isDraft == false
          and .mergeStateStatus != "DIRTY"
          and (.headRefName | test("^agent/issue-[0-9]+$"))
        )
      | {
          number,
          branch: .headRefName,
          sha: .headRefOid,
          url
        }
    ]
    | sort_by(.branch | capture("^agent/issue-(?<issue>[0-9]+)$").issue | tonumber)
  ' <<<"$open_prs_json")"
else
  eligible_json="$(jq -c --argjson issues "$ISSUES_JSON" '
    ($issues | map(tostring)) as $issue_strings
    | [
        $issue_strings[] as $issue
        | .[]
        | select(
            .isDraft == false
            and .mergeStateStatus != "DIRTY"
            and .headRefName == ("agent/issue-" + $issue)
          )
        | {
            number,
            branch: .headRefName,
            sha: .headRefOid,
            url
          }
      ]
  ' <<<"$open_prs_json")"
fi

eligible_count="$(jq 'length' <<<"$eligible_json")"
echo "Found $eligible_count eligible agent PR(s) for '$CHECK_NAME' CI gating."
append_summary "### Agent PR CI gate" ""
append_summary "- Eligible PRs: $eligible_count"

if [[ "$eligible_count" -eq 0 ]]; then
  append_summary "- Result: no eligible PR heads to check."
  exit 0
fi

dispatched_count=0
active_count=0
already_current_count=0
completed_non_success_count=0

while IFS=$'\t' read -r number branch sha url; do
  [[ -z "${number:-}" ]] && continue

  check_json="$(latest_check "$sha")"
  if [[ -n "$check_json" ]]; then
    status="$(jq -r '.status // ""' <<<"$check_json")"
    conclusion="$(jq -r '.conclusion // ""' <<<"$check_json")"
    details_url="$(jq -r '.details_url // ""' <<<"$check_json")"
    if [[ "$status" == "completed" && "$conclusion" == "success" ]]; then
      set_commit_status "$sha" success "GitHub Actions $CHECK_NAME passed." "$details_url"
      echo "PR #$number already has successful '$CHECK_NAME' check on $sha."
      already_current_count=$((already_current_count + 1))
      continue
    fi
    if [[ "$status" == "completed" ]]; then
      if [[ "$(active_run_total "$branch" "$sha")" -gt 0 ]]; then
        set_commit_status "$sha" pending "Fresh GitHub Actions $CHECK_NAME run is active." "$details_url"
        echo "PR #$number has a previous '$CHECK_NAME' conclusion '$conclusion', but a fresh $WORKFLOW_FILE run is active."
        active_count=$((active_count + 1))
        continue
      fi
      set_commit_status "$sha" failure "GitHub Actions $CHECK_NAME concluded: $conclusion." "$details_url"
      echo "PR #$number already has completed '$CHECK_NAME' check on $sha with conclusion '$conclusion'."
      completed_non_success_count=$((completed_non_success_count + 1))
      continue
    fi
    set_commit_status "$sha" pending "GitHub Actions $CHECK_NAME is $status." "$details_url"
    echo "PR #$number already has '$CHECK_NAME' check in status '$status' on $sha."
    active_count=$((active_count + 1))
    continue
  fi

  if [[ "$(active_run_total "$branch" "$sha")" -gt 0 ]]; then
    set_commit_status "$sha" pending "GitHub Actions $CHECK_NAME is already queued or running."
    echo "PR #$number already has queued/running $WORKFLOW_FILE for $branch @ $sha."
    active_count=$((active_count + 1))
    continue
  fi

  set_commit_status "$sha" pending "Dispatching GitHub Actions $CHECK_NAME."
  echo "Dispatching $WORKFLOW_FILE for PR #$number ($branch @ $sha)."
  gh workflow run "$WORKFLOW_FILE" --ref "$branch"
  dispatched_count=$((dispatched_count + 1))
done < <(
  jq -r '.[] | [.number, .branch, .sha, .url] | @tsv' <<<"$eligible_json"
)

append_summary "- Already green: $already_current_count"
append_summary "- Already active: $active_count"
append_summary "- Dispatched: $dispatched_count"
append_summary "- Completed non-success before wait: $completed_non_success_count"

deadline=$((SECONDS + TIMEOUT_SECONDS))

while true; do
  pending_count=0
  passed_count=0

  while IFS=$'\t' read -r number branch sha url; do
    [[ -z "${number:-}" ]] && continue

    check_json="$(latest_check "$sha")"
    if [[ -z "$check_json" ]]; then
      echo "PR #$number is waiting for '$CHECK_NAME' check to appear on $sha."
      pending_count=$((pending_count + 1))
      continue
    fi

    status="$(jq -r '.status // ""' <<<"$check_json")"
    conclusion="$(jq -r '.conclusion // ""' <<<"$check_json")"
    details_url="$(jq -r '.details_url // ""' <<<"$check_json")"

    if [[ "$status" != "completed" ]]; then
      echo "PR #$number '$CHECK_NAME' check is $status."
      pending_count=$((pending_count + 1))
      continue
    fi

    if [[ "$conclusion" == "success" ]]; then
      set_commit_status "$sha" success "GitHub Actions $CHECK_NAME passed." "$details_url"
      passed_count=$((passed_count + 1))
      continue
    fi

    if [[ "$(active_run_total "$branch" "$sha")" -gt 0 ]]; then
      set_commit_status "$sha" pending "Fresh GitHub Actions $CHECK_NAME run is active." "$details_url"
      echo "PR #$number has previous '$CHECK_NAME' conclusion '$conclusion', but a fresh run is still active."
      pending_count=$((pending_count + 1))
      continue
    fi

    set_commit_status "$sha" failure "GitHub Actions $CHECK_NAME concluded: $conclusion." "$details_url"
    echo "::error::PR #$number '$CHECK_NAME' check completed with conclusion '$conclusion': $url"
    append_summary "- Failed: PR #$number ($conclusion) - $url"
    exit 1
  done < <(
    jq -r '.[] | [.number, .branch, .sha, .url] | @tsv' <<<"$eligible_json"
  )

  if [[ "$pending_count" -eq 0 ]]; then
    echo "All $passed_count eligible PR head(s) passed '$CHECK_NAME'."
    append_summary "- Passed: $passed_count"
    append_summary "- Result: green."
    exit 0
  fi

  if (( SECONDS >= deadline )); then
    while IFS=$'\t' read -r number branch sha url; do
      [[ -z "${number:-}" ]] && continue

      check_json="$(latest_check "$sha")"
      if [[ -z "$check_json" ]]; then
        set_commit_status "$sha" error "Timed out waiting for GitHub Actions $CHECK_NAME."
        continue
      fi

      status="$(jq -r '.status // ""' <<<"$check_json")"
      details_url="$(jq -r '.details_url // ""' <<<"$check_json")"
      if [[ "$status" != "completed" ]]; then
        set_commit_status "$sha" error "Timed out while GitHub Actions $CHECK_NAME was $status." "$details_url"
      fi
    done < <(
      jq -r '.[] | [.number, .branch, .sha, .url] | @tsv' <<<"$eligible_json"
    )

    echo "::error::Timed out after ${TIMEOUT_SECONDS}s waiting for '$CHECK_NAME' checks."
    append_summary "- Timed out waiting for checks: $pending_count"
    exit 1
  fi

  echo "Waiting ${POLL_SECONDS}s for $pending_count pending '$CHECK_NAME' check(s)..."
  sleep "$POLL_SECONDS"
done
