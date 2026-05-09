use crate::agent::shell::{cmd_capture, cmd_run, cmd_stdout, cmd_stdout_or_die, log};
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
    pub const AREA_FREQ_CLI: &str = "area:freq-cli";
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
    let out = cmd_stdout(
        "gh",
        &[
            "issue",
            "list",
            "--search",
            "retro in:title",
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            ".[].number",
        ],
    )
    .unwrap_or_default();
    out.lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// Parse the JSON output of `gh issue list --label tracker --json number,title`
/// into the canonical, sorted, deduped tracker list. Pure helper so the
/// behavior is unit-testable without invoking `gh`.
pub(crate) fn parse_tracker_list(json: &str) -> Vec<TrackerInfo> {
    #[derive(serde::Deserialize)]
    struct Row {
        number: u32,
        title: String,
    }
    let rows: Vec<Row> = serde_json::from_str(json).unwrap_or_default();
    let mut nums: Vec<TrackerInfo> = rows
        .into_iter()
        .map(|row| TrackerInfo {
            number: row.number,
            title: row.title,
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
    let out = cmd_stdout(
        "gh",
        &[
            "issue",
            "list",
            "--label",
            labels::TRACKER,
            "--state",
            "open",
            "--json",
            "number,title",
        ],
    );
    match out {
        Some(json) => parse_tracker_list(&json),
        None => Vec::new(),
    }
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
    let num = tracker.to_string();
    cmd_stdout_or_die(
        "gh",
        &["issue", "view", &num, "--json", "body", "--jq", ".body"],
        "failed to read tracker body",
    )
}

pub fn check_off_issue(tracker: u32, issue_num: u32) {
    let body = get_tracker_body(tracker);
    let updated = mark_completed(&body, issue_num);
    let tracker_s = tracker.to_string();
    if !cmd_run("gh", &["issue", "edit", &tracker_s, "--body", &updated]) {
        crate::agent::shell::die(&format!("failed to check off #{issue_num} in tracker"));
    }
    log(&format!("Checked off #{issue_num} in tracker"));
}

pub fn close_issue(issue_num: u32) {
    let num_s = issue_num.to_string();
    if !cmd_run("gh", &["issue", "close", &num_s]) {
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
        let out = cmd_stdout(
            "gh",
            &[
                "pr",
                "list",
                "--head",
                &head,
                "--state",
                "open",
                "--json",
                "headRefName",
                "--jq",
                ".[0].headRefName",
            ],
        );
        if let Some(branch) = out
            && !branch.is_empty()
        {
            return branch;
        }
    }
    crate::agent::cmd::origin_default_branch()
}

pub fn fetch_issue(issue_num: u32) -> (String, String) {
    let num_s = issue_num.to_string();
    let title = cmd_stdout_or_die(
        "gh",
        &["issue", "view", &num_s, "--json", "title", "--jq", ".title"],
        &format!("failed to fetch issue #{issue_num}"),
    );
    let body = cmd_stdout_or_die(
        "gh",
        &["issue", "view", &num_s, "--json", "body", "--jq", ".body"],
        &format!("failed to fetch issue #{issue_num}"),
    );
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
- Run ./scripts/test-examples.sh to verify nothing is broken.
- Keep idle memory under 10MB — no unnecessary allocations.
- After implementing, update ISSUES.md: set the status of #{issue_num} to ✅ Done in the Task Dependency Hierarchy table.
- Update STATUS.md if this issue changes the status of any tracked feature (e.g., from 🟡 to ✅).
- CRITICAL: Always keep ISSUES.md and STATUS.md in sync with your changes.
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

pub fn build_test_fix_prompt(issue_num: u32, test_output: &str) -> String {
    format!(
        r#"The pre-push hook for issue #{issue_num} failed because `cargo test` reported failures.

Here is the test output:

{test_output}

Fix ALL test failures above. Common fixes:
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
    let out = cmd_stdout(
        "gh",
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,headRefName,author",
            "--limit",
            "50",
        ],
    )
    .unwrap_or_default();
    serde_json::from_str(&out).unwrap_or_default()
}

/// Fetch the diff for a single PR.
pub fn pr_diff(pr_num: u32) -> String {
    let num_s = pr_num.to_string();
    cmd_stdout_or_die("gh", &["pr", "diff", &num_s], "failed to fetch PR diff")
}

/// Find the open PR for the current branch, if any.
pub fn current_branch_pr() -> Option<PrSummary> {
    let out = cmd_stdout("gh", &["pr", "view", "--json", "number,title,headRefName"])?;
    serde_json::from_str(&out).ok()
}

fn parse_auto_merge_response(output: Option<String>) -> bool {
    match output {
        Some(s) => !s.is_empty() && s != "null",
        None => false,
    }
}

/// Check whether auto-merge is currently enabled on a PR.
pub fn is_auto_merge_enabled(pr_num: u32) -> bool {
    let num_s = pr_num.to_string();
    let out = cmd_stdout(
        "gh",
        &[
            "pr",
            "view",
            &num_s,
            "--json",
            "autoMergeRequest",
            "--jq",
            ".autoMergeRequest",
        ],
    );
    parse_auto_merge_response(out)
}

/// Enable auto-merge (squash) on a PR. Returns true on success.
pub fn enable_auto_merge(pr_num: u32) -> bool {
    let num_s = pr_num.to_string();
    log(&format!("Enabling auto-merge on PR #{pr_num}..."));
    let (ok, output) = cmd_capture("gh", &["pr", "merge", &num_s, "--auto", "--squash"]);
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
    let num_s = pr_num.to_string();
    cmd_stdout_or_die(
        "gh",
        &["pr", "view", &num_s, "--json", "body", "--jq", ".body"],
        "failed to fetch PR body",
    )
}

/// Fetch the head branch (ref name) of a pull request.
///
/// Used by the Phase 2 Fix Comments flow (#144) so the dev process can check
/// out the right branch into a worktree before launching the agent against
/// the PR's review threads.
pub fn pr_head_branch(pr_num: u32) -> String {
    let num_s = pr_num.to_string();
    cmd_stdout_or_die(
        "gh",
        &[
            "pr",
            "view",
            &num_s,
            "--json",
            "headRefName",
            "--jq",
            ".headRefName",
        ],
        "failed to fetch PR head branch",
    )
}

/// One unresolved review thread on a pull request.
///
/// Returned by [`fetch_unresolved_review_threads`] and consumed by
/// [`build_pr_review_fix_prompt`]. The `id` field is the GraphQL node ID
/// suitable for the `resolveReviewThread` mutation that Phase 3 (#145) will
/// invoke after a successful Fix Comments push.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: u32,
    pub body: String,
    pub author: String,
}

/// Default bot login that owns automated review threads. Mirrors the default
/// in `scripts/resolve-pr-threads.sh` so a Fix Comments run only acts on
/// findings the dev agent itself raised, not human review comments.
pub const DEFAULT_REVIEW_BOT_LOGIN: &str = "llm-overlord";

/// Fetch all unresolved bot-authored review threads on a PR via the GitHub
/// GraphQL API.
///
/// Mirrors `scripts/resolve-pr-threads.sh` — uses `gh api graphql` so we
/// inherit whatever credentials are in the parent process's environment.
/// Filters out resolved threads and human-authored threads (only `bot_login`
/// or any author ending in `[bot]` is kept) so the Fix Comments agent only
/// touches findings the project's review bot raised.
pub fn fetch_unresolved_review_threads(pr_num: u32, bot_login: &str) -> Vec<ReviewThread> {
    let owner_repo = match cmd_stdout(
        "gh",
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ],
    ) {
        Some(s) if !s.is_empty() => s,
        _ => {
            log("WARNING: could not resolve owner/repo via `gh repo view`");
            return Vec::new();
        }
    };
    let (owner, repo) = match owner_repo.split_once('/') {
        Some((o, r)) => (o.to_string(), r.to_string()),
        None => {
            log(&format!(
                "WARNING: unexpected repo identifier '{owner_repo}'"
            ));
            return Vec::new();
        }
    };

    // Identical query to scripts/resolve-pr-threads.sh so behaviour stays in
    // lock-step with the prototype shell script. The leading newline keeps gh
    // from interpreting the value as a file reference.
    let query = "\nquery($owner: String!, $repo: String!, $number: Int!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequest(number: $number) {\n      reviewThreads(first: 100) {\n        nodes {\n          id\n          isResolved\n          comments(first: 1) {\n            nodes {\n              author { login }\n              path\n              line\n              originalLine\n              body\n            }\n          }\n        }\n      }\n    }\n  }\n}";

    let pr_num_s = pr_num.to_string();
    let owner_arg = format!("owner={owner}");
    let repo_arg = format!("repo={repo}");
    let number_arg = format!("number={pr_num_s}");
    let query_arg = format!("query={query}");

    let out = match cmd_stdout(
        "gh",
        &[
            "api",
            "graphql",
            "-F",
            &owner_arg,
            "-F",
            &repo_arg,
            "-F",
            &number_arg,
            "-f",
            &query_arg,
        ],
    ) {
        Some(s) => s,
        None => {
            log(&format!(
                "WARNING: failed to fetch review threads for PR #{pr_num}"
            ));
            return Vec::new();
        }
    };

    parse_review_threads(&out, bot_login)
}

/// GraphQL mutation that marks one review thread as resolved on a pull
/// request. Mirrors the mutation in `scripts/resolve-pr-threads.sh` so the
/// Rust call path stays in lock-step with the prototype shell script.
const RESOLVE_REVIEW_THREAD_MUTATION: &str = "\nmutation($threadId: ID!) {\n  resolveReviewThread(input: {threadId: $threadId}) {\n    thread { id isResolved }\n  }\n}";

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
    let thread_arg = format!("threadId={thread_id}");
    let query_arg = format!("query={RESOLVE_REVIEW_THREAD_MUTATION}");
    let resp = match cmd_stdout(
        "gh",
        &["api", "graphql", "-F", &thread_arg, "-f", &query_arg],
    ) {
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
fn parse_review_threads(json: &str, bot_login: &str) -> Vec<ReviewThread> {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            log(&format!("WARNING: review-threads JSON parse failed: {e}"));
            return Vec::new();
        }
    };
    let nodes = v
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for thread in nodes {
        let resolved = thread
            .get("isResolved")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if resolved {
            continue;
        }
        let id = thread
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let comments = thread
            .pointer("/comments/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        let Some(c) = comments.first() else {
            continue;
        };
        let author = c
            .pointer("/author/login")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let is_bot = author == bot_login || author.ends_with("[bot]");
        if !is_bot {
            continue;
        }
        let path = c
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if path.is_empty() {
            continue;
        }
        // `line` can be null on outdated threads — fall back to originalLine
        // so we still anchor the finding somewhere meaningful in the prompt.
        let line = c
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| c.get("originalLine").and_then(serde_json::Value::as_u64))
            .unwrap_or(0) as u32;
        let body = c
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        out.push(ReviewThread {
            id,
            path,
            line,
            body,
            author,
        });
    }
    out
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
    let owner_repo = match cmd_stdout(
        "gh",
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ],
    ) {
        Some(s) if !s.is_empty() => s,
        _ => {
            log("WARNING: could not resolve owner/repo via `gh repo view`");
            return std::collections::HashMap::new();
        }
    };
    let (owner, repo) = match owner_repo.split_once('/') {
        Some((o, r)) => (o.to_string(), r.to_string()),
        None => {
            log(&format!(
                "WARNING: unexpected repo identifier '{owner_repo}'"
            ));
            return std::collections::HashMap::new();
        }
    };

    let query = "\nquery($owner: String!, $repo: String!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequests(states: OPEN, first: 100) {\n      nodes {\n        number\n        reviewThreads(first: 100) {\n          nodes {\n            isResolved\n            comments(first: 1) {\n              nodes {\n                author { login }\n              }\n            }\n          }\n        }\n      }\n    }\n  }\n}";

    let owner_arg = format!("owner={owner}");
    let repo_arg = format!("repo={repo}");
    let query_arg = format!("query={query}");

    let out = match cmd_stdout(
        "gh",
        &[
            "api", "graphql", "-F", &owner_arg, "-F", &repo_arg, "-f", &query_arg,
        ],
    ) {
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
/// the filter logic from [`parse_review_threads`] (resolved threads
/// dropped, only `bot_login` or `[bot]`-suffixed authors counted) so the
/// badge count and the Fix Comments agent see the same set of threads.
fn parse_pr_thread_counts(json: &str, bot_login: &str) -> std::collections::HashMap<u32, u32> {
    let mut counts = std::collections::HashMap::new();
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            log(&format!("WARNING: pr-thread-counts JSON parse failed: {e}"));
            return counts;
        }
    };
    let prs = v
        .pointer("/data/repository/pullRequests/nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    for pr in prs {
        let Some(number) = pr
            .get("number")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        let threads = pr
            .pointer("/reviewThreads/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        let mut count: u32 = 0;
        for t in threads {
            if t.get("isResolved")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let author = t
                .pointer("/comments/nodes/0/author/login")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if author == bot_login || author.ends_with("[bot]") {
                count += 1;
            }
        }
        if count > 0 {
            counts.insert(number, count);
        }
    }
    counts
}

/// Build the prompt for the Phase 2 Fix Comments agent run (#144).
///
/// The agent is launched with `cwd` set to a fresh git worktree on the PR's
/// head branch, so all file paths in the prompt and in the agent's edits are
/// relative to that worktree (not the user's main checkout). The dev process
/// commits and pushes the worktree after the agent run completes.
pub fn build_pr_review_fix_prompt(
    project_name: &str,
    pr_num: u32,
    pr_title: &str,
    branch: &str,
    diff: &str,
    threads: &[ReviewThread],
) -> String {
    let mut threads_section = String::new();
    for (i, t) in threads.iter().enumerate() {
        threads_section.push_str(&format!(
            "### Thread {i_num} — `{path}:{line}` (by @{author})\n\n{body}\n\n",
            i_num = i + 1,
            path = t.path,
            line = t.line,
            author = t.author,
            body = t.body,
        ));
    }
    let thread_count = threads.len();

    format!(
        r#"You are addressing review comments on pull request #{pr_num} for the {project_name} project.

Read AGENTS.md and skills/ for project conventions and coding standards.

## Working directory

Your current working directory is a freshly-created git worktree on branch `{branch}`. All file paths below are relative to this worktree. Do NOT `cd` elsewhere and do NOT run `git checkout` — the calling script handles branching and cleanup.

## Pull Request #{pr_num}: {pr_title}

### Diff
```diff
{diff}
```

## Unresolved Review Threads ({thread_count})

Address each thread below. The author of each thread is the project's review bot, so these are findings from an earlier automated code review pass.

{threads_section}
## Instructions

- For each thread, edit the file at the indicated path to address the finding. The line numbers refer to the **new** version of the file (the RIGHT side of the diff above).
- Stay focused: only fix what the threads call out. Do NOT refactor neighbouring code or rename unrelated symbols. The smaller the diff, the easier the next review.
- Do NOT run `cargo test`, `./scripts/test-examples.sh`, or any other workspace-wide validation inside this worktree. The worktree is throwaway, builds inside it are slow, and CI will validate the push. If you want to sanity-check your edit, re-`Read` the file to confirm the change applied — that is enough.
- Do NOT commit. Do NOT push. The calling script handles commit and push so it can clean up the worktree atomically.
- Do NOT post comments or reviews back to GitHub. The calling script handles that.

If a thread is ambiguous or you cannot determine the right fix without a human, leave the file unchanged for that thread and explain in your final summary which thread(s) you skipped and why."#
    )
}

pub fn build_sprint_planning_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"You are a sprint planning assistant for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## Current State

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

## Instructions

Produce a DRAFT sprint plan for the next development cycle:

0. **Read upstream recommendations.** The Strategic Review workflow publishes a single
   living issue labelled `strategic-review` whose body contains the **Recommended Path
   Forward** — the canonical list of candidate work items for sprint planning. Run
   `gh issue list --state open --label strategic-review --json number,title --limit 5` to
   find it, then `gh issue view <number>` to read its body. Treat the items in
   "Recommended Path Forward" as the primary input pool for this sprint plan; the open
   issues list below is supplementary context (in-flight work, leftover items, PRs).
1. **Analyse** — Review the strategic-review recommendations, open issues, open PRs, and completed work. Identify what is ready, what is blocked, and what has open review work.
2. **Prioritise** — Rank work items by impact and urgency. Consider dependencies.
3. **Dependencies** — Identify dependencies between work items. Assign each item a Layer number (0 = no dependencies, 1 = depends on layer-0 items, etc.). Items in the same layer can run in parallel.
4. **Group** — Organise items into a coherent sprint with clear goals.
5. **Estimate** — Provide rough sizing (S/M/L) for each item.
6. **Output** — Present the draft sprint plan with a Task Dependency Hierarchy table:

   | Issue | Depends On | Depended On By | Layer | Status |
   |-------|-----------|----------------|-------|--------|

   followed by a Markdown checklist with `- [ ] #N Title (blocked by #X, #Y)` entries.

If there are open PRs that should be merged before new work begins, call that out.

This is a DRAFT for human review. Do NOT create or modify any GitHub issues.
The human will provide feedback before the plan is finalised."#
    )
}

pub fn build_sprint_planning_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    format!(
        r#"You are a sprint planning assistant for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## Current State

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

## Human Feedback on the Draft

The human reviewed the draft sprint plan and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above and produce the FINAL sprint plan:

0. **Re-read upstream recommendations.** Sprint planning's primary input pool is the
   single open `strategic-review` issue's **Recommended Path Forward** section. Fetch it
   with `gh issue list --state open --label strategic-review --json number --limit 5`
   followed by `gh issue view <number>`. Pick from those recommendations; treat the open
   issues list above as supplementary context for in-flight work.
1. Adjust priorities, grouping, and scope based on the feedback.
2. Create GitHub issues for each work item using `gh issue create --title "..." --body "..."`.
   Do NOT include `Tracked by #<tracker>` yet — the tracker doesn't exist until step 3.
   The back-reference will be added by `gh issue edit` in step 4.
   **Ordering**: create all child issues first, collect their `#N` numbers, then create the tracker.
3. Create a GitHub tracker issue using:
   `gh issue create --title "Sprint: <goal>" --body "..." --label "sprint,tracker"`
   The tracker body must contain:
   - A Task Dependency Hierarchy table:

     | Issue | Depends On | Depended On By | Layer | Status |
     |-------|-----------|----------------|-------|--------|
     | #N Title | #X | #Y | 0 | 🔴 Not Started |

   - A checklist with `- [ ] #N Title (blocked by #X, #Y)` entries for each item.
4. Edit each child issue to add `Tracked by #<tracker>` in the body using
   `gh issue edit <child> --body "..."`.
5. Update ISSUES.md to add the new sprint's Task Dependency Hierarchy section. Keep existing completed sections intact.
6. Update STATUS.md if the sprint scope changes the status of any tracked feature.
7. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW."#
    )
}

fn strategic_review_context(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    format!(
        r#"## Project Context

### Crate Topology
{crate_tree}

### Recent Commits (last 30)
{recent_commits}

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status (STATUS.md)
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}"#
    )
}

const STRATEGIC_PERSPECTIVES: &str = r#"## Conduct the review from each perspective in turn:

### 1. Product Stakeholder
- What business value has been delivered so far?
- Where are the gaps between what exists and what users/operators need?
- What capabilities would unlock the most adoption or differentiation?
- Are there external pressures (compliance, market, ecosystem) to account for?

### 2. Business Analyst
- Are there missing user stories or acceptance criteria in open issues?
- Which requirements are implicit in the architecture but not tracked?
- What cross-cutting concerns (observability, documentation, onboarding) are under-specified?
- Draft 3-5 concrete user stories for the highest-priority gap.

### 3. Lead Engineer
- What technical debt is accumulating? Where are the architectural risks?
- Are there scalability bottlenecks or single points of failure?
- Which "Future Enhancements" listed in ISSUES.md are now urgent vs. deferrable?
- What refactoring would pay dividends across multiple future features?
- Review open PRs — are any stale, conflicting, or blocking other work?

### 4. UX / DX Researcher
- How is the developer experience for someone deploying their first app?
- What friction exists in the CLI, the manifest format, or the error messages?
- Are logs, status output, and diagnostics actionable?
- What documentation or examples are missing?"#;

#[allow(clippy::too_many_arguments)]
pub fn build_strategic_review_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    report_synthesis: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let synthesis_section = if report_synthesis.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Report Synthesis (from GitHub issue labelled `uxr-synthesis`)

The most recent UXR Synth phase produced the following synthesis (fetched from the
open `uxr-synthesis` GitHub issue). Use it as a starting point — validate, challenge,
or build on its findings. Reference the synthesis issue number when creating downstream
issues so they link back via `Depends On #<synthesis>`.

{report_synthesis}

---
"#
        )
    };
    format!(
        r#"You are a strategic review board for the {project_name} project. You will conduct a
multi-perspective analysis, role-playing the viewpoints that typically drive a product
forward, then synthesise a unified recommendation.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{synthesis_section}
{STRATEGIC_PERSPECTIVES}

---

## Synthesis

After completing all four perspectives, produce:

1. **Unified Assessment** — A 2-3 paragraph summary of where the project stands and what matters most.
2. **Recommended Path Forward** — An ordered list of 5-10 work items, each with:
   - Title (a clear, actionable headline — these are recommendation entries inside the
     single strategic-review issue body, NOT separate GitHub issues)
   - Perspective(s) driving it (Stakeholder / BA / Engineering / DX)
   - Sizing (S / M / L)
   - Brief rationale
3. **Risks & Watch Items** — Anything that could derail progress if ignored.

The finalized strategic review will be published as **exactly one** GitHub issue carrying
the `strategic-review` label — a single living strategic-direction artifact. Do not
propose a parent-tracker / child-issue layout; the recommended path forward lives as a
section inside that one issue, not as separate trackable work items. Sprint planning
consumes its own workflow and will turn these recommendations into trackable sprint
issues at that stage.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the analysis, adjust priorities, add context, or redirect focus.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_strategic_review_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    report_synthesis: &str,
    feedback: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let synthesis_section = if report_synthesis.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Report Synthesis (from GitHub issue labelled `uxr-synthesis`)

{report_synthesis}

The single strategic-review issue body MUST include
`Depends On #<synthesis-issue-number>` so it links back to the synthesis.

---
"#
        )
    };
    format!(
        r#"You are a strategic review board for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{synthesis_section}
## Human Feedback

The human reviewed the draft strategic analysis and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Adjust the recommended path forward — reprioritise,
add, remove, or reshape work items as directed.

Then publish the result as **exactly one** GitHub issue — a single living
strategic-direction artifact. Do NOT create child or recommendation issues; the
recommended path forward belongs as a section inside this single issue's body, not as
separate trackable work items. Sprint planning consumes its own workflow and will turn
these recommendations into trackable sprint issues at that stage; the strategic review
must not percolate into sprint planning as discrete tickets.

1. **Find or create the strategic review issue.** Run
   `gh issue list --state open --label "{strategic_label}" --json number,title --limit 5`
   to see if an open strategic-review issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the strategic review
     remains a single living document.
   - If none exists, create one with
     `gh issue create --title "Strategic Review: <YYYY-MM-DD> — <unified-assessment-headline>" --label "{strategic_label}"`.
     Use only the `{strategic_label}` label — do NOT add `{tracker_label}` or any
     sprint/area labels, since this issue is a strategic-direction artifact, not
     schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Unified Assessment** — Updated 2-3 paragraph summary reflecting the feedback.
   - **Recommended Path Forward** — Ordered list of 5-10 work items, each as a sub-section
     (NOT as `#N` issue refs) with: Title, Perspective(s) driving it, Sizing (S/M/L),
     Rationale, and Acceptance Criteria. These are recommendation entries, not tickets.
   - **Risks & Watch Items** — Updated risks.
   - **Dependencies** — `Depends On #<synthesis-issue-number>` linking back to the UXR
     Synthesis issue this review was built from (if one exists).
   - **Last Updated** — today's date.

3. **Do not file recommendation issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Sprint Planning.

4. **Update ISSUES.md** — Reference the single strategic-review issue. Do NOT add a
   per-recommendation Task Dependency Hierarchy here — that lives in sprint planning.
5. **Update STATUS.md** — If any new capability is being tracked, add or update the
   relevant rows.
6. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW.

This output closes the feedback loop: sprint planning will read this single issue's
"Recommended Path Forward" section and turn the items it picks into trackable sprint
issues at that stage."#,
        project_name = project_name,
        context = context,
        synthesis_section = synthesis_section,
        feedback = feedback,
        strategic_label = labels::STRATEGIC_REVIEW,
        tracker_label = labels::TRACKER,
    )
}

const ROADMAP_PHASES: &str = r#"## Create a long-term Roadmap based on the Strategic Review:

### Phase 1: Foundation (Next 1-2 Sprints)
- What critical blockers or technical debt must be addressed immediately?
- Which core features need stabilization before further expansion?

### Phase 2: Expansion (Next 2-4 Sprints)
- What primary capabilities will unlock new user segments or use cases?
- How will the system scale to handle increased load or node types?

### Phase 3: Ecosystem (Future)
- How will Freq Cloud integrate with external systems, clouds, or developer tools?
- What are the long-term extensibility and sustainability goals?"#;

#[allow(clippy::too_many_arguments)]
pub fn build_roadmapper_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    strategic_review: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let strategic_section = if strategic_review.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Strategic Review Recommendation (from GitHub issue labelled `strategic-review`)

The most recent Strategic Review produced the following analysis and recommendations (fetched from the
open `strategic-review` GitHub issue). Use it as the primary input for the Roadmap.

{strategic_review}

---
"#
        )
    };
    format!(
        r#"You are the Roadmapper for the {project_name} project. Your goal is to transform strategic
intent into a structured, long-term roadmap.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{strategic_section}
{ROADMAP_PHASES}

---

## Roadmap Output

Produce a structured roadmap that includes:

1. **Strategic Intent** — A brief (1-2 paragraph) vision statement for the next several months.
2. **Milestone Phases** — For each of the three phases defined above, provide:
   - Goals & Outcomes
   - 3-5 high-level initiatives (as a bulleted list — these are NOT separate GitHub issues,
     they are sections of the single roadmap document)
   - Success metrics

The finalized roadmap will be published as **exactly one** GitHub issue carrying the
`roadmap` label — a single common operating picture for management forecasting. Do not
propose a parent-tracker / child-issue layout; phases and initiatives live inside the
body of that one issue, not as separate trackable work items.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the roadmap, adjust timelines, and refine initiatives.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_roadmapper_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    strategic_review: &str,
    feedback: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let strategic_section = if strategic_review.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Strategic Review Recommendation (from GitHub issue labelled `strategic-review`)

{strategic_review}

---
"#
        )
    };
    format!(
        r#"You are the Roadmapper for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{strategic_section}

## Human Feedback

Incorporating this feedback into the final roadmap:
{feedback}

---

## Final Roadmap Execution

Your final task is to publish the roadmap as **exactly one** GitHub issue — a single
"common operating picture" for management forecasting. Do NOT create child or initiative
issues; phases and initiatives belong as sections inside this single issue's body, not as
separate trackable work items. Sprint planning consumes its own workflow; the roadmap must
not percolate into sprint planning as discrete tickets.

1. **Find or create the roadmap issue.** Run
   `gh issue list --state open --label "{roadmap_label}" --json number,title --limit 5`
   to see if an open roadmap issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the roadmap remains a
     single living document.
   - If none exists, create one with
     `gh issue create --title "Roadmap: <YYYY-MM-DD> — <headline>" --label "{roadmap_label}"`.
     Use only the `{roadmap_label}` label — do NOT add `{tracker_label}` or any sprint/area
     labels, since this issue is a strategic artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Strategic Intent** — 1-2 paragraph vision statement.
   - **Phase 1: Foundation**, **Phase 2: Expansion**, **Phase 3: Ecosystem** — each with
     Goals & Outcomes, the 3-5 initiatives as a bulleted list (NOT as `#N` issue refs),
     and Success Metrics.
   - **Dependencies** — `Depends On #<strategic-review-number>` linking back to the
     Strategic Review issue this roadmap was built from.
   - **Last Updated** — today's date.

3. **Do not file initiative issues, do not file a parent tracker issue, do not edit any
   other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Strategic Review and Sprint Planning.

Use a clear, evocative title and a structured, scannable body."#,
        project_name = project_name,
        context = context,
        strategic_section = strategic_section,
        feedback = feedback,
        tracker_label = labels::TRACKER,
        roadmap_label = labels::ROADMAP,
    )
}

pub fn build_ideation_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    format!(
        r#"You are an ideation partner for the {project_name} project. Your job is to generate
a wide, varied set of raw ideas — not to evaluate, prioritise, or structure them.
Aim for quantity and variety over quality.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Instructions

Produce at least 15 distinct ideas across these buckets:

### Capability ideas
Features users would notice — new APIs, CLI commands, dashboard panels, deployment
targets, developer workflows, or integrations.

### Foundational ideas
Infrastructure, refactors, dev-experience improvements — things that make the system
faster, more reliable, easier to develop, or cheaper to operate.

### Provocations
"What if we did the opposite?", "What if we deleted X?", contrarian or uncomfortable
ideas that challenge assumptions. These should make the reader pause.

### Wildcards
Half-formed hypotheses, analogies from other systems, things you'd normally dismiss.
Connections between unrelated domains, speculative features, "wouldn't it be cool if…"
thoughts.

## Format

For each idea: one-sentence description, one-sentence rationale.
No sizing, no commitment, no ranking. Do **not** create GitHub issues.
Do **not** filter or evaluate ideas — the human will react in feedback.

This is a DRAFT for human review. The human will keep some ideas, drop others,
expand on a few, and provide feedback before anything is finalised."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_ideation_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    feedback: &str,
    dry_run: bool,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let dry_run_note = if dry_run {
        "\n\n**DRY RUN MODE**: Do NOT actually run any `gh` commands. Instead, print the \
         exact commands you WOULD run (gh issue list, gh issue close, gh issue create) \
         with their full arguments, so the human can review what would be filed."
    } else {
        ""
    };
    format!(
        r#"You are an ideation partner for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Human Feedback

The human reviewed the ideation draft and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Keep the ideas the human endorsed, drop the ones they
rejected, and expand on any they flagged for elaboration. You may add new ideas if the
feedback suggests directions not yet covered.

Produce the FINAL ideation set, organised by bucket (Capability / Foundational /
Provocations / Wildcards). For each surviving idea: one-sentence description,
one-sentence rationale, and (if the human requested it) a short expansion paragraph.

## Publishing the Ideation as a GitHub Issue

After completing the final ideation set, publish it as a GitHub issue so it is
reviewable, durable, and consumable by downstream workflows (UXR Synth, Strategic
Review).
{dry_run_note}

### Step 1 — Close any prior open ideation issues

Run:
```
gh issue list --label ideation --state open --json number --jq '.[].number'
```

For each open issue number returned, close it with a superseded comment:
```
gh issue close <NUMBER> --comment "Superseded by the new ideation issue."
```

### Step 2 — Create the new ideation issue

Run:
```
gh issue create \
  --title "Ideation: <YYYY-MM-DD> — <one-line headline>" \
  --body "<full ideation set with all buckets and surviving ideas, plus a footer: 'Generated by Ideation agent run on <YYYY-MM-DD>.'>" \
  --label "ideation"
```

Use today's date for `<YYYY-MM-DD>`. The title headline should capture the overall
theme of the surviving ideas. The body must contain the complete final ideation set.

### Step 3 — Update the superseded comments

Go back to each issue you closed in Step 1 and update the close comment to include the
new issue number: "Superseded by #<new>."

### Step 4 — Echo the issue URL

After creating the issue, print the issue URL so it appears in the editor panel output.
Format: `Ideation published: <URL>`

Do NOT write any files to disk — the GitHub issue IS the artifact."#
    )
}

fn report_persona_lens_section(skill_paths: &crate::agent::types::SkillPaths) -> String {
    format!(
        r#"## Synthesis Lens — User Personas

Before producing any analysis, load `{skill_path}`.
This skill describes users of the platform, not contributors to the project
itself. Do NOT conflate it with other skills such as architecture,
coding standards, issue tracking, or project context, which are about building the
platform rather than using it.

For sections 2-6, tag each evidence item to the single closest persona by matching
`recognition_cues:`. Weight each finding against that persona's `jobs_to_be_done:`,
`pains:`, `adoption_yes_if:`, `rejection_no_if:`, and `anti_goals:`. If a piece of
signal matches no persona cleanly, surface it in section 7 as a possible persona blind
spot instead of forcing a weak fit.
"#,
        skill_path = skill_paths.user_personas,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_report_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    ideation: &str,
    skill_paths: &crate::agent::types::SkillPaths,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let ideation_section = if ideation.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Ideation (from GitHub issue labelled `ideation`)

The most recent Ideation phase produced the following raw ideas (fetched from the
open `ideation` GitHub issue). Use this as upstream input — **converge** these ideas
into the structured report. Pick the strongest threads, discard the noise, and explain
your filtering rationale in the Executive Summary or Recommended Next Actions.

{ideation}

---
"#
        )
    };
    let persona_lens_section = report_persona_lens_section(skill_paths);
    format!(
        r#"You are a project analyst for the {project_name} project. Produce a concise
**Strategic Report** summarising current state, progress, and recommended next actions.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{ideation_section}
{persona_lens_section}
## Report Structure

Produce the report with these sections:

### 1. Executive Summary
2-3 sentences on overall project health and momentum.

### 2. Progress Since Last Review
- What has shipped (recent commits, merged PRs)?
- Which issues were closed?
- Velocity trend: accelerating, steady, or slowing?

### 3. Current Sprint Status
- How many issues are open vs completed on active trackers?
- What percentage of the sprint is done?
- Any issues that are overdue or stalled?

### 4. Blockers & Dependencies
- Which issues are blocked and by what?
- Are there dependency chains that could cascade delays?
- External blockers (tooling, infrastructure, reviews)?

### 5. Risk Assessment
For each risk, rate severity (High/Medium/Low) and likelihood:
- Technical risks (architecture, scalability, debt)
- Delivery risks (scope creep, resource, timeline)
- Quality risks (test coverage, error handling, security)

### 6. Recommended Next Actions
Ordered list of 3-5 concrete actions with rationale. Each should be:
- Actionable within the current sprint
- Tied to a specific issue or gap identified above

### 7. Open Questions
Items that need human decision-making or clarification.

Keep the report factual and data-driven. Reference specific issue numbers and PRs.
Do NOT create any GitHub issues — this is a DRAFT for human review.
The human will review the report, adjust emphasis, correct misreadings, or add context.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_report_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    ideation: &str,
    feedback: &str,
    dry_run: bool,
    skill_paths: &crate::agent::types::SkillPaths,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let dry_run_note = if dry_run {
        "\n\n**DRY RUN MODE**: Do NOT actually run any `gh` commands. Instead, print the \
         exact commands you WOULD run (gh issue list, gh issue close, gh issue create) \
         with their full arguments, so the human can review what would be filed."
    } else {
        ""
    };
    let ideation_section = if ideation.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Ideation (from GitHub issue labelled `ideation`)

{ideation}

When producing the synthesis, reference the strongest ideation threads and explain
which were kept and which were filtered out, and why.

---
"#
        )
    };
    let persona_lens_section = report_persona_lens_section(skill_paths);
    format!(
        r#"You are a project analyst for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{ideation_section}
{persona_lens_section}
## Human Feedback

The human reviewed the draft report and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Adjust the report — correct any misreadings,
shift emphasis, add missing context, or reshape sections as directed.

Then produce the FINAL report with these sections:

1. **Executive Summary** — Updated to reflect the feedback.
2. **Progress Since Last Review** — Adjusted findings.
3. **Current Sprint Status** — Corrected if needed.
4. **Blockers & Dependencies** — Updated.
5. **Risk Assessment** — Re-rated if directed.
6. **Recommended Next Actions** — Reprioritised per feedback.
7. **Open Questions** — Updated.

After the full report, produce a **## Synthesis** section that distils the report into
a compact briefing suitable for feeding directly into a Strategic Review. This synthesis
should contain:
- The top 3-5 priorities with brief rationale
- Key risks and blockers that must inform strategic decisions
- Velocity assessment (one line)
- Visible persona attribution: name the dominant persona signal this cycle and call out
  any persona that appeared in zero evidence as a possible blind spot

## Publishing the Synthesis as a GitHub Issue

After completing the report, publish it as a GitHub issue so it is reviewable, durable,
and consumable by downstream workflows (Strategic Review, Sprint Planning).
{dry_run_note}

### Step 1 — Capture the list of prior open synthesis issues

Run:
```
gh issue list --label uxr-synthesis --state open --json number --jq '.[].number'
```

Save the list of issue numbers — you'll close them in Step 3 with a back-reference.

### Step 2 — Create the new synthesis issue

Run:
```
gh issue create \
  --title "UXR Synthesis: <YYYY-MM-DD> — <one-line headline>" \
  --body "<full report body including sections 1-7 and the ## Synthesis block, plus a footer: 'Generated by UXR Synth agent run on <YYYY-MM-DD>.'>" \
  --label "uxr-synthesis"
```

Use today's date for `<YYYY-MM-DD>`. The title headline should capture the single most
important finding. The body must contain the complete final report (sections 1–7) and
the Synthesis block. Capture the new issue number (`#<new>`) from the URL output.

### Step 3 — Close prior synthesis issues with a back-reference

For each issue number captured in Step 1, close it with a single comment that already
includes the new issue number — no follow-up edit needed:
```
gh issue close <NUMBER> --comment "Superseded by #<new>."
```

### Step 4 — Echo the issue URL

After creating the issue, print the issue URL so it appears in the editor panel output.
Format: `UXR synthesis published: <URL>`

Do NOT write any files to disk — the GitHub issue IS the artifact."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_retrospective_draft_prompt(
    project_name: &str,
    recent_commits: &str,
    closed_issues: &str,
    merged_prs: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"You are a sprint retrospective facilitator for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## What Happened This Cycle

### Recent Commits (last 50)
{recent_commits}

### Recently Closed Issues
{closed_issues}

### Recently Merged PRs
{merged_prs}

### Still Open Issues
{open_issues}

### Still Open PRs
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

---

## Conduct the retrospective:

### 1. What shipped
- Summarise the features, fixes, and improvements that landed.
- Were the sprint goals met? What was left incomplete and why?

### 2. What went well
- Which patterns, tools, or approaches produced good results?
- Were there any wins worth repeating (clean merges, good test coverage, fast turnarounds)?

### 3. What was painful
- Where did the process break down? Flaky tests, merge conflicts, unclear requirements?
- Were there bottlenecks — blocked issues, stale PRs, missing context?
- Did any implemented work need immediate follow-up fixes?

### 4. What to change
- Concrete process improvements for the next cycle.
- Are there recurring problems that need a systemic fix (tooling, documentation, conventions)?
- Should the sprint size, scope, or structure change?

### 5. Velocity & health
- Rough throughput: how many issues closed vs. opened?
- Is the open issue/PR backlog growing, shrinking, or stable?
- Any signs of tech debt accumulating faster than it's being addressed?

---

## Output

Produce a structured retrospective report with the five sections above.

The finalized retrospective will be published as **exactly one** GitHub issue carrying
the `retrospective` label — a single living retrospective artifact for this cycle. Do
not propose a one-issue-per-action-item layout; action items live as a checklist inside
the body of that one issue, not as separate trackable work items, so the retro does not
percolate into sprint planning as discrete tickets.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will add their own observations, correct misreadings, and highlight what matters most."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_retrospective_finalize_prompt(
    project_name: &str,
    recent_commits: &str,
    closed_issues: &str,
    merged_prs: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    format!(
        r#"You are a sprint retrospective facilitator for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## What Happened This Cycle

### Recent Commits (last 50)
{recent_commits}

### Recently Closed Issues
{closed_issues}

### Recently Merged PRs
{merged_prs}

### Still Open Issues
{open_issues}

### Still Open PRs
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

---

## Human Feedback on the Draft

The human reviewed the draft retrospective and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback. Adjust the retrospective findings and recommendations accordingly.

Then produce the FINAL output as **exactly one** GitHub issue — a single living
retrospective artifact for this cycle. Do NOT create one issue per action item; action
items live as a checklist inside the body of this single issue, not as separate trackable
work items. Sprint planning consumes its own workflow; the retrospective must not
percolate into sprint planning as discrete tickets.

1. **Find or create the retrospective issue.** Run
   `gh issue list --state open --label "{retro_label}" --json number,title --limit 5`
   to see if an open retrospective issue already exists for the current cycle.
   - If one exists for this cycle, **edit it in place** with
     `gh issue edit <number> --body-file -` (or `--title` if the headline changed). Reuse
     the same issue so the retro remains a single living document for the cycle.
   - If none exists, create one with
     `gh issue create --title "Retro: <YYYY-MM-DD> — <headline>" --label "{retro_label}"`.
     Use only the `{retro_label}` label — do NOT add `{tracker_label}` or any sprint/area
     labels, since this issue is a reflective artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Retrospective Report** — the five sections (What shipped, What went well, What was
     painful, What to change, Velocity & health), updated with the human's corrections
     and observations.
   - **Action Items** — a markdown checklist (`- [ ] ...`) of small, concrete process
     improvements and follow-ups, each with a one-line "definition of done". These are
     checklist items, NOT separate `#N` issue refs.
   - **Last Updated** — today's date.

3. **Do not file per-action-item issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Strategic Review and Sprint Planning.

4. **Update ISSUES.md** — Mark completed issues as ✅ Done in the Task Dependency
   Hierarchy tables. Reference the single retro issue, not per-item children.
5. **Update STATUS.md** — Reflect any status changes from the completed sprint work.
6. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW.

The action items inside this single issue feed directly into the next strategic review
and sprint planning cycle."#,
        project_name = project_name,
        recent_commits = recent_commits,
        closed_issues = closed_issues,
        merged_prs = merged_prs,
        open_issues = open_issues,
        open_prs = open_prs,
        status = status,
        issues_md = issues_md,
        feedback = feedback,
        retro_label = labels::RETROSPECTIVE,
        tracker_label = labels::TRACKER,
    )
}

pub fn build_code_review_prompt(
    project_name: &str,
    pr_num: u32,
    title: &str,
    body: &str,
    diff: &str,
) -> String {
    format!(
        r#"You are a code reviewer for the {project_name} project.

Read AGENTS.md and skills/ for project conventions and coding standards.

## Pull Request #{pr_num}: {title}

### Description
{body}

### Diff
```diff
{diff}
```

## Review Dimensions

1. **Correctness** — Does the code do what the PR claims? Logic errors?
2. **Security** — OWASP top 10, unsafe code, command injection, path traversal.
3. **Performance** — Unnecessary allocations, blocking in async, O(n²) where O(n) is possible.
4. **Style** — Consistency with project conventions in skills/.
5. **Tests** — Adequately tested? Edge cases covered?
6. **Memory** — Idle memory under 10MB — flag any unnecessary allocations.

For each finding, capture:
- `path` — file relative to repo root
- `line` — line number in the **new** version of the file (RIGHT side of the diff)
- `severity` — critical / warning / nit
- `body` — markdown explanation with the suggested fix, prefixed with the severity tag (e.g. `**[warning]** ...`)

## Posting the Review

Submit the review as a **single** REST call via `gh api`. This posts the
verdict and all inline comments atomically. Do NOT use `gh pr review` —
it cannot attach line-anchored comments, and that is the whole point of
this workflow.

### Step 1 — Resolve repo + head SHA
```sh
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
HEAD_SHA=$(gh pr view {pr_num} --json headRefOid -q .headRefOid)
```

### Step 2 — Pick the verdict
- **APPROVE** — zero findings worth flagging.
- **REQUEST_CHANGES** — at least one critical or warning finding.
- **COMMENT** — only nits, or non-actionable observations.

Your `gh` commands run under the bot account identity (GH_TOKEN is set
for you), so same-author rules do not apply. Do NOT downgrade
REQUEST_CHANGES to COMMENT to work around restrictions you do not have.

### Step 3 — Build the payload and POST it
Write the JSON to a temp file, then submit it via `--input`:

```sh
cat > /tmp/review-{pr_num}.json <<'JSON'
{{
  "commit_id": "<HEAD_SHA>",
  "event": "APPROVE | REQUEST_CHANGES | COMMENT",
  "body": "<top-level summary, 1-3 sentences>",
  "comments": [
    {{
      "path": "<file path>",
      "line": <new-version line number>,
      "side": "RIGHT",
      "body": "**[severity]** <markdown finding with suggested fix>"
    }}
  ]
}}
JSON

gh api -X POST \
  -H "Accept: application/vnd.github+json" \
  "repos/$REPO/pulls/{pr_num}/reviews" \
  --input /tmp/review-{pr_num}.json
```

Constraints the API enforces — respect them or you will get HTTP 422:
- `line` MUST refer to a line that is actually present in the diff hunks
  for that file. Anchoring to an unchanged line outside the hunks is rejected.
- For multi-line findings, include both `start_line` and `line`, both
  using `side: "RIGHT"`.
- An empty `comments` array is allowed — that turns the call into a
  verdict-only review (use this for a clean APPROVE).
- If `gh api` returns non-2xx, surface the response body verbatim and
  stop. Do not retry blindly and do not fall back to `gh pr review`.

### Step 4 — Confirm and stop
On success, log the `html_url` from the response and exit. Do NOT also
run `gh pr review` or `gh pr comment` — the inline comments and verdict
are already posted in the single call above."#
    )
}

pub fn build_security_review_prompt(
    project_name: &str,
    crate_tree: &str,
    snapshot: &str,
    dry_run: bool,
) -> String {
    let snapshot_section = if snapshot.is_empty() {
        "Read the codebase directly using the tools available to you. Start with AGENTS.md, \
         skills/, then systematically review each crate under crates/."
            .to_string()
    } else {
        format!(
            "## Codebase Snapshot\n\n\
             The following is a cleaned snapshot of the project. Use this as your primary reference.\n\n\
             {snapshot}"
        )
    };

    format!(
        r#"You are a security auditor performing a comprehensive security-focused code review
of the {project_name} project.

Read AGENTS.md and skills/ for full project context and coding standards.

## Project Crates
```
{crate_tree}
```

{snapshot_section}

---

## Security Review Scope

Perform a thorough static security analysis covering ALL of the following areas:

### 1. OWASP Top 10
- **Injection** — SQL injection, command injection, code injection in any ops or handlers.
- **XSS** — Cross-site scripting in any HTML output, SSE streams, or dashboard rendering.
- **SSRF** — Server-side request forgery in fetch ops, proxy handlers, or service discovery.
- **Path traversal** — Verify all filesystem operations reject `..`, leading `/`, null bytes.
- **Broken authentication** — Weak token generation, missing auth checks on endpoints.
- **Broken access control** — RBAC bypass, privilege escalation, missing authorization gates.
- **Security misconfiguration** — Default credentials, overly permissive CORS, debug endpoints.
- **Insecure deserialization** — Unsafe deserialization of untrusted input.
- **Insufficient logging** — Missing audit trails for security-sensitive operations.
- **CSRF** — Missing CSRF protections on state-changing endpoints.

### 2. Authentication & Authorization
- Are all management API endpoints properly gated (API key, RBAC)?
- Is session/token handling secure (timing-safe comparison, proper expiry)?
- Are password hashing parameters adequate (Argon2id tuning)?
- Any endpoints reachable without authentication that should require it?

### 3. Secrets Handling
- Hardcoded keys, tokens, or credentials anywhere in the codebase?
- Are secrets properly encrypted at rest (ChaCha20-Poly1305)?
- Could secrets leak into logs, error messages, or SSE streams?
- Is `ANTHROPIC_API_KEY` / other API keys handled safely?

### 4. Sandbox Escape Vectors
- **V8 isolate** — Can tenant code escape the sandbox via deno_core ops?
- **Filesystem** — Are read/write ops properly scoped to tenant directories?
- **Network** — Can tenant code access internal services or private IPs?
- **Environment** — Can tenant code read host environment variables beyond allowed scope?
- **Resource exhaustion** — Can a tenant exhaust memory, CPU, disk, or file descriptors?

### 5. Wire Protocol Weaknesses
- **Replay attacks** — Is nonce-based replay protection correctly implemented?
- **Spoofing** — Can a node impersonate another in the mesh?
- **DoS** — Can malformed or oversized messages crash a node?
- **Key exchange** — Is the ML-KEM-768 handshake correctly implemented?
- **Message integrity** — Are all messages authenticated (AEAD)?

### 6. Dependency Vulnerabilities
- Review `Cargo.toml` for known vulnerable crate versions.
- Check for dependencies with known CVEs.
- Flag any unnecessary dependencies that expand the attack surface.

### 7. Unsafe Rust Usage
- Audit all `unsafe` blocks for soundness.
- Check for undefined behavior, data races, or memory corruption.
- Verify safety invariants are documented and upheld.

---

## Output Format

For each finding, produce a structured entry:

### [SEVERITY] Title
- **Severity**: Critical / High / Medium / Low / Informational
- **Category**: (e.g., OWASP-A01, Sandbox Escape, Wire Protocol, etc.)
- **Location**: `crate/file.rs:line_range`
- **Description**: What the vulnerability is and how it could be exploited.
- **Impact**: What an attacker could achieve.
- **Remediation**: Specific code changes or mitigations to fix it.

---

## Summary

After all findings, produce:

1. **Executive Summary** — 2-3 sentences on overall security posture.
2. **Finding Count** — Table of findings by severity (Critical/High/Medium/Low/Info).
3. **Top 3 Priority Fixes** — The most impactful issues to address first.
4. **Positive Observations** — Security practices that are already well-implemented.

Be thorough but avoid false positives. Only flag real, actionable issues.

## Issue Creation

After the review is complete, file the results as GitHub issues:

### Duplicate Detection

Before creating any issue, check for an existing open issue with the same title:
```
gh issue list --label security --search "<finding title>" --state open
```
If a matching open issue already exists, skip creating it and note "Already tracked: #<N>" in the summary.

### Actionable Findings (Critical / High / Medium)

For each actionable finding (Critical, High, or Medium severity), create a GitHub issue with a severity label:
```
gh issue create \
  --title "security: [SEVERITY] <finding title>" \
  --body "<severity, category, location, description, impact, remediation>" \
  --label "security,code-review,severity:<severity_lowercase>"
```
Where `severity:<severity_lowercase>` is one of `severity:critical`, `severity:high`, or `severity:medium`.

**Ordering**: create all per-finding issues first, collect their `#N` numbers, then create the tracker.

### Low / Informational Findings

Low and Informational findings should be batched into a single rollup issue:
```
gh issue create \
  --title "security: Low/Info findings rollup — <YYYY-MM-DD>" \
  --body "<list of all Low and Info findings with severity, category, location, description>" \
  --label "security,code-review,severity:low"
```
If there are no Low/Info findings, skip this step.

### Tracker Issue

After all finding issues (including the rollup) are created, create a tracker issue:
```
gh issue create \
  --title "Security Review: <YYYY-MM-DD> — <executive-summary-headline>" \
  --body "..." \
  --label "security,tracker"
```
The tracker body must contain:
- The executive summary
- The finding count table (Critical/High/Medium/Low/Info)
- A checklist with `- [ ] #N <finding title>` entries for each child issue (including the rollup)
- The top 3 priority fixes

### Link Children to Tracker

Edit each child issue to add `Tracked by #<tracker>` in the body using `gh issue edit <child> --body "..."`.

### Cross-Reference Summary

After all issues are filed, output a final summary line in this exact format:
```
Filed: #<N1>, #<N2>, ... (tracker: #<T>)
```
This allows the human reviewer to audit the created issues at a glance.{dry_run_section}"#,
        dry_run_section = if dry_run {
            "\n\n## DRY RUN MODE\n\n\
             **IMPORTANT**: This is a dry-run. Do NOT execute any `gh issue create` or `gh issue edit` commands.\n\
             Instead, for each issue you would create, output the full `gh` command you would have run, prefixed with `[dry-run]`.\n\
             Still perform the full security analysis and duplicate detection, but only print what would be filed."
        } else {
            ""
        }
    )
}

pub fn build_refresh_agents_prompt(project_name: &str, agent_files: &[String]) -> String {
    let file_list = if agent_files.is_empty() {
        "- `AGENTS.md`".to_string()
    } else {
        agent_files
            .iter()
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"You are refreshing agent-facing documentation for the {project_name} project.

Read AGENTS.md and the listed skill files before making any edits.

## Allowed Files

You may edit ONLY the following existing agent-facing files:
{file_list}

Do NOT edit source code, tests, Cargo manifests, README files, STATUS.md, ISSUES.md, or any
other project files. Do NOT create new skills from scratch.

## Refresh Objective

Review each allowed file against the current repository state and update only documentation drift.
For every stale claim you change, confirm the real repo state first.

At a minimum, verify:
- referenced file paths still exist
- referenced scripts still exist and still match the described purpose
- referenced crates, macros, ops, and APIs still exist with the described names/shapes
- referenced docs still exist and the cited filenames/sections still line up

## Execution Rules

1. Inspect the repo directly with read-only commands before editing.
2. Edit only the allowed files above.
3. Do NOT commit, push, or open a pull request. The shell will handle git and PR creation.
4. If nothing drifted, leave the worktree unchanged.

## Final Output Contract

After you finish, output exactly one of the following:

### If you made no edits
`REFRESH_AGENTS_NO_CHANGES`

### If you edited files
Emit this exact block:

```
REFRESH_AGENTS_SUMMARY_BEGIN
path/to/file | one-line reason for the edit
path/to/other/file | one-line reason for the edit
REFRESH_AGENTS_SUMMARY_END
```

Requirements for each reason:
- one line only
- describe the specific drift you corrected
- mention the repo fact that forced the update

Do not include files outside the allowed list in that summary block."#
    )
}

// ── Refresh Docs (one-shot) ──

pub fn build_refresh_docs_prompt(project_name: &str, doc_files: &[String]) -> String {
    let file_list = if doc_files.is_empty() {
        "- `README.md`".to_string()
    } else {
        doc_files
            .iter()
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"You are refreshing project documentation for the {project_name} project.

Read the listed files and compare each claim against the current repository state.

## Allowed Files

You may edit ONLY the following existing project documentation files:
{file_list}

Do NOT edit source code, tests, Cargo manifests, agent-facing files (AGENTS.md, skills/**,
CLAUDE.md, CLINE.md, GEMINI.md, COPILOT.md, GROK.md, JUNIE.md, XAI.md), or any other non-documentation files.

## Refresh Objective

Review each allowed file against the current repository state and update only documentation drift.
For every stale claim you change, confirm the real repo state first.

At a minimum, verify:
- referenced crates, binaries, and scripts still exist with the described shapes
- code snippets and command examples still compile / run
- feature lists and architectural descriptions match the current node types and crate layout
- STATUS.md and ISSUES.md reflect the current state of trackers and open work
  (tracker parity: every documented tracker should match what `gh issue list --label tracker` returns)
- the documented Dev UI workflow inventory matches the actual shipped workflows in
  `crates/dev/src/agent/types.rs::Workflow` and the sidebar buttons in
  `crates/dev/src/ui/sidebar.rs` — no missing workflows, no leftover renamed names

## Execution Rules

1. Inspect the repo directly with read-only commands before editing.
2. Edit only the allowed files above.
3. Do NOT commit, push, or open a pull request. The shell will handle git and PR creation.
4. If nothing drifted, leave the worktree unchanged.

## Final Output Contract

After you finish, output exactly one of the following:

### If you made no edits
`REFRESH_DOCS_NO_CHANGES`

### If you edited files
Emit this exact block:

```
REFRESH_DOCS_SUMMARY_BEGIN
path/to/file | one-line reason for the edit
path/to/other/file | one-line reason for the edit
REFRESH_DOCS_SUMMARY_END
```

Requirements for each reason:
- one line only
- describe the specific drift you corrected
- mention the repo fact that forced the update

Do not include files outside the allowed list in that summary block."#
    )
}

// ── Housekeeping (two-phase) ──

/// Build context string from pre-gathered housekeeping data.
fn housekeeping_context(
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"## Project Context

### Open Issues (JSON)
{open_issues}

### Open Pull Requests (JSON)
{open_prs}

### Local Branches
{local_branches}

### Tracker Issue Bodies
{tracker_bodies}

### STATUS.md
{status}

### ISSUES.md
{issues_md}"#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_housekeeping_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
) -> String {
    let context = housekeeping_context(
        open_issues,
        open_prs,
        local_branches,
        tracker_bodies,
        status,
        issues_md,
    );
    format!(
        r#"You are a housekeeping agent for the {project_name} project. Your job is to audit
the project for orphaned, stale, and drifted artifacts and produce a structured report.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Sweep Categories

Run ALL of the following sweeps. For each finding, report:
- **Kind**: the sweep category (1-7)
- **Target**: the specific artifact (issue/PR/branch/file/label)
- **Age**: how long since last activity
- **Suggested action**: what to do about it
- **Severity**: Critical / High / Medium / Low / Info

### 1. Tracker Drift (highest priority)

- **Closed children not checked off**: tracker checklist shows `- [ ]` for an issue
  that is `state:closed`. Run `gh issue view <N> --json state` for each unchecked item.
  Suggested fix: tick the box.
- **Trackers with all children closed but tracker still open**: propose closing the tracker.
- **Tracker children that no longer exist**: an issue number in the checklist that 404s.
  Propose removing the line.
- **Orphan child references**: issue body says `Tracked by #N` where #N is closed or
  doesn't exist. Propose removing the back-reference.
- **Layer ordering inconsistencies**: child issue's `Depends On` includes a closed issue
  (no longer blocking). Propose downgrading the dependency.

### 2. Stale Issues

- Open issues with **zero activity** (no commits, comments, label changes, assignee
  changes) for **>60 days**. Use `gh issue view <N> --json updatedAt,assignees,labels`
  to check. Surface each with: last-activity timestamp, assignee, label set, linked PRs.
- Open issues whose `Blocked by #X` references a closed issue — the blocker is gone,
  they're actually unblocked. Propose removing the blocked-by line.
- Open issues with `wontfix` / `duplicate` / `invalid` labels still in `open` state —
  propose closing.

### 3. Stale Pull Requests

- PRs in `open` state with no commits, no review activity, no comments for **>14 days**.
  Use `gh pr view <N> --json updatedAt,author,headRefName,mergeable`. Surface with:
  author, branch, last activity, conflict status.
- PRs whose `Closes #N` references an already-closed issue. The PR is doing nothing.
- PRs from `agent/issue-N` branches where issue #N is closed without merge.
  Propose closing the PR + deleting the branch.

### 4. Orphaned Local Branches

- Local branches matching `agent/issue-N` for issues that are `state:closed`.
  Safe to delete after confirming no uncommitted changes.
- Local branches matching `agent/issue-N` with no remote tracking and no recent commits.
- **NEVER** propose auto-deleting unmerged branches. Surface for human approval with
  last commit metadata.

### 5. Generated / Orphaned Files

- `REPORT_SYNTHESIS.md` at the project root — if found, propose deletion.
- Files matching `.agent-tmp-*` or similar agent scratchpads.
- Other generated artifacts (e.g., `prompt.md`, `embeddings.json`).

### 6. Label Taxonomy Drift

- Labels referenced in `crates/dev/src/agent/tracker.rs` or in AGENTS.md but **not present**
  in the repo. Run `gh label list --json name --limit 200` to get current labels. Propose
  `gh label create`.
- Labels present in the repo but not referenced anywhere in the codebase. Surface for review.
- Labels with zero open issues attached and last applied >90 days ago.

### 7. ISSUES.md / STATUS.md Drift

- Entries in `ISSUES.md` Task Dependency Hierarchy tables whose status disagrees with
  the actual GitHub issue state (e.g. table says 🔴 Not Started but issue is closed).
- `STATUS.md` rows referencing capabilities whose tracking issue is closed without the
  row being updated.

---

## Output Format

Group findings by category. Within each category, sort by severity (Critical first).
Use this structure:

```
## Category N: <Name>

### [SEVERITY] <Target>
- **Kind**: <sweep category>
- **Target**: <artifact identifier>
- **Age**: <days since last activity>
- **Suggested action**: <what to do>
- **Details**: <any additional context>
```

After all categories, produce:

## Summary
- Total findings by severity
- Top 3 most impactful cleanups
- Estimated effort (low/medium/high) for each suggested action

**IMPORTANT**: Do NOT modify anything. This is a READ-ONLY audit.
Do NOT run `gh issue close`, `gh issue edit`, `git branch -d`, or any mutating commands.
Only run read commands (`gh issue view`, `gh issue list`, `gh pr list`, `git branch`, etc.)
to gather data for the report."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_housekeeping_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    let context = housekeeping_context(
        open_issues,
        open_prs,
        local_branches,
        tracker_bodies,
        status,
        issues_md,
    );
    format!(
        r#"You are a housekeeping agent for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Human Feedback

The human reviewed the housekeeping draft and provided this feedback:

{feedback}

## Instructions

Execute ONLY the cleanup actions the human approved. The feedback may say things like:
- "Skip section X entirely"
- "Fix the tracker drift but skip the stale-PR section"
- "Delete REPORT_SYNTHESIS.md but keep the agent branches for now"
- "All clear, go ahead"

Respect the feedback precisely. If the human skipped a section, do not touch it.

## Execution Order

Execute approved cleanups in this order (lowest-risk first):

1. **Tick tracker checkboxes** — Update tracker issue bodies to check off closed children.
   Use `gh issue edit <tracker> --body "<updated body>"`.
2. **Remove stale references** — Edit issue bodies to remove dead `Blocked by` or
   `Tracked by` references.
3. **Close stale issues** — Close issues with `wontfix`/`duplicate`/`invalid` labels.
   Use `gh issue close <N> --comment "Closed by housekeeping: <reason>"`.
4. **Close stale PRs** — Close PRs that reference closed issues or abandoned branches.
   Use `gh pr close <N> --comment "Closed by housekeeping: <reason>"`.
5. **Delete orphaned branches** — Delete local branches for closed issues.
   Use `git branch -d <branch>` (safe delete only).
   **NEVER delete unmerged branches** even if the human approved it — surface again
   with a warning: "Branch <name> has unmerged commits. Skipping deletion for safety.
   Use `git branch -D <name>` manually if you are sure."
6. **Delete generated files** — Remove orphaned generated files.
7. **Create missing labels** — Use `gh label create <name> --color <hex>`.
8. **Close completed trackers** — Close trackers where all children are done.

## Audit Trail

After executing all approved actions, file a `housekeeping` GitHub issue summarising
what was done:

### Step 1 — Close any prior open housekeeping issues

Run:
```
gh issue list --label housekeeping --state open --json number --jq '.[].number'
```

For each open issue number returned, close it with:
```
gh issue close <NUMBER> --comment "Superseded by the new housekeeping run."
```

### Step 2 — Create the new housekeeping issue

Run:
```
gh issue create \
  --title "Housekeeping: <YYYY-MM-DD> — <one-line summary>" \
  --body "<structured report of all actions taken, grouped by category>" \
  --label "housekeeping"
```

If tracker-related actions were taken, also add the `tracker` label:
```
--label "housekeeping,tracker"
```

### Step 3 — Echo the issue URL

Print the new issue URL so it appears in the editor panel.
Format: `Housekeeping complete: <URL>`"#
    )
}

// ── Interview prompts ──

pub fn build_interview_draft_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    format!(
        r#"You are an interview facilitator for a software project. Your job is to conduct
a structured discovery interview with the project maintainer to surface the gap
between what currently exists and what the user intends.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Instructions

Analyze the project state above — code, issues, PRs, and commit history — then ask
**directed, project-specific questions**. Do NOT use generic templates; every question
must reference concrete artifacts you observed (specific issues, code patterns, PRs,
missing tests, architectural gaps, etc.).

Organize your questions under these section headers (use exactly these headings):

### Intent vs. Current State
Ask about gaps between what exists and what the user intends. Reference specific
issues, code areas, or patterns that seem incomplete or misaligned.

### Priority and Sequencing
Ask what matters most and what should come first. Reference competing priorities
you detected (e.g. open issues that pull in different directions).

### Scope Boundaries
Ask what is in scope and what is out. Reference features or ideas that seem
ambitious or unclear in their boundaries.

### Open Questions
Surface unresolved decisions or tensions you detected in the codebase or issue
tracker. Ask the user to weigh in.

## Format

- Use the exact section headings above (### level)
- Ask 2-3 focused questions per section
- Each question should be concrete and reference specific project artifacts
- Keep questions concise — one or two sentences each
- Number your questions within each section

This is round 1 of an interactive interview. The user will answer, and you will
ask follow-up questions in round 2."#
    )
}

pub fn build_interview_followup_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    prior_answers: &[String],
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let answers_section: String = prior_answers
        .iter()
        .enumerate()
        .map(|(i, a)| format!("### Round {} response\n\n{a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        r#"You are an interview facilitator for a software project, conducting round {round}
of a structured discovery interview.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Prior Interview Responses

{answers_section}

---

## Instructions

Based on the user's prior answers, ask **targeted follow-up questions** that dig
deeper into areas where:
- Answers were ambiguous or revealed new tensions
- Important details were missing
- Priorities or scope need further clarification
- You detected contradictions between answers and the project state

Organize follow-ups under the same section headings:

### Intent vs. Current State
### Priority and Sequencing
### Scope Boundaries
### Open Questions

Only include sections where you have meaningful follow-ups. Skip sections where
the user's answers were already clear and complete.

## Format

- 1-3 follow-up questions per section (only where needed)
- Reference the user's specific answers when asking follow-ups
- Keep questions concrete and actionable

This is round {round} of the interview. The user will answer, then you will
generate a final summary."#,
        round = prior_answers.len() + 1
    )
}

pub fn build_interview_summary_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    all_answers: &[String],
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let answers_section: String = all_answers
        .iter()
        .enumerate()
        .map(|(i, a)| format!("### Round {} response\n\n{a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        r#"You are an interview facilitator for a software project. You have completed a
multi-round discovery interview. Now generate the structured summary.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Interview Responses

{answers_section}

---

## Instructions

Synthesize all interview responses into a structured summary document. This summary
will be consumed by other agent workflows for downstream planning.

## Output Format

Produce a Markdown document with these sections:

### Vision & Intent
One paragraph distilling the user's intended direction for the project.

### Priorities (ordered)
A numbered list of priorities in the order the user specified, with brief rationale
for each.

### Scope Boundaries
Two subsections:
- **In scope**: What the user confirmed is in scope
- **Out of scope**: What the user explicitly excluded or deferred

### Key Decisions
Bullet list of decisions made during the interview, referencing specific issues
or code areas where applicable.

### Open Items
Anything still unresolved — questions the user deferred, tensions that remain,
or areas needing further investigation.

### Recommended Next Actions
3-5 concrete next steps derived from the interview, each tied to a specific
issue, PR, or code area.

Do NOT create GitHub issues. Output only the summary document.
End with the line: `---interview-summary-complete---`"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #88 / #137: `find_tracker` must return one entry per row in the
    /// `gh issue list --label tracker` JSON, sorted by issue number.
    #[test]
    fn parse_tracker_list_extracts_number_and_title() {
        let json = r#"[
            {"number": 102, "title": "Agents: behavior, skills, and issue hygiene"},
            {"number": 14, "title": "Sprint 1 Tracker"}
        ]"#;
        let trackers = parse_tracker_list(json);
        assert_eq!(trackers.len(), 2);
        // Sorted ascending by number, regardless of input order.
        assert_eq!(trackers[0].number, 14);
        assert_eq!(trackers[0].title, "Sprint 1 Tracker");
        assert_eq!(trackers[1].number, 102);
        assert_eq!(
            trackers[1].title,
            "Agents: behavior, skills, and issue hygiene"
        );
    }

    /// #88 / #137: duplicate rows must be collapsed (defends against gh
    /// label paging quirks that have surfaced doubles).
    #[test]
    fn parse_tracker_list_dedupes_repeated_numbers() {
        let json = r#"[
            {"number": 5, "title": "tracker A"},
            {"number": 5, "title": "tracker A"},
            {"number": 7, "title": "tracker B"}
        ]"#;
        let trackers = parse_tracker_list(json);
        assert_eq!(trackers.len(), 2);
        assert_eq!(trackers[0].number, 5);
        assert_eq!(trackers[1].number, 7);
    }

    /// #88 / #137: an empty `gh` response must yield an empty Vec rather
    /// than panicking.
    #[test]
    fn parse_tracker_list_handles_empty_input() {
        assert!(parse_tracker_list("[]").is_empty());
        assert!(parse_tracker_list("").is_empty());
    }

    /// #88 / #137: regression guard for the title-keyword bug. Before
    /// the fix, `find_tracker` matched issues whose title contained
    /// "tracker" (e.g. the parent-tracker child issue from #84). The
    /// label-based call now filters server-side, so the parser is fed
    /// only label-tagged rows — but this guard also asserts the gh
    /// argument list still includes `--label labels::TRACKER` so a
    /// future refactor cannot silently revert to title search.
    #[test]
    fn find_tracker_uses_label_filter_not_title_search() {
        let src = include_str!("tracker.rs");
        // Locate the find_tracker function body.
        let body_start = src
            .find("pub fn find_tracker()")
            .expect("find_tracker function should exist");
        // Bound the search to the next top-level `pub fn` so we only
        // inspect this function's body.
        let body_end = src[body_start + 1..]
            .find("\npub fn ")
            .map(|i| body_start + 1 + i)
            .unwrap_or(src.len());
        let body = &src[body_start..body_end];
        assert!(
            body.contains("\"--label\""),
            "find_tracker must call gh with --label, body was: {body}"
        );
        assert!(
            body.contains("labels::TRACKER"),
            "find_tracker must filter by labels::TRACKER, body was: {body}"
        );
        // Defensive: the deprecated title-search path used `--search`
        // with quoted title keywords. Make sure it's not back.
        assert!(
            !body.contains("\"--search\""),
            "find_tracker must not use --search (title-keyword regression)"
        );
    }

    #[test]
    fn refs_basic() {
        assert_eq!(extract_issue_refs("- [ ] #42 something"), vec![42]);
    }

    #[test]
    fn refs_multiple() {
        assert_eq!(extract_issue_refs("blocked by #3, #7"), vec![3, 7]);
    }

    #[test]
    fn refs_ignores_bare_numbers() {
        assert_eq!(extract_issue_refs("keep under 10MB"), Vec::<u32>::new());
    }

    #[test]
    fn refs_ignores_hash_without_digits() {
        assert_eq!(extract_issue_refs("use # as comment"), Vec::<u32>::new());
    }

    #[test]
    fn refs_adjacent_to_punctuation() {
        assert_eq!(extract_issue_refs("(#5)"), vec![5]);
        assert_eq!(extract_issue_refs("#5."), vec![5]);
        assert_eq!(extract_issue_refs("#5,#6"), vec![5, 6]);
    }

    #[test]
    fn refs_with_spaces() {
        assert_eq!(extract_issue_refs("# 42"), vec![42]);
        assert_eq!(extract_issue_refs("#  42"), vec![42]);
    }

    #[test]
    fn bare_basic() {
        assert_eq!(extract_bare_numbers("blocked by 3, 5"), vec![3, 5]);
    }

    #[test]
    fn bare_mixed_text() {
        assert_eq!(extract_bare_numbers("issues 12 and 34"), vec![12, 34]);
    }

    #[test]
    fn blockers_prefers_hash_refs() {
        assert_eq!(extract_blockers(" #3, #7"), vec![3, 7]);
    }

    #[test]
    fn blockers_falls_back_to_bare() {
        assert_eq!(extract_blockers(" 3, 7"), vec![3, 7]);
    }

    #[test]
    fn blockers_empty() {
        assert_eq!(extract_blockers(""), Vec::<u32>::new());
    }

    #[test]
    fn completed_basic() {
        let body = "\
- [x] #1 Set up project
- [x] #2 Add CI
- [ ] #3 Implement feature";
        let done = parse_completed(body);
        assert_eq!(done, HashSet::from([1, 2]));
    }

    #[test]
    fn completed_uppercase_x() {
        let body = "- [X] #99 Done thing";
        assert_eq!(parse_completed(body), HashSet::from([99]));
    }

    #[test]
    fn completed_ignores_bare_numbers_in_text() {
        let body = "- [x] #5 keep under 10MB";
        let done = parse_completed(body);
        assert_eq!(done, HashSet::from([5]));
        assert!(!done.contains(&10));
    }

    #[test]
    fn completed_with_emoji() {
        let body = "| #5 | ✅ Done |";
        let done = parse_completed(body);
        assert_eq!(done, HashSet::from([5]));
    }

    #[test]
    fn completed_with_alternate_markers() {
        let body = r#"
| #1 | Item 1 | ✔️ Done |
| #2 | Item 2 | ☑️ Done |
| #3 | Item 3 | done |
| #4 | Item 4 | Complete |
"#;
        let set = parse_completed(body);
        assert!(set.contains(&1));
        assert!(set.contains(&2));
        assert!(set.contains(&3));
        assert!(set.contains(&4));
    }

    #[test]
    fn completed_skips_dependencies_in_tables() {
        let body = "| #5 | ✅ Done | #1, #2 |";
        let done = parse_completed(body);
        assert!(done.contains(&5));
        assert!(!done.contains(&1));
        assert!(!done.contains(&2));
    }

    #[test]
    fn completed_empty() {
        assert_eq!(parse_completed(""), HashSet::new());
    }

    #[test]
    fn execution_order_single_issue() {
        let body = "- [ ] #10 New task";
        assert_eq!(pending_issues_execution_order(body), vec![10]);
    }

    #[test]
    fn execution_order_respects_pending_blockers_before_dependents() {
        let body = "\
- [ ] #49 Child blocked by #48
- [ ] #48 Parent
";
        assert_eq!(pending_issues_execution_order(body), vec![48, 49]);
    }

    #[test]
    fn execution_order_completed_blocker_skips_reordering() {
        let body = "\
- [x] #48 Parent done
- [ ] #49 Child blocked by #48
";
        assert_eq!(pending_issues_execution_order(body), vec![49]);
    }

    #[test]
    fn execution_order_fallback_document_order_on_cycle() {
        let body = "\
- [ ] #1 Alpha blocked by #2
- [ ] #2 Beta blocked by #1
";
        assert_eq!(pending_issues_execution_order(body), vec![1, 2]);
    }

    #[test]
    fn pending_no_blockers() {
        let body = "- [ ] #10 New task";
        let pending = parse_pending(body);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].number, 10);
        assert!(pending[0].blockers.is_empty());
    }

    #[test]
    fn pending_with_hash_blockers() {
        let body = "- [ ] #11 Task blocked by #10";
        let pending = parse_pending(body);
        assert_eq!(pending[0].number, 11);
        assert_eq!(pending[0].blockers, vec![10]);
    }

    #[test]
    fn pending_with_bare_blockers() {
        let body = "- [ ] #12 Task blocked by 10, 11";
        let pending = parse_pending(body);
        assert_eq!(pending[0].blockers, vec![10, 11]);
    }

    #[test]
    fn pending_does_not_leak_issue_into_blockers() {
        let body = "- [ ] #13 task";
        let pending = parse_pending(body);
        assert!(pending[0].blockers.is_empty());
    }

    #[test]
    fn pending_with_table_status() {
        let body = "| #42 | #10 | — | 0 | 🟡 In progress |";
        let pending = parse_pending(body);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].number, 42);
        assert_eq!(pending[0].blockers, vec![10]);
    }

    #[test]
    fn pending_skips_completed_lines() {
        let body = "- [x] #1 done";
        assert!(parse_pending(body).is_empty());
    }

    #[test]
    fn pending_deduplicates_repeated_issues() {
        let body = "\
- [ ] #34 Focused Delivery: Gateway WebSocket upgrade handling MVP
| #34 Focused Delivery: Gateway WebSocket upgrade handling MVP | — | #35, #36, #32 | 0 | 🔴 Not Started |
";
        let pending = parse_pending(body);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].number, 34);
        // The table row heuristic should not pick up dependents as blockers
        assert!(pending[0].blockers.is_empty());
    }

    #[test]
    fn pending_extracts_blockers_from_table() {
        let body = "| #36 | #34, #35 | #32 | 2 | 🔴 Not Started |";
        let pending = parse_pending(body);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].number, 36);
        assert_eq!(pending[0].blockers, vec![34, 35]);
    }

    #[test]
    fn ready_no_blockers() {
        let issue = PendingIssue {
            number: 1,
            title: String::new(),
            blockers: vec![],
            pr_number: None,
        };
        let completed = HashSet::new();
        assert!(is_ready(&issue, &completed));
    }

    #[test]
    fn ready_all_done() {
        let issue = PendingIssue {
            number: 3,
            title: String::new(),
            blockers: vec![1, 2],
            pr_number: None,
        };
        let completed = HashSet::from([1, 2]);
        assert!(is_ready(&issue, &completed));
    }

    #[test]
    fn blocked_missing_dep() {
        let issue = PendingIssue {
            number: 3,
            title: String::new(),
            blockers: vec![1, 2],
            pr_number: None,
        };
        let completed = HashSet::from([1]);
        assert!(!is_ready(&issue, &completed));
    }

    #[test]
    fn mark_replaces_checkbox() {
        let body = "- [ ] #123 task";
        assert_eq!(mark_completed(body, 123), "- [x] #123 task");
    }

    #[test]
    fn mark_bold_issue_ref() {
        let body = "- [ ] **#19** — Persist controller state `[M]`";
        assert_eq!(
            mark_completed(body, 19),
            "- [x] **#19** — Persist controller state `[M]`"
        );
    }

    #[test]
    fn mark_no_match_is_noop() {
        let body = "- [ ] #456 task";
        assert_eq!(mark_completed(body, 123), body);
    }

    // ── PrSummary deserialization ──

    #[test]
    fn pr_summary_deserialize_full() {
        let json = r#"[
            {"number":42,"title":"Add caching","headRefName":"feat/cache","author":{"login":"alice"}}
        ]"#;
        let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].title, "Add caching");
        assert_eq!(prs[0].head_ref_name, "feat/cache");
        assert_eq!(prs[0].author.as_ref().unwrap().login, "alice");
    }

    #[test]
    fn pr_summary_deserialize_no_author() {
        let json = r#"[{"number":1,"title":"Fix","headRefName":"fix/bug"}]"#;
        let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
        assert_eq!(prs.len(), 1);
        assert!(prs[0].author.is_none());
    }

    #[test]
    fn pr_summary_deserialize_empty_array() {
        let prs: Vec<PrSummary> = serde_json::from_str("[]").unwrap();
        assert!(prs.is_empty());
    }

    /// Phase 4 (#146): the new `unresolved_thread_count` field must default
    /// to 0 when missing from the `gh pr list` JSON, so the existing CLI
    /// payload (which has no thread-count column) deserializes unchanged.
    #[test]
    fn pr_summary_unresolved_thread_count_defaults_to_zero() {
        let json = r#"[
            {"number":42,"title":"Add caching","headRefName":"feat/cache","author":{"login":"alice"}}
        ]"#;
        let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
        assert_eq!(prs[0].unresolved_thread_count, 0);
    }

    // ── Phase 4: batched PR thread-count parser (#146) ──

    /// Acceptance criterion from #146: parses a batched
    /// `repository.pullRequests.reviewThreads` GraphQL response into a
    /// `{pr_number: count}` map. Resolved threads and human-authored
    /// threads are excluded so the badge count matches what the Phase 2
    /// Fix Comments dispatch would actually act on.
    #[test]
    fn parse_pr_thread_counts_filters_resolved_and_human_authors() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 143,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": true,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "geoffsee"}}]}
                                        }
                                    ]
                                }
                            },
                            {
                                "number": 144,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": true,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        }
                                    ]
                                }
                            },
                            {
                                "number": 145,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "dependabot[bot]"}}]}
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }"#;
        let counts = parse_pr_thread_counts(json, "llm-overlord");

        // PR #143: 4 threads total, but only 2 are unresolved AND bot-authored.
        assert_eq!(counts.get(&143), Some(&2));
        // PR #144: only 1 thread, and it's resolved => not in the map at all.
        assert!(!counts.contains_key(&144));
        // PR #145: dependabot[bot] qualifies via the bracket-bot suffix rule.
        assert_eq!(counts.get(&145), Some(&1));
    }

    /// PRs with no review threads at all (the common case for fresh PRs)
    /// must NOT appear in the map — callers treat absence as zero.
    #[test]
    fn parse_pr_thread_counts_omits_zero_count_prs() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 200,
                                "reviewThreads": {"nodes": []}
                            }
                        ]
                    }
                }
            }
        }"#;
        let counts = parse_pr_thread_counts(json, "llm-overlord");
        assert!(counts.is_empty());
    }

    /// Empty `pullRequests.nodes` (no open PRs) must yield an empty map,
    /// not a panic.
    #[test]
    fn parse_pr_thread_counts_handles_empty_response() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {"nodes": []}
                }
            }
        }"#;
        assert!(parse_pr_thread_counts(json, "llm-overlord").is_empty());
    }

    /// Malformed JSON / unrelated payloads return an empty map without
    /// panicking — Phase 4 must NOT crash refresh on a parse error.
    #[test]
    fn parse_pr_thread_counts_survives_garbage() {
        assert!(parse_pr_thread_counts("not json", "llm-overlord").is_empty());
        assert!(parse_pr_thread_counts("", "llm-overlord").is_empty());
        assert!(parse_pr_thread_counts("{}", "llm-overlord").is_empty());
    }

    // ── Prompt builder: issue implementation ──

    #[test]
    fn build_prompt_contains_issue_number_and_body() {
        let p = build_prompt(
            "test-project",
            7,
            "Add caching",
            "Implement LRU cache",
            "fn main() {}",
            0,
            "",
        );
        assert!(p.contains("test-project"));
        assert!(p.contains("Issue #7"));
        assert!(p.contains("Add caching"));
        assert!(p.contains("Implement LRU cache"));
        assert!(p.contains("fn main() {}"));
        assert!(p.contains("Codebase Snapshot"));
        assert!(p.contains("ISSUES.md"));
        assert!(p.contains("STATUS.md"));
        assert!(p.contains("Do NOT commit"));
        // No tracker section when tracker body is empty
        assert!(!p.contains("Parent Tracker"));
    }

    #[test]
    fn build_prompt_includes_parent_tracker_when_present() {
        let tracker_body =
            "## Sprint Goal\nShip caching layer.\n- [ ] #7 Add caching\n- [ ] #8 Add eviction";
        let p = build_prompt(
            "test-project",
            7,
            "Add caching",
            "Implement LRU cache",
            "fn main() {}",
            42,
            tracker_body,
        );
        assert!(p.contains("## Parent Tracker #42"));
        assert!(p.contains("Ship caching layer."));
        assert!(p.contains("Treat the tracker as authoritative for scope"));
        assert!(p.contains("surface the conflict as a comment on the tracker"));
        // Still contains the issue content
        assert!(p.contains("Issue #7"));
        assert!(p.contains("Implement LRU cache"));
    }

    #[test]
    fn build_prompt_no_tracker_section_when_body_empty() {
        let p = build_prompt(
            "test-project",
            7,
            "Add caching",
            "Implement LRU cache",
            "",
            99,
            "",
        );
        assert!(!p.contains("Parent Tracker"));
        assert!(!p.contains("surface the conflict"));
        assert!(p.contains("Issue #7"));
    }

    // ── Prompt builder: sprint planning draft vs finalize ──

    #[test]
    fn sprint_draft_does_not_create_issues() {
        let p = build_sprint_planning_draft_prompt(
            "test-project",
            "[issues]",
            "[prs]",
            "[status]",
            "[issues_md]",
        );
        assert!(p.contains("[issues]"));
        assert!(p.contains("[prs]"));
        assert!(p.contains("[status]"));
        assert!(p.contains("[issues_md]"));
        assert!(p.contains("Dependency Hierarchy"));
        assert!(p.contains("DRAFT"));
        assert!(!p.contains("gh issue create"));
    }

    #[test]
    fn sprint_finalize_includes_feedback_and_creates_issues() {
        let p = build_sprint_planning_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[s]",
            "[m]",
            "focus on DX",
        );
        assert!(p.contains("focus on DX"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("ISSUES.md"));
        assert!(!p.contains("DRAFT"));
    }

    #[test]
    fn sprint_finalize_creates_tracker_with_labels() {
        let p =
            build_sprint_planning_finalize_prompt("test-project", "[i]", "[p]", "[s]", "[m]", "fb");
        assert!(p.contains("--label \"sprint,tracker\""));
        assert!(p.contains("Tracked by #<tracker>"));
    }

    // ── Prompt builder: strategic review draft vs finalize ──

    #[test]
    fn strategic_draft_contains_all_perspectives() {
        let p = build_strategic_review_draft_prompt(
            "test-project",
            "[issues]",
            "[prs]",
            "[commits]",
            "[status]",
            "[issues_md]",
            "[crates]",
            "",
        );
        assert!(p.contains("Product Stakeholder"));
        assert!(p.contains("Business Analyst"));
        assert!(p.contains("Lead Engineer"));
        assert!(p.contains("UX / DX Researcher"));
        assert!(p.contains("DRAFT"));
        assert!(!p.contains("gh issue create"));
    }

    #[test]
    fn strategic_draft_includes_all_context() {
        let p = build_strategic_review_draft_prompt(
            "test-project",
            "ISSUES_JSON",
            "PRS_JSON",
            "abc123 commit",
            "STATUS_CONTENT",
            "ISSUES_MD",
            "CRATE_LIST",
            "",
        );
        assert!(p.contains("ISSUES_JSON"));
        assert!(p.contains("PRS_JSON"));
        assert!(p.contains("abc123 commit"));
        assert!(p.contains("STATUS_CONTENT"));
        assert!(p.contains("ISSUES_MD"));
        assert!(p.contains("CRATE_LIST"));
    }

    #[test]
    fn strategic_draft_includes_report_synthesis_when_present() {
        let p = build_strategic_review_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "Top priority: fix auth. Velocity: steady.",
        );
        assert!(p.contains("Prior Report Synthesis"));
        assert!(p.contains("Top priority: fix auth. Velocity: steady."));
    }

    #[test]
    fn strategic_draft_omits_synthesis_when_empty() {
        let p = build_strategic_review_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
        );
        assert!(!p.contains("Prior Report Synthesis"));
    }

    #[test]
    fn strategic_finalize_includes_feedback_and_creates_single_issue() {
        let p = build_strategic_review_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            "skip OIDC, focus on CLI",
        );
        assert!(p.contains("skip OIDC, focus on CLI"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("gh issue edit"));
        assert!(!p.contains("DRAFT"));
        // Single-issue contract: exactly one strategic-review issue, edited in place on
        // subsequent runs. No per-recommendation children, no parent tracker.
        assert!(p.contains("**exactly one** GitHub issue"));
        assert!(p.contains("--label \"strategic-review\""));
        assert!(p.contains("Do not file recommendation issues"));
    }

    #[test]
    fn strategic_finalize_does_not_emit_tracker_layout() {
        let p = build_strategic_review_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            "fb",
        );
        // The old strategic-review,tracker layout is gone — strategic review is a single
        // living artifact, not a parent + child issue tree. Sprint Planning is the only
        // workflow that still files trackers.
        assert!(!p.contains("\"strategic-review,tracker\""));
        assert!(!p.contains("Tracked by #<tracker>"));
    }

    #[test]
    fn strategic_draft_sets_single_issue_expectation() {
        let p = build_strategic_review_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
        );
        // The draft must tell the agent up front that finalize publishes one issue, not
        // many — otherwise it shapes the recommended path forward around per-item issues.
        assert!(p.contains("**exactly one** GitHub issue"));
        assert!(p.contains("`strategic-review` label"));
    }

    #[test]
    fn strategic_finalize_includes_report_synthesis() {
        let p = build_strategic_review_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "synthesis content here",
            "my feedback",
        );
        assert!(p.contains("Prior Report Synthesis"));
        assert!(p.contains("synthesis content here"));
        assert!(p.contains("my feedback"));
    }

    // ── Prompt builder: ideation draft vs finalize ──

    #[test]
    fn ideation_draft_is_divergent_draft() {
        let p =
            build_ideation_draft_prompt("test-project", "[i]", "[p]", "[c]", "[s]", "[m]", "[t]");
        assert!(p.contains("DRAFT"));
        assert!(p.contains("Capability ideas"));
        assert!(p.contains("Foundational ideas"));
        assert!(p.contains("Provocations"));
        assert!(p.contains("Wildcards"));
        assert!(p.contains("at least 15"));
        assert!(!p.contains("gh issue create"));
    }

    #[test]
    fn ideation_draft_includes_all_context() {
        let p = build_ideation_draft_prompt(
            "test-project",
            "ISSUES_JSON",
            "PRS_JSON",
            "abc123 commit",
            "STATUS_CONTENT",
            "ISSUES_MD",
            "CRATE_LIST",
        );
        assert!(p.contains("ISSUES_JSON"));
        assert!(p.contains("PRS_JSON"));
        assert!(p.contains("abc123 commit"));
        assert!(p.contains("STATUS_CONTENT"));
        assert!(p.contains("ISSUES_MD"));
        assert!(p.contains("CRATE_LIST"));
    }

    #[test]
    fn ideation_finalize_includes_feedback_and_creates_issue() {
        let p = build_ideation_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "keep ideas 1-5, drop the rest",
            false,
        );
        assert!(p.contains("keep ideas 1-5, drop the rest"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("ideation"));
        assert!(!p.contains("DRAFT"));
        assert!(!p.contains("DRY RUN"));
    }

    #[test]
    fn ideation_finalize_dry_run_includes_dry_run_note() {
        let p = build_ideation_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "feedback",
            true,
        );
        assert!(p.contains("DRY RUN"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("ideation"));
    }

    // ── Prompt builder: report draft vs finalize ──

    #[test]
    fn report_draft_is_draft_not_final() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            &sp,
        );
        assert!(p.contains("DRAFT"));
        assert!(p.contains("Executive Summary"));
        assert!(p.contains("Risk Assessment"));
        assert!(!p.contains("gh issue create"));
    }

    #[test]
    fn report_draft_includes_ideation_when_present() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "Add WebSocket support idea",
            &sp,
        );
        assert!(p.contains("Prior Ideation"));
        assert!(p.contains("Add WebSocket support idea"));
    }

    #[test]
    fn report_draft_includes_persona_lens() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            &sp,
        );
        assert!(p.contains(&sp.user_personas));
        assert!(p.contains("Synthesis Lens"));
        assert!(p.contains("Do NOT conflate it with other skills"));
        assert!(p.contains("`recognition_cues:`"));
        assert!(p.contains("`jobs_to_be_done:`"));
        assert!(p.contains("`pains:`"));
        assert!(p.contains("`anti_goals:`"));
        assert!(p.contains("possible persona blind"));
    }

    #[test]
    fn report_draft_includes_persona_lens_with_custom_skill_path() {
        // Verifies that library consumers (e.g. crates/dev in the freq workspace)
        // can override the user-personas skill path and have it propagate into
        // the prompt verbatim — drop-in support for prefixed skill layouts.
        let sp = crate::agent::types::SkillPaths {
            user_personas: "/custom/skills/freq-cloud-user-personas/SKILL.md".into(),
            issue_tracking: "/custom/skills/freq-cloud-issue-tracking/SKILL.md".into(),
        };
        let p = build_report_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            &sp,
        );
        assert!(p.contains("/custom/skills/freq-cloud-user-personas/SKILL.md"));
        assert!(!p.contains("user-personas/SKILL.md\n"));
    }

    #[test]
    fn report_draft_omits_ideation_when_empty() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_draft_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            &sp,
        );
        assert!(!p.contains("Prior Ideation"));
    }

    #[test]
    fn report_finalize_includes_feedback_and_synthesis() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            "add more detail on blockers",
            false,
            &sp,
        );
        assert!(p.contains("add more detail on blockers"));
        assert!(p.contains("Synthesis"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("uxr-synthesis"));
        assert!(!p.contains("REPORT_SYNTHESIS.md"));
        assert!(!p.contains("DRY RUN"));
        assert!(!p.contains("DRAFT"));
    }

    #[test]
    fn report_finalize_includes_persona_lens_and_synthesis_attribution() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            "feedback",
            false,
            &sp,
        );
        assert!(p.contains(&sp.user_personas));
        assert!(p.contains("Synthesis Lens"));
        assert!(p.contains("Do NOT conflate it with other skills"));
        assert!(p.contains("`recognition_cues:`"));
        assert!(p.contains("`jobs_to_be_done:`"));
        assert!(p.contains("`pains:`"));
        assert!(p.contains("`anti_goals:`"));
        assert!(p.contains("possible persona blind"));
        assert!(p.contains("dominant persona signal"));
        assert!(p.contains("appeared in zero evidence"));
    }

    #[test]
    fn report_finalize_dry_run_includes_dry_run_note() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "",
            "feedback",
            true,
            &sp,
        );
        assert!(p.contains("DRY RUN"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("uxr-synthesis"));
    }

    #[test]
    fn report_finalize_includes_ideation_when_present() {
        let sp = crate::agent::types::SkillPaths::default();
        let p = build_report_finalize_prompt(
            "test-project",
            "[i]",
            "[p]",
            "[c]",
            "[s]",
            "[m]",
            "[t]",
            "ideation content here",
            "my feedback",
            false,
            &sp,
        );
        assert!(p.contains("Prior Ideation"));
        assert!(p.contains("ideation content here"));
        assert!(p.contains("my feedback"));
    }

    // ── Prompt builder: retrospective draft vs finalize ──

    #[test]
    fn retro_draft_contains_all_sections() {
        let p = build_retrospective_draft_prompt(
            "test-project",
            "[commits]",
            "[closed]",
            "[merged]",
            "[open_i]",
            "[open_p]",
            "[status]",
            "[issues_md]",
        );
        assert!(p.contains("What shipped"));
        assert!(p.contains("What went well"));
        assert!(p.contains("What was painful"));
        assert!(p.contains("What to change"));
        assert!(p.contains("Velocity"));
        assert!(p.contains("DRAFT"));
        assert!(!p.contains("gh issue create"));
    }

    #[test]
    fn retro_draft_includes_all_context() {
        let p = build_retrospective_draft_prompt(
            "test-project",
            "COMMITS",
            "CLOSED",
            "MERGED",
            "OPEN_I",
            "OPEN_P",
            "STATUS",
            "ISSUES_MD",
        );
        assert!(p.contains("COMMITS"));
        assert!(p.contains("CLOSED"));
        assert!(p.contains("MERGED"));
        assert!(p.contains("OPEN_I"));
        assert!(p.contains("OPEN_P"));
        assert!(p.contains("STATUS"));
        assert!(p.contains("ISSUES_MD"));
    }

    #[test]
    fn retro_finalize_includes_feedback_and_creates_single_issue() {
        let p = build_retrospective_finalize_prompt(
            "test-project",
            "[c]",
            "[cl]",
            "[m]",
            "[oi]",
            "[op]",
            "[s]",
            "[im]",
            "error messages need work",
        );
        assert!(p.contains("error messages need work"));
        assert!(p.contains("gh issue create"));
        assert!(p.contains("gh issue edit"));
        assert!(p.contains("ISSUES.md"));
        assert!(!p.contains("DRAFT"));
        // Single-issue contract: exactly one retrospective issue, edited in place on
        // subsequent runs. No per-action-item children, no parent tracker.
        assert!(p.contains("**exactly one** GitHub issue"));
        assert!(p.contains("--label \"retrospective\""));
        assert!(p.contains("Do not file per-action-item issues"));
    }

    #[test]
    fn retro_draft_sets_single_issue_expectation() {
        let p = build_retrospective_draft_prompt(
            "test-project",
            "[c]",
            "[cl]",
            "[m]",
            "[oi]",
            "[op]",
            "[s]",
            "[im]",
        );
        // The draft must tell the agent up front that finalize publishes one issue,
        // not many — otherwise it shapes the draft around per-item issues.
        assert!(p.contains("**exactly one** GitHub issue"));
        assert!(p.contains("`retrospective` label"));
    }

    // ── Prompt builder: code review ──

    #[test]
    fn code_review_prompt_includes_pr_context() {
        let p = build_code_review_prompt(
            "test-project",
            42,
            "Add caching",
            "Implements LRU",
            "+fn cache()",
        );
        assert!(p.contains("test-project"));
        assert!(p.contains("Pull Request #42"));
        assert!(p.contains("Add caching"));
        assert!(p.contains("Implements LRU"));
        assert!(p.contains("+fn cache()"));
        assert!(p.contains("APPROVE"));
        assert!(p.contains("REQUEST_CHANGES"));
        assert!(p.contains("gh api"));
        assert!(p.contains("/pulls/42/reviews"));
    }

    #[test]
    fn code_review_prompt_uses_inline_comment_schema() {
        let p = build_code_review_prompt(
            "test-project",
            42,
            "Add caching",
            "Implements LRU",
            "+fn cache()",
        );
        // Inline-comment payload schema must be present so future edits
        // can't accidentally drop the line-anchored review path.
        assert!(p.contains("\"path\""));
        assert!(p.contains("\"line\""));
        assert!(p.contains("\"side\": \"RIGHT\""));
        assert!(p.contains("\"comments\""));
        assert!(p.contains("commit_id"));
        // Must explicitly forbid the gh pr review fallback.
        assert!(p.contains("Do NOT use `gh pr review`"));
    }

    #[test]
    fn code_review_prompt_checks_security() {
        let p = build_code_review_prompt("test-project", 1, "t", "b", "d");
        assert!(p.contains("Security"));
        assert!(p.contains("OWASP"));
    }

    // ── Phase 2: Fix Comments prompt + thread parser (#144) ──

    fn sample_thread(id: &str, path: &str, line: u32, body: &str) -> ReviewThread {
        ReviewThread {
            id: id.to_string(),
            path: path.to_string(),
            line,
            body: body.to_string(),
            author: DEFAULT_REVIEW_BOT_LOGIN.to_string(),
        }
    }

    /// Acceptance criterion from #144: "New unit tests on the prompt builder
    /// asserting it includes the diff, thread bodies, and per-thread line
    /// anchors."
    #[test]
    fn pr_review_fix_prompt_includes_diff_branch_and_thread_anchors() {
        let threads = vec![
            sample_thread(
                "PRT_kw1",
                "test-review-fixture.md",
                14,
                "Item 5 is incorrect — JWTs are signed by default, not encrypted.",
            ),
            sample_thread(
                "PRT_kw2",
                "test-review-fixture.md",
                16,
                "Item 7 is incorrect — fast-forward merges do not create a merge commit.",
            ),
        ];
        let p = build_pr_review_fix_prompt(
            "freq-cloud",
            143,
            "test: PR review comment fixture",
            "test-pr-review-comments",
            "@@ -10,5 +10,5 @@\n-old\n+new\n",
            &threads,
        );

        // Project + PR identification.
        assert!(p.contains("freq-cloud"));
        assert!(p.contains("Pull Request #143"));
        assert!(p.contains("test: PR review comment fixture"));

        // Branch must be embedded so the agent knows which worktree it's in.
        assert!(p.contains("test-pr-review-comments"));

        // Diff must be included verbatim inside the diff fence.
        assert!(p.contains("```diff"));
        assert!(p.contains("@@ -10,5 +10,5 @@"));
        assert!(p.contains("-old"));
        assert!(p.contains("+new"));

        // Each thread must surface its anchor (path:line), bot author, and body.
        assert!(p.contains("test-review-fixture.md:14"));
        assert!(p.contains("test-review-fixture.md:16"));
        assert!(p.contains(&format!("@{DEFAULT_REVIEW_BOT_LOGIN}")));
        assert!(p.contains("JWTs are signed by default"));
        assert!(p.contains("fast-forward merges do not create a merge commit"));

        // Thread count is reported so the agent can sanity-check coverage.
        assert!(p.contains("Unresolved Review Threads (2)"));

        // Worktree contract: do NOT commit, do NOT push, do NOT cd elsewhere.
        assert!(p.contains("Do NOT commit"));
        assert!(p.contains("Do NOT `cd`"));
    }

    /// A Fix Comments run with zero threads is supposed to bail out before
    /// reaching the prompt builder, but if it ever does the prompt must
    /// still be coherent (no panic, no missing sections).
    #[test]
    fn pr_review_fix_prompt_handles_empty_threads() {
        let p = build_pr_review_fix_prompt(
            "freq-cloud",
            143,
            "fixture",
            "test-pr-review-comments",
            "diff content",
            &[],
        );
        assert!(p.contains("Unresolved Review Threads (0)"));
        assert!(p.contains("diff content"));
    }

    /// Fixture mirrors the shape of `gh api graphql` output for the
    /// `reviewThreads` query in `scripts/resolve-pr-threads.sh`. Resolved
    /// threads and human-authored threads must be filtered out so a Fix
    /// Comments run only acts on findings the project's review bot raised.
    #[test]
    fn parse_review_threads_filters_resolved_and_human_authors() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_kw1",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 14,
                                                "originalLine": 14,
                                                "body": "Item 5 is incorrect."
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw2",
                                    "isResolved": true,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 16,
                                                "body": "already resolved"
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw3",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee"},
                                                "path": "src/foo.rs",
                                                "line": 42,
                                                "body": "human comment"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
        let threads = parse_review_threads(json, "llm-overlord");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "PRT_kw1");
        assert_eq!(threads[0].path, "test-review-fixture.md");
        assert_eq!(threads[0].line, 14);
        assert_eq!(threads[0].author, "llm-overlord");
        assert_eq!(threads[0].body, "Item 5 is incorrect.");
    }

    /// GitHub apps surface as `<name>[bot]` in the GraphQL response (e.g.
    /// `dependabot[bot]`). The parser must accept any author whose login
    /// ends with `[bot]` so the bot-only filter doesn't depend on the
    /// configured `bot_login` matching exactly.
    #[test]
    fn parse_review_threads_accepts_bracket_bot_suffix() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_x",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "dependabot[bot]"},
                                                "path": "Cargo.toml",
                                                "line": 5,
                                                "body": "bump"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
        let threads = parse_review_threads(json, "llm-overlord");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].author, "dependabot[bot]");
    }

    /// Outdated threads can have `line: null`. The parser must fall back to
    /// `originalLine` so the prompt still has a meaningful anchor instead of
    /// dropping the thread or printing `:0`.
    #[test]
    fn parse_review_threads_falls_back_to_original_line() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_outdated",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "src/foo.rs",
                                                "line": null,
                                                "originalLine": 42,
                                                "body": "outdated finding"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
        let threads = parse_review_threads(json, "llm-overlord");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].line, 42);
    }

    /// An empty `reviewThreads.nodes` array (PR with no review activity) must
    /// yield an empty Vec, not a panic.
    #[test]
    fn parse_review_threads_handles_empty_response() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": []
                        }
                    }
                }
            }
        }"#;
        assert!(parse_review_threads(json, "llm-overlord").is_empty());
    }

    /// Malformed JSON must not panic — the function logs a warning and
    /// returns an empty Vec so the calling Fix run can bail cleanly.
    #[test]
    fn parse_review_threads_survives_malformed_json() {
        assert!(parse_review_threads("not json at all", "llm-overlord").is_empty());
        assert!(parse_review_threads("", "llm-overlord").is_empty());
    }

    // ── Phase 3: resolveReviewThread mutation parser (#145) ──

    /// The mutation query string itself must contain the resolveReviewThread
    /// operation name and the threadId variable so a future refactor can't
    /// silently degrade it into a no-op.
    #[test]
    fn resolve_review_thread_mutation_targets_correct_operation() {
        assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("resolveReviewThread"));
        assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("$threadId: ID!"));
        assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("isResolved"));
    }

    /// The acceptance criterion in #145: a successful mutation response with
    /// `isResolved: true` returns true.
    #[test]
    fn parse_resolve_review_thread_response_accepts_success() {
        let json = r#"{
            "data": {
                "resolveReviewThread": {
                    "thread": { "id": "PRT_kw1", "isResolved": true }
                }
            }
        }"#;
        assert!(parse_resolve_review_thread_response(json));
    }

    /// `isResolved: false` in the response means the mutation succeeded at
    /// the API level but the thread did not flip to resolved (e.g. already
    /// merged, permission edge case). Treat as failure so the caller logs
    /// it and the user can investigate.
    #[test]
    fn parse_resolve_review_thread_response_rejects_unresolved() {
        let json = r#"{
            "data": {
                "resolveReviewThread": {
                    "thread": { "id": "PRT_kw1", "isResolved": false }
                }
            }
        }"#;
        assert!(!parse_resolve_review_thread_response(json));
    }

    /// A GraphQL `errors` response with no `data` payload must surface as
    /// failure, not panic.
    #[test]
    fn parse_resolve_review_thread_response_rejects_graphql_error() {
        let json = r#"{
            "errors": [
                { "message": "Resource not accessible by integration", "type": "FORBIDDEN" }
            ]
        }"#;
        assert!(!parse_resolve_review_thread_response(json));
    }

    /// Malformed JSON / empty bodies / unrelated payloads return false
    /// without panicking — Phase 3 must NOT abort the Fix run on a parse
    /// error since the fix is already pushed.
    #[test]
    fn parse_resolve_review_thread_response_survives_garbage() {
        assert!(!parse_resolve_review_thread_response("not json"));
        assert!(!parse_resolve_review_thread_response(""));
        assert!(!parse_resolve_review_thread_response("{}"));
        assert!(!parse_resolve_review_thread_response(
            r#"{"data": {"unrelated": true}}"#
        ));
    }

    // ── Prompt builder: fix prompt ──

    #[test]
    fn fix_prompt_includes_output() {
        let p = build_fix_prompt(5, "error: cannot find type");
        assert!(p.contains("issue #5"));
        assert!(p.contains("error: cannot find type"));
        assert!(p.contains("Do NOT commit"));
    }

    // ── build_security_review_prompt ──

    #[test]
    fn security_review_prompt_with_snapshot() {
        let prompt = build_security_review_prompt(
            "test-project",
            "compute-node\nedge-node",
            "fn main() {}",
            false,
        );
        assert!(prompt.contains("test-project"));
        assert!(prompt.contains("compute-node"));
        assert!(prompt.contains("edge-node"));
        assert!(prompt.contains("Codebase Snapshot"));
        assert!(prompt.contains("fn main() {}"));
        assert!(!prompt.contains("Read the codebase directly"));
    }

    #[test]
    fn security_review_prompt_without_snapshot() {
        let prompt = build_security_review_prompt("test-project", "compute-node", "", false);
        assert!(prompt.contains("compute-node"));
        assert!(prompt.contains("Read the codebase directly"));
        assert!(!prompt.contains("Codebase Snapshot"));
    }

    #[test]
    fn security_review_prompt_creates_tracker_with_labels() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(prompt.contains("--label \"security,tracker\""));
        assert!(prompt.contains("Tracked by #<tracker>"));
        assert!(prompt.contains("Tracker Issue"));
        assert!(prompt.contains("Actionable Findings"));
    }

    #[test]
    fn security_review_prompt_creates_per_finding_issues() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(prompt.contains("gh issue create"));
        assert!(prompt.contains("gh issue edit"));
        assert!(prompt.contains("security:"));
        assert!(prompt.contains("severity:critical"));
        assert!(prompt.contains("severity:high"));
        assert!(prompt.contains("severity:medium"));
    }

    #[test]
    fn security_review_prompt_duplicate_detection() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(prompt.contains("Duplicate Detection"));
        assert!(prompt.contains("gh issue list --label security --search"));
        assert!(prompt.contains("Already tracked"));
    }

    #[test]
    fn security_review_prompt_low_info_rollup() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(prompt.contains("Low / Informational Findings"));
        assert!(prompt.contains("rollup"));
        assert!(prompt.contains("severity:low"));
    }

    #[test]
    fn security_review_prompt_cross_reference_summary() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(prompt.contains("Cross-Reference Summary"));
        assert!(prompt.contains("Filed:"));
    }

    #[test]
    fn security_review_prompt_dry_run() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", true);
        assert!(prompt.contains("DRY RUN MODE"));
        assert!(prompt.contains("[dry-run]"));
        assert!(prompt.contains("Do NOT execute any `gh issue create`"));
    }

    #[test]
    fn security_review_prompt_no_dry_run_section_when_false() {
        let prompt =
            build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
        assert!(!prompt.contains("DRY RUN MODE"));
    }

    #[test]
    fn refresh_agents_prompt_limits_scope_and_requires_summary_block() {
        let prompt = build_refresh_agents_prompt(
            "test-project",
            &[
                "AGENTS.md".to_string(),
                "skills/testing/SKILL.md".to_string(),
            ],
        );
        assert!(prompt.contains("test-project"));
        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("skills/testing/SKILL.md"));
        assert!(prompt.contains("Do NOT edit source code"));
        assert!(prompt.contains("REFRESH_AGENTS_SUMMARY_BEGIN"));
        assert!(prompt.contains("REFRESH_AGENTS_NO_CHANGES"));
        assert!(prompt.contains("Do NOT commit, push, or open a pull request"));
    }

    #[test]
    fn refresh_docs_prompt_limits_scope_and_requires_summary_block() {
        let prompt = build_refresh_docs_prompt(
            "test-project",
            &[
                "README.md".to_string(),
                "STATUS.md".to_string(),
                "docs/ARCHITECTURE.md".to_string(),
            ],
        );
        assert!(prompt.contains("test-project"));
        assert!(prompt.contains("README.md"));
        assert!(prompt.contains("STATUS.md"));
        assert!(prompt.contains("docs/ARCHITECTURE.md"));
        assert!(prompt.contains("Do NOT edit source code"));
        assert!(prompt.contains("REFRESH_DOCS_SUMMARY_BEGIN"));
        assert!(prompt.contains("REFRESH_DOCS_NO_CHANGES"));
        assert!(prompt.contains("Do NOT commit, push, or open a pull request"));
    }

    // ── parse_auto_merge_response ──

    #[test]
    fn auto_merge_null_is_disabled() {
        assert!(!parse_auto_merge_response(Some("null".into())));
    }

    #[test]
    fn auto_merge_empty_is_disabled() {
        assert!(!parse_auto_merge_response(Some(String::new())));
    }

    #[test]
    fn auto_merge_none_is_disabled() {
        assert!(!parse_auto_merge_response(None));
    }

    #[test]
    fn auto_merge_json_object_is_enabled() {
        assert!(parse_auto_merge_response(Some(
            r#"{"mergeMethod":"SQUASH"}"#.into()
        )));
    }

    // ── find_upstream_branch ──

    #[test]
    fn upstream_branch_no_blockers() {
        assert_eq!(
            find_upstream_branch(&[]),
            crate::agent::cmd::origin_default_branch()
        );
    }

    // ── Housekeeping prompt builders ──

    #[test]
    fn housekeeping_draft_prompt_contains_all_sweep_categories() {
        let prompt = build_housekeeping_draft_prompt(
            "test-project",
            "[]",
            "[]",
            "master\nagent/issue-1",
            "- [ ] #1 task",
            "| Feature | ✅ |",
            "# ISSUES",
        );
        assert!(prompt.contains("Tracker Drift"));
        assert!(prompt.contains("Stale Issues"));
        assert!(prompt.contains("Stale Pull Requests"));
        assert!(prompt.contains("Orphaned Local Branches"));
        assert!(prompt.contains("Generated / Orphaned Files"));
        assert!(prompt.contains("Label Taxonomy Drift"));
        assert!(prompt.contains("ISSUES.md / STATUS.md Drift"));
        assert!(prompt.contains("READ-ONLY audit"));
        assert!(prompt.contains("Do NOT modify anything"));
    }

    #[test]
    fn housekeeping_draft_prompt_includes_context() {
        let prompt = build_housekeeping_draft_prompt(
            "test-project",
            "[{\"number\":42}]",
            "[{\"number\":10}]",
            "master\nagent/issue-42",
            "- [ ] #42 task",
            "status content",
            "issues content",
        );
        assert!(prompt.contains("[{\"number\":42}]"));
        assert!(prompt.contains("agent/issue-42"));
        assert!(prompt.contains("- [ ] #42 task"));
    }

    #[test]
    fn housekeeping_finalize_prompt_contains_feedback() {
        let prompt = build_housekeeping_finalize_prompt(
            "test-project",
            "[]",
            "[]",
            "master",
            "",
            "",
            "",
            "Fix tracker drift only, skip everything else",
        );
        assert!(prompt.contains("Fix tracker drift only, skip everything else"));
        assert!(prompt.contains("Execution Order"));
        assert!(prompt.contains("NEVER delete unmerged branches"));
        assert!(prompt.contains("housekeeping"));
    }

    // ── Tracker drift detection (unit test for sweep category 1) ──

    #[test]
    fn detect_tracker_drift_closed_children_unchecked() {
        // Simulates a tracker body where issue #5 is listed as unchecked
        // but in reality the issue is closed. The housekeeping sweep should
        // detect this as "Closed children not checked off".
        let tracker_body = "- [ ] #5 Implement feature\n- [x] #6 Setup CI\n- [ ] #7 Add tests";
        let completed = parse_completed(tracker_body);
        let pending = parse_pending(tracker_body);

        // #5 and #7 are pending (unchecked)
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().any(|p| p.number == 5));
        assert!(pending.iter().any(|p| p.number == 7));
        // #6 is completed (checked)
        assert!(completed.contains(&6));
        assert!(!completed.contains(&5));

        // If we know issue #5 is closed (state:closed), it should be flagged
        // as tracker drift — the checkbox should be ticked.
        let closed_issues: HashSet<u32> = HashSet::from([5]);
        let drifted: Vec<&PendingIssue> = pending
            .iter()
            .filter(|p| closed_issues.contains(&p.number))
            .collect();
        assert_eq!(drifted.len(), 1);
        assert_eq!(drifted[0].number, 5);
    }
}
