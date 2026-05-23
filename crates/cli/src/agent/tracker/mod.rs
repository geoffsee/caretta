use crate::agent::gh::Gh;
use crate::agent::platform::{
    IssueActions, OpenPrReviewThreads, PrReviewThread, PullRequestActions,
};
use crate::agent::shell::log;
pub use cli_common::{PendingIssue, PrAuthor, PrSummary, TrackerInfo};
use std::collections::{HashMap, HashSet};

/// Standardized label taxonomy — single source of truth for all label strings.
///
/// Corresponds to `.github/labels.yml`. Every `gh issue create` invocation in
/// prompt builders should reference these constants rather than hardcoded
/// string literals. See AGENTS.md "Label conventions" for usage rules.
#[allow(dead_code)]
pub mod labels {
    // ── Workflow labels (no prefix) ────────────────────────────────────────
    pub const TRACKER: &str = "tracker";
    pub const IDEATION: &str = "ideation";
    pub const UXR_SYNTHESIS: &str = "uxr-synthesis";
    pub const STRATEGIC_REVIEW: &str = "strategic-review";
    pub const ROADMAP: &str = "roadmap";
    pub const SPRINT: &str = "sprint";
    pub const CODE_REVIEW: &str = "code-review";
    pub const SECURITY: &str = "security";
    pub const RETROSPECTIVE: &str = "retrospective";
    pub const DEV_UI: &str = "dev-ui";

    // ── area: — crate / subsystem ──────────────────────────────────────────
    pub const AREA_DEV_UI: &str = "area:dev-ui";
    pub const AREA_EDGE_NODE: &str = "area:edge-node";
    pub const AREA_GATEWAY_NODE: &str = "area:gateway-node";
    pub const AREA_NETWORK_NODE: &str = "area:network-node";
    pub const AREA_SERVICE_NODE: &str = "area:service-node";
    pub const AREA_CONSOLE_NODE: &str = "area:console-node";
    pub const AREA_CARETTA_CLI: &str = "area:caretta-cli";
    pub const AREA_DOCS: &str = "area:docs";
    pub const AREA_CI: &str = "area:ci";

    // ── kind: — type of work ───────────────────────────────────────────────
    pub const KIND_BUG: &str = "kind:bug";
    pub const KIND_FEATURE: &str = "kind:feature";
    pub const KIND_REFACTOR: &str = "kind:refactor";
    pub const KIND_PERF: &str = "kind:perf";
    pub const KIND_TEST: &str = "kind:test";
    pub const KIND_DOCS: &str = "kind:docs";
    pub const KIND_CHORE: &str = "kind:chore";
    pub const KIND_SECURITY: &str = "kind:security";

    // ── severity: — security findings and bugs ─────────────────────────────
    pub const SEVERITY_CRITICAL: &str = "severity:critical";
    pub const SEVERITY_HIGH: &str = "severity:high";
    pub const SEVERITY_MEDIUM: &str = "severity:medium";
    pub const SEVERITY_LOW: &str = "severity:low";
    pub const SEVERITY_INFO: &str = "severity:info";

    // ── priority: — sprint scheduling ──────────────────────────────────────
    pub const PRIORITY_P0: &str = "priority:p0";
    pub const PRIORITY_P1: &str = "priority:p1";
    pub const PRIORITY_P2: &str = "priority:p2";
    pub const PRIORITY_P3: &str = "priority:p3";

    // ── status: — current state (rare) ─────────────────────────────────────
    pub const STATUS_BLOCKED: &str = "status:blocked";
    pub const STATUS_NEEDS_REVIEW: &str = "status:needs-review";
    pub const STATUS_WONTFIX: &str = "status:wontfix";
}

/// Extract only `#N` issue references. Ignores bare numbers so
/// things like "10MB" don't pollute results.
pub fn extract_issue_refs(s: &str) -> Vec<u32> {
    let mut nums = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            i += 1;
            // Skip optional whitespace after #
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > start
                && let Ok(n) = s[start..i].parse::<u32>()
            {
                nums.push(n);
            }
        } else {
            i += 1;
        }
    }
    nums
}

/// Extract bare decimal numbers (fallback for "blocked by 3, 5").
pub fn extract_bare_numbers(s: &str) -> Vec<u32> {
    let mut nums = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if let Ok(n) = s[start..i].parse::<u32>() {
                nums.push(n);
            }
        } else {
            i += 1;
        }
    }
    nums
}

/// Extract blocker numbers from the tail after "blocked by".
/// Prefers `#N` refs; falls back to bare numbers.
pub fn extract_blockers(tail: &str) -> Vec<u32> {
    let refs = extract_issue_refs(tail);
    if !refs.is_empty() {
        refs
    } else {
        extract_bare_numbers(tail)
    }
}

pub fn parse_completed(body: &str) -> HashSet<u32> {
    let mut set = HashSet::new();
    for line in body.lines() {
        let lower = line.to_lowercase();
        // Support both Markdown checkboxes and various table status markers
        let is_done = lower.contains("[x]")
            || lower.contains("✅")
            || lower.contains("✔️")
            || lower.contains("☑️")
            || lower.contains("done")
            || lower.contains("complete");

        if is_done {
            let refs = extract_issue_refs(line);
            if line.contains('|') {
                // Heuristic for table rows: only take the first issue number.
                if let Some(&first) = refs.first() {
                    set.insert(first);
                }
            } else {
                for num in refs {
                    set.insert(num);
                }
            }
        }
    }
    set
}

pub fn parse_pending(body: &str) -> Vec<PendingIssue> {
    let completed = parse_completed(body);
    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    for line in body.lines() {
        let lower = line.to_lowercase();
        // Support Markdown checkboxes and the table status markers from ISSUES.md (🟡, 🔴)
        let is_pending = lower.contains("[ ]") || lower.contains("🟡") || lower.contains("🔴");
        if !is_pending {
            continue;
        }

        let refs = extract_issue_refs(line);
        let Some(&number) = refs.first() else {
            continue;
        };

        if completed.contains(&number) || !seen.insert(number) {
            continue;
        }

        let blockers = match lower.find("blocked by") {
            Some(idx) => {
                let tail = &line[idx + "blocked by".len()..];
                extract_blockers(tail)
            }
            None => {
                // Heuristic for table rows: assume second column contains dependencies
                if line.contains('|') {
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 3 {
                        extract_issue_refs(parts[2])
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
        };

        // Extract title: text after the issue number, before status markers or pipes
        let title = {
            // Find text after the last #N reference up to end-of-line or pipe
            let after_ref = if let Some(pos) = line.find(&format!("#{number}")) {
                let skip = pos + format!("#{number}").len();
                line[skip..].trim_start_matches(|c: char| {
                    c == '*' || c == '_' || c == ' ' || c == ':' || c == ')'
                })
            } else {
                ""
            };
            // Take up to a pipe or blocker marker
            let end = after_ref
                .find('|')
                .or_else(|| after_ref.to_lowercase().find("blocked"))
                .unwrap_or(after_ref.len());
            after_ref[..end]
                .trim()
                .trim_end_matches(['*', '_'])
                .to_string()
        };

        issues.push(PendingIssue {
            number,
            title,
            blockers,
            pr_number: None,
        });
    }
    issues
}

pub fn is_ready(issue: &PendingIssue, completed: &HashSet<u32>) -> bool {
    issue.blockers.iter().all(|b| completed.contains(b))
}

/// Pending tracker issue numbers ordered for execution. Dependents are queued after any
/// **pending** blockers listed on their tracker rows (edges inferred only among pending rows).
/// Rows marked completed in the tracker body satisfy blocker constraints without reordering.
/// Stable tie-break: earliest tracker-body occurrence wins (`parse_pending` order).
///
/// If constraints among pending rows are cyclic or ambiguous, remaining issues are appended in
/// document order (same net effect as the previous loop worker visiting rows sequentially).
pub fn pending_issues_execution_order(body: &str) -> Vec<u32> {
    let completed = parse_completed(body);
    let pending = parse_pending(body);
    if pending.is_empty() {
        return Vec::new();
    }

    let pending_set: HashSet<u32> = pending.iter().map(|p| p.number).collect();
    let doc_rank: HashMap<u32, usize> = pending
        .iter()
        .enumerate()
        .map(|(idx, p)| (p.number, idx))
        .collect();

    fn blockers_satisfied_for_pick(
        blockers: &[u32],
        pending_set: &HashSet<u32>,
        completed: &HashSet<u32>,
        picked: &HashSet<u32>,
    ) -> bool {
        blockers.iter().all(|b| {
            if completed.contains(b) {
                return true;
            }
            if !pending_set.contains(b) {
                return true;
            }
            picked.contains(b)
        })
    }

    let mut ordered = Vec::with_capacity(pending.len());
    let mut picked: HashSet<u32> = HashSet::new();

    while picked.len() < pending.len() {
        let mut ready: Vec<u32> = pending
            .iter()
            .filter(|p| !picked.contains(&p.number))
            .filter(|p| blockers_satisfied_for_pick(&p.blockers, &pending_set, &completed, &picked))
            .map(|p| p.number)
            .collect();

        if ready.is_empty() {
            let mut rest: Vec<u32> = pending
                .iter()
                .filter(|p| !picked.contains(&p.number))
                .map(|p| p.number)
                .collect();
            rest.sort_by_key(|n| doc_rank[n]);
            for n in rest {
                ordered.push(n);
                picked.insert(n);
            }
            break;
        }

        ready.sort_by_key(|n| doc_rank[n]);
        let next = ready[0];
        ordered.push(next);
        picked.insert(next);
    }

    ordered
}

/// Return body with `- [ ] #N` (or `- [ ] **#N**`) replaced by `- [x] ...`.
///
/// Handles optional bold/italic markdown wrapping around the issue reference.
pub fn mark_completed(body: &str, issue_num: u32) -> String {
    let needle = "- [ ] ";
    let mut result = String::with_capacity(body.len());
    for line in body.lines() {
        if line.contains(needle) {
            let refs = extract_issue_refs(line);
            if refs.first() == Some(&issue_num) {
                result.push_str(&line.replacen("- [ ] ", "- [x] ", 1));
                result.push('\n');
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    // Trim trailing newline if original didn't end with one.
    if !body.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Return issue numbers of open issues whose title starts with "retro:".
/// These are retrospective action items that should be worked on before
/// regular sprint tracker issues.
pub fn find_retro_issues() -> Vec<u32> {
    let out = Gh::open_issue_numbers_matching_title("retro in:title").unwrap_or_default();
    out.lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// Parse the JSON output of `gh issue list --label tracker --json number,title`
/// into the canonical, sorted, deduped tracker list. Pure helper so the
/// behavior is unit-testable without invoking `gh`.
pub(crate) fn parse_tracker_list(
    rows: &[crate::agent::platform::IssueSummary],
) -> Vec<TrackerInfo> {
    let mut nums: Vec<TrackerInfo> = rows
        .iter()
        .map(|row| TrackerInfo {
            number: row.number,
            title: row.title.clone(),
        })
        .collect();
    nums.sort_by_key(|t| t.number);
    nums.dedup_by_key(|t| t.number);
    nums
}

pub fn find_tracker() -> Vec<TrackerInfo> {
    // Trackers are identified by the `tracker` label. Title-based search was
    // previously used, but it incidentally matched any issue mentioning
    // "sprint" or "tracker" in its title (e.g. "Dev UI: agent must read parent
    // tracker before working a child issue"). The label is the authoritative
    // source — see #85 for the standardized label taxonomy.
    let out = Gh::open_issue_summaries_with_label(labels::TRACKER).unwrap_or_default();
    parse_tracker_list(&out)
}

/// Build a map from issue number to PR number for the given list of open PRs
/// whose branch matches the `agent/issue-{N}` convention.
pub fn open_pr_map_from(prs: &[PrSummary]) -> std::collections::HashMap<u32, u32> {
    let mut map = std::collections::HashMap::new();
    for pr in prs {
        if let Some(rest) = pr.head_ref_name.strip_prefix("agent/issue-")
            && let Ok(issue_num) = rest.parse::<u32>()
        {
            map.insert(issue_num, pr.number);
        }
    }
    map
}

pub fn get_tracker_body(tracker: u32) -> String {
    Gh::issue_body_or_die(tracker, "failed to read tracker body")
}

pub fn check_off_issue(tracker: u32, issue_num: u32) {
    let body = get_tracker_body(tracker);
    let updated = mark_completed(&body, issue_num);
    if !Gh::edit_issue_body(tracker, &updated) {
        crate::agent::shell::die(&format!("failed to check off #{issue_num} in tracker"));
    }
    log(&format!("Checked off #{issue_num} in tracker"));
}

pub fn close_issue(issue_num: u32) {
    if !Gh::close_issue(issue_num) {
        log(&format!("WARNING: failed to close #{issue_num}"));
    } else {
        log(&format!("Closed #{issue_num}"));
    }
}

/// Given a list of blocker issue numbers, find the first one with an open PR
/// and return its branch name. Otherwise returns [`origin_default_branch`].
///
/// [`origin_default_branch`]: crate::agent::cmd::origin_default_branch
pub fn find_upstream_branch(blockers: &[u32]) -> String {
    for &blocker in blockers {
        let head = format!("agent/issue-{blocker}");
        if let Some(branch) = Gh::find_open_pr_head_for_head(&head)
            && !branch.is_empty()
        {
            return branch;
        }
    }
    crate::agent::cmd::origin_default_branch()
}

pub fn fetch_issue(issue_num: u32) -> (String, String) {
    let context = format!("failed to fetch issue #{issue_num}");
    let title = Gh::issue_title_or_die(issue_num, &context);
    let body = Gh::issue_body_or_die(issue_num, &context);
    (title, body)
}

pub fn build_prompt(
    project_name: &str,
    issue_num: u32,
    title: &str,
    body: &str,
    codebase: &str,
    tracker_num: u32,
    tracker_body: &str,
) -> String {
    let tracker_section = if !tracker_body.is_empty() {
        format!(
            r#"## Parent Tracker #{tracker_num}

This issue is part of a tracker. Read the tracker body below to understand the
broader scope, sibling dependencies, sprint goal, and any constraints the human
captured before starting work. **Treat the tracker as authoritative for scope**:
do not expand beyond what the tracker authorises, and do not narrow below what
sibling issues depend on you delivering.

{tracker_body}

"#
        )
    } else {
        String::new()
    };

    let tracker_instruction = if !tracker_body.is_empty() {
        "\n- Before diving into implementation, re-read the Parent Tracker section above. If your planned changes conflict with a sibling issue, the dependency hierarchy, or the sprint goal, **stop and surface the conflict as a comment on the tracker** instead of proceeding silently."
    } else {
        ""
    };

    format!(
        r#"You are working on the {project_name} project.

{tracker_section}Implement the following GitHub issue:

## Issue #{issue_num}: {title}

{body}

## Codebase Snapshot

The following is a cleaned snapshot of the entire project. Use this as your primary
reference — avoid re-reading files that are already included below.

{codebase}

## Instructions
- Read AGENTS.md and the relevant skills/ for project conventions before starting.
- Implement the changes described above.
- Validate your changes using the test/build/format commands documented in AGENTS.md.
- Keep idle memory under 10MB — no unnecessary allocations.
- Do NOT update shared tracker/status files such as ISSUES.md or STATUS.md from an issue implementation branch; serialized planning, retrospective, and housekeeping workflows own those edits.
- Do NOT modify `.github/**`, especially `.github/workflows/**`, from a sprint/tracker issue branch. If this issue or its parent tracker requires `.github/**` changes, stop and comment that the work is blocked by the immutable CI control-plane policy.
- Do NOT commit changes — the calling script handles commits.{tracker_instruction}"#
    )
}

#[allow(dead_code)]
pub fn build_fix_prompt(issue_num: u32, output: &str) -> String {
    format!(
        r#"Testing failed for issue #{issue_num}.

Here is the output:

{output}

Fix the issues reported above. Do NOT commit — the calling script handles commits."#
    )
}

pub fn build_lint_fix_prompt(issue_num: u32, clippy_output: &str) -> String {
    format!(
        r#"The pre-commit hook for issue #{issue_num} failed due to clippy warnings.

Here is the clippy output:

{clippy_output}

Fix ALL clippy warnings above. Common fixes:
- `too_many_arguments`: add `#[allow(clippy::too_many_arguments)]` above the function
- `doc_overindented_list_items`: fix doc comment indentation
- `collapsible_if`: merge nested if-let into one
- Other warnings: follow the clippy suggestion

Do NOT commit — the calling script handles commits."#
    )
}

/// Prompt for re-invoking the agent when `git commit` is rejected by a
/// pre-commit hook in the target repo. Generic (no issue number) so both the
/// `issue` and `fix-pr` paths can share it. The captured output is the real
/// combined stdout+stderr from the failed `git commit` call.
pub fn build_commit_hook_fix_prompt(hook_output: &str) -> String {
    format!(
        r#"`git commit` was rejected by a pre-commit hook in this repository.

Here is the combined output from the failed commit attempt:

{hook_output}

Fix the issues the hook flagged so the next commit attempt can succeed. Guidance:
- If the hook is a type/lint/test gate, fix the underlying code it complained about.
- If the errors are in files you did not modify, treat them as pre-existing landmines blocking the commit — fix them too if a small change unblocks the gate, or stop and explain why you cannot.
- Do NOT bypass the hook (no `--no-verify`, no disabling the hook).
- Do NOT commit yourself — the calling script will retry the commit after you return."#
    )
}

pub fn build_test_fix_prompt(issue_num: u32, test_output: &str) -> String {
    format!(
        r#"The configured test command for issue #{issue_num} reported failures.

Here is the test output:

{test_output}

Fix ALL test failures above. Common guidance:
- If a test assertion fails, fix the code under test (not the test) unless the test expectation is clearly wrong.
- If a test times out, look for deadlocks, missing signals, or infinite loops in the code being tested.
- If a compilation error prevents tests from running, fix the compilation error.

Do NOT commit — the calling script handles commits."#
    )
}

/// Fetch open PRs as JSON (number, title, headRefName, author login).
///
/// Returns an empty Vec when `gh` is unavailable or GitHub is unreachable —
/// callers (including context gatherers used in `--dry-run`) treat the PR
/// list as best-effort context, not a hard dependency.
pub fn list_open_prs() -> Vec<PrSummary> {
    Gh::open_pr_summaries(50).unwrap_or_default()
}

/// Open pull request number for `head` equal to `branch`, if one exists.
pub fn open_pr_number_for_head_branch(branch: &str) -> Option<u32> {
    Gh::find_open_pr_number_for_head(branch)
}

/// Fetch the diff for a single PR.
pub fn pr_diff(pr_num: u32) -> String {
    Gh::pr_diff_or_die(pr_num)
}

/// Find the open PR for the current branch, if any.
pub fn current_branch_pr() -> Option<PrSummary> {
    let out = Gh::current_branch_pr_summary()?;
    Some(PrSummary {
        number: out.number,
        title: out.title,
        head_ref_name: out.head_ref,
        author: None,
        unresolved_thread_count: 0,
    })
}

fn parse_auto_merge_response(output: Option<String>) -> bool {
    match output {
        Some(s) => !s.is_empty() && s != "null",
        None => false,
    }
}

/// Check whether auto-merge is currently enabled on a PR.
pub fn is_auto_merge_enabled(pr_num: u32) -> bool {
    parse_auto_merge_response(Gh::pr_auto_merge_status_raw(pr_num))
}

/// Enable auto-merge (squash) on a PR. Returns true on success.
pub fn enable_auto_merge(pr_num: u32) -> bool {
    log(&format!("Enabling auto-merge on PR #{pr_num}..."));
    let (ok, output) = Gh::enable_pr_auto_merge_squash(pr_num);
    if ok {
        log(&format!("Auto-merge enabled on PR #{pr_num}"));
    } else {
        log(&format!(
            "Failed to enable auto-merge on PR #{pr_num}: {output}"
        ));
    }
    ok
}

/// Fetch PR body/description.
pub fn pr_body(pr_num: u32) -> String {
    Gh::pr_body_or_die(pr_num)
}

/// Fetch the head branch (ref name) of a pull request.
///
/// Used by the Phase 2 Fix Comments flow (#144) so the dev process can check
/// out the right branch into a worktree before launching the agent against
/// the PR's review threads.
pub fn pr_head_branch(pr_num: u32) -> String {
    Gh::pr_head_ref_or_die(pr_num)
}

/// Current GitHub `reviewDecision` for a PR.
///
/// Returns one of `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or
/// an empty string when no reviews have been submitted. Returns `None` if
/// `gh` is unreachable so callers can decide whether to retry or skip.
pub fn pr_review_decision(pr_num: u32) -> Option<String> {
    Gh::pr_review_decision(pr_num).map(|s| s.trim().to_string())
}

/// One unresolved review thread on a pull request.
///
/// Returned by [`fetch_unresolved_review_threads`] and consumed by
/// [`build_pr_review_fix_prompt`]. The `id` field is the GraphQL node ID
/// suitable for the `resolveReviewThread` mutation that Phase 3 (#145) will
/// invoke after a successful Fix Comments push.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThreadComment {
    pub author: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: u32,
    pub body: String,
    pub author: String,
    pub comments: Vec<ReviewThreadComment>,
}

/// Compact summary of a submitted PR review from `gh pr view --json reviews`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSummary {
    pub author: String,
    pub state: String,
    pub submitted_at: String,
    pub body: String,
}

/// Default bot login that owns automated review threads. This is matched
/// alongside two more general signals — the `[bot]` REST-style suffix and
/// the GraphQL `__typename: "Bot"` field — so the constant exists mainly as
/// a named fallback. Set to the GitHub App that posts reviews on this repo.
pub const DEFAULT_REVIEW_BOT_LOGIN: &str = "caretta-ai";

/// Opt-in marker a human can place in a review-thread comment body to request
/// that the Fix Comments agent treat that thread as actionable. Matched
/// case-insensitively against the first comment of each thread. Human
/// authors are otherwise excluded so the agent does not turn questions or
/// requests for discussion into unrequested code edits.
pub const HUMAN_FIX_MARKER: &str = "@caretta fix";

/// Returns `true` when `body` contains the [`HUMAN_FIX_MARKER`] opt-in
/// marker, case-insensitively. Pulled out so both parsers share the rule.
fn has_human_fix_marker(body: &str) -> bool {
    body.to_lowercase().contains(HUMAN_FIX_MARKER)
}

/// Raw JSON from GitHub's `reviewThreads` GraphQL query for `pr_num`, or
/// `None` when the repo or request cannot be resolved.
fn pull_request_review_threads(pr_num: u32) -> Option<Vec<PrReviewThread>> {
    let out = Gh::fetch_pr_review_threads(pr_num);
    if out.is_none() {
        log(&format!(
            "WARNING: failed to fetch review threads for PR #{pr_num}"
        ));
    }
    out
}

/// Fetch all unresolved bot-authored review threads on a PR via the GitHub
/// GraphQL API.
///
/// Uses `gh api graphql` so we inherit whatever credentials are in the
/// parent process's environment.
/// Filters out resolved threads. A thread is kept when any of these hold:
/// the author login matches `bot_login`; the author login ends with
/// `[bot]` (REST-style App suffix); the GraphQL `author.__typename` is
/// `Bot` (covers GitHub Apps whose login is returned without the `[bot]`
/// suffix, e.g. App-installation tokens); or the first comment's body
/// contains [`HUMAN_FIX_MARKER`] (a human opt-in so the agent can act on
/// specific human-authored review comments without blanket-trusting them).
pub fn fetch_unresolved_review_threads(pr_num: u32, bot_login: &str) -> Vec<ReviewThread> {
    pull_request_review_threads(pr_num)
        .map(|out| parse_review_threads(&out, bot_login))
        .unwrap_or_default()
}

/// Every unresolved inline review thread on a PR (any author). Used only when
/// [`crate::agent::issue::work_on_issue`] re-enters an open PR so requested changes are not
/// missed when the review did not come from the configured bot login.
///
/// Resolved threads and comments without a file path are still omitted (see
/// [`parse_all_unresolved_review_threads`]).
pub fn fetch_all_unresolved_review_threads(pr_num: u32) -> Vec<ReviewThread> {
    pull_request_review_threads(pr_num)
        .map(|out| parse_all_unresolved_review_threads(&out))
        .unwrap_or_default()
}

/// Fetch compact prior PR review summaries for prompt context.
pub fn fetch_pr_reviews(pr_num: u32) -> Vec<ReviewSummary> {
    Gh::pr_reviews(pr_num)
        .map(|out| parse_pr_reviews(&out))
        .unwrap_or_default()
}

/// Phase 3 (#145): mark a single review thread as resolved on GitHub via the
/// `resolveReviewThread` GraphQL mutation.
///
/// Returns `true` only if GitHub confirms `isResolved: true` in the response;
/// any error (network failure, malformed response, mutation rejection) is
/// surfaced as `false` and logged so the calling code can decide whether to
/// continue. Per the #145 acceptance criteria, resolve failures must NOT
/// abort a Fix Comments run — the fix is already pushed; an unresolved
/// thread is cosmetic.
pub fn resolve_review_thread(thread_id: &str) -> bool {
    let resp = match Gh::mark_review_thread_resolved(thread_id) {
        Some(r) => r,
        None => {
            log(&format!(
                "WARNING: gh api graphql failed for resolveReviewThread on {thread_id}"
            ));
            return false;
        }
    };
    let ok = parse_resolve_review_thread_response(&resp);
    if !ok {
        log(&format!(
            "WARNING: resolveReviewThread mutation did not confirm isResolved for {thread_id}: {resp}"
        ));
    }
    ok
}

/// Parse the JSON returned by the `resolveReviewThread` GraphQL mutation,
/// returning `true` only if GitHub confirmed `isResolved: true`.
///
/// Split out from [`resolve_review_thread`] so the parse path can be
/// unit-tested against fixture JSON without needing a live GitHub PR.
fn parse_resolve_review_thread_response(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            v.pointer("/data/resolveReviewThread/thread/isResolved")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

/// Parse the JSON returned by the `reviewThreads` GraphQL query into a list
/// of [`ReviewThread`]s, filtering out resolved and human-authored threads.
///
/// Split out from [`fetch_unresolved_review_threads`] so it can be unit-tested
/// against fixture JSON without needing a live GitHub PR.
fn parse_review_threads(nodes: &[PrReviewThread], bot_login: &str) -> Vec<ReviewThread> {
    let mut out = Vec::new();
    for thread in nodes {
        if thread.is_resolved {
            continue;
        }
        let id = thread.id.clone();
        if id.is_empty() {
            continue;
        }
        let comments = &thread.comments;
        let Some(c) = comments.first() else {
            continue;
        };
        let thread_comments = parse_review_thread_comments(comments);
        let author = c.author_login.clone();
        let typename = c.author_type.as_deref().unwrap_or("");
        let body = c.body.clone();
        let is_bot = author == bot_login || author.ends_with("[bot]") || typename == "Bot";
        if !is_bot && !has_human_fix_marker(&body) {
            continue;
        }
        let path = c.path.clone().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let line = c.line.or(c.original_line).unwrap_or(0);
        out.push(ReviewThread {
            id,
            path,
            line,
            body,
            author,
            comments: thread_comments,
        });
    }
    out
}

/// Parse the same `reviewThreads` JSON as [`parse_review_threads`], but keep
/// every unresolved thread with a file path (human reviewers included).
fn parse_all_unresolved_review_threads(nodes: &[PrReviewThread]) -> Vec<ReviewThread> {
    let mut out = Vec::new();
    for thread in nodes {
        if thread.is_resolved {
            continue;
        }
        let id = thread.id.clone();
        if id.is_empty() {
            continue;
        }
        let comments = &thread.comments;
        let Some(c) = comments.first() else {
            continue;
        };
        let thread_comments = parse_review_thread_comments(comments);
        let author = c.author_login.clone();
        let body = c.body.clone();
        let path = c.path.clone().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let line = c.line.or(c.original_line).unwrap_or(0);
        out.push(ReviewThread {
            id,
            path,
            line,
            body,
            author,
            comments: thread_comments,
        });
    }
    out
}

fn parse_review_thread_comments(
    comments: &[crate::agent::platform::PrReviewThreadComment],
) -> Vec<ReviewThreadComment> {
    comments
        .iter()
        .map(|c| ReviewThreadComment {
            author: c.author_login.clone(),
            body: c.body.clone(),
        })
        .collect()
}

fn parse_pr_reviews(
    reviews: &[crate::agent::platform::PrReviewSummaryRecord],
) -> Vec<ReviewSummary> {
    reviews
        .iter()
        .map(|review| ReviewSummary {
            author: review.author_login.clone(),
            state: review.state.clone(),
            submitted_at: review.submitted_at.clone(),
            body: review.body.clone(),
        })
        .collect()
}

pub fn render_prior_pr_review_context(reviews: &[ReviewSummary]) -> String {
    if reviews.is_empty() {
        return String::new();
    }

    let mut recent = reviews.to_vec();
    recent.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));

    let mut out = String::from("## Prior PR Review Context\n\n");
    for review in recent.iter().take(10) {
        out.push_str(&format!(
            "### {state} by @{author} at {submitted_at}\n\n",
            state = review.state,
            author = review.author,
            submitted_at = review.submitted_at,
        ));
        let body = cap_review_body(&review.body);
        if body.trim().is_empty() {
            out.push_str("(No review body.)\n\n");
        } else {
            out.push_str(&body);
            out.push_str("\n\n");
        }
    }
    out.trim_end().to_string()
}

fn cap_review_body(body: &str) -> String {
    const MAX_REVIEW_BODY_CHARS: usize = 1000;
    if body.chars().count() <= MAX_REVIEW_BODY_CHARS {
        return body.to_string();
    }
    let mut capped: String = body.chars().take(MAX_REVIEW_BODY_CHARS).collect();
    capped.push_str("\n\n[Review body truncated.]");
    capped
}

/// Phase 4 (#146): fetch unresolved bot-authored review thread counts for
/// every open pull request in a single batched GraphQL round-trip.
///
/// Returns a map keyed by PR number. PRs with zero unresolved bot threads
/// are not included in the map (callers should treat absence as "0"). Used
/// by `refresh_tracker` to populate `PrSummary::unresolved_thread_count`
/// before the sidebar re-renders, so the per-PR `(N)` badge stays in sync
/// with the rest of the refresh.
///
/// Acceptance criterion from #146: "Refresh time stays under ~2s for repos
/// with up to 20 open PRs (one batched query, not N round-trips)." A single
/// `repository.pullRequests(states: OPEN, first: 100)` query satisfies that
/// — N PRs cost one round-trip, not N.
pub fn fetch_unresolved_thread_counts(bot_login: &str) -> std::collections::HashMap<u32, u32> {
    let out = match Gh::fetch_open_pr_review_threads_batched() {
        Some(s) => s,
        None => {
            log("WARNING: failed to fetch open-PR thread counts");
            return std::collections::HashMap::new();
        }
    };
    parse_pr_thread_counts(&out, bot_login)
}

/// Parse the JSON returned by the batched `pullRequests.reviewThreads`
/// query into a `{pr_number: unresolved_bot_thread_count}` map.
///
/// Split out from [`fetch_unresolved_thread_counts`] so it can be unit-
/// tested against fixture JSON without needing live GitHub PRs. Mirrors
/// the filter logic from [`parse_review_threads`]: resolved threads are
/// dropped; a thread counts when its first comment is bot-authored (login
/// match, `[bot]` suffix, or GraphQL `author.__typename == "Bot"`) OR its
/// body contains [`HUMAN_FIX_MARKER`]. Badge count and Fix Comments agent
/// must see the same set of threads.
fn parse_pr_thread_counts(
    prs: &[OpenPrReviewThreads],
    bot_login: &str,
) -> std::collections::HashMap<u32, u32> {
    let mut counts = std::collections::HashMap::new();
    for pr in prs {
        let number = pr.pr_number;
        let threads = &pr.review_threads;
        let mut count: u32 = 0;
        for t in threads {
            if t.is_resolved {
                continue;
            }
            let author = t
                .comments
                .first()
                .map(|c| c.author_login.as_str())
                .unwrap_or("");
            let typename = t
                .comments
                .first()
                .and_then(|c| c.author_type.as_deref())
                .unwrap_or("");
            let body = t.comments.first().map(|c| c.body.as_str()).unwrap_or("");
            let is_bot = author == bot_login || author.ends_with("[bot]") || typename == "Bot";
            if is_bot || has_human_fix_marker(body) {
                count += 1;
            }
        }
        if count > 0 {
            counts.insert(number, count);
        }
    }
    counts
}

mod prompts;
pub use prompts::*;

#[cfg(test)]
mod tests;
