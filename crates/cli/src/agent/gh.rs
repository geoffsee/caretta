//! Shared wrapper around the `gh` GitHub CLI.
//!
//! All caretta code that shells out to `gh` should go through [`Gh`] so the
//! program name, default behaviour, and any cross-cutting concerns (logging,
//! redaction, future auth wiring) live in one place.
//!
//! The surface is split across behaviour traits ([`PullRequestActions`],
//! [`IssueActions`], [`RepoActions`]) so call sites read like English:
//! `Gh::merge_pr_squash(n)`, `Gh::close_issue(n)`,
//! `Gh::repo_name_with_owner()`. JSON projections, jq selectors, GraphQL
//! plumbing, and other gh-flag noise are hidden inside the wrapper —
//! callers ask for a named piece of information (`pr_body_or_die`,
//! `pr_diagnostic_json`, `fetch_pr_review_threads_json`) and the wrapper
//! handles the encoding. The low-level [`Gh::stdout`] / [`Gh::run`] escape
//! hatches remain for one-off invocations with no semantic counterpart yet.

use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_run_env, cmd_stdout, cmd_stdout_or_die, die, has_command,
};

const GH: &str = "gh";

/// GraphQL mutation that marks one review thread as resolved on a pull
/// request. Exposed at module scope so contract tests can assert the
/// operation name, variables, and selected fields remain intact across
/// refactors. The leading newline keeps `gh` from interpreting a leading
/// `@` (if one were ever introduced) as a file reference.
pub(crate) const RESOLVE_REVIEW_THREAD_MUTATION: &str = "\nmutation($threadId: ID!) {\n  resolveReviewThread(input: {threadId: $threadId}) {\n    thread { id isResolved }\n  }\n}";

/// Namespace handle for `gh` CLI invocations.
pub struct Gh;

impl Gh {
    /// Whether the `gh` binary is reachable on `PATH`.
    pub fn is_installed() -> bool {
        has_command(GH)
    }

    /// Abort the process with `message` when `gh` is not on `PATH`.
    pub fn require_installed_or_die(message: &str) -> ! {
        die(message)
    }

    /// Run `gh <args>` and return trimmed stdout, or `None` on failure.
    pub fn stdout(args: &[&str]) -> Option<String> {
        cmd_stdout(GH, args)
    }

    /// Run `gh <args>` and return trimmed stdout, dying with `context` on
    /// failure.
    pub fn stdout_or_die(args: &[&str], context: &str) -> String {
        cmd_stdout_or_die(GH, args, context)
    }

    /// Run `gh <args>` and return `(success, combined_stdout_stderr)`.
    pub fn capture(args: &[&str]) -> (bool, String) {
        cmd_capture(GH, args)
    }

    /// Run `gh <args>` inheriting stdio. Returns success.
    pub fn run(args: &[&str]) -> bool {
        cmd_run(GH, args)
    }

    /// Run `gh <args>` with additional env vars, inheriting stdio. Returns
    /// success.
    pub fn run_env(args: &[&str], env: &[(String, String)]) -> bool {
        cmd_run_env(GH, args, env)
    }
}

// ── Pull requests ────────────────────────────────────────────────────────

/// Single-PR fields and PR-wide actions.
///
/// Method names describe what's being read or mutated (`pr_body_or_die`,
/// `merge_pr_squash`, `enable_pr_auto_merge_squash`), so call sites don't
/// have to decode JSON field lists, jq selectors, or argv ordering.
pub trait PullRequestActions {
    /// PR body / description, aborting the process on failure.
    fn pr_body_or_die(pr_num: u32) -> String;

    /// PR head branch (ref name), aborting the process on failure.
    fn pr_head_ref_or_die(pr_num: u32) -> String;

    /// PR base branch (ref name), or `None` when `gh` cannot resolve it.
    fn pr_base_ref(pr_num: u32) -> Option<String>;

    /// GitHub `reviewDecision` for a PR — one of `APPROVED`,
    /// `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or an empty string when no
    /// reviews have been submitted. `None` on `gh` failure.
    fn pr_review_decision(pr_num: u32) -> Option<String>;

    /// Raw `autoMergeRequest` jq projection — `Some("null")` when auto-merge
    /// is disabled, `Some("{...}")` when enabled, `None` on `gh` failure.
    /// Most callers want [`pr_is_auto_merge_enabled`](Self::pr_is_auto_merge_enabled).
    fn pr_auto_merge_status_raw(pr_num: u32) -> Option<String>;

    /// True when GitHub's squash auto-merge is currently enabled on a PR.
    /// `false` when not enabled, when `gh` fails, or when the response
    /// shape can't be parsed — callers that need to distinguish those cases
    /// should reach for [`pr_auto_merge_status_raw`](Self::pr_auto_merge_status_raw).
    fn pr_is_auto_merge_enabled(pr_num: u32) -> bool;

    /// Concatenated bodies of every comment on a PR, separated by newlines
    /// (`gh pr view <pr> --json comments --jq .comments[].body`). Useful for
    /// "does any comment contain marker X?" checks. `None` on `gh` failure.
    fn pr_comment_bodies(pr_num: u32) -> Option<String>;

    /// `gh pr diff <pr>` — full PR diff, aborting the process on failure.
    fn pr_diff_or_die(pr_num: u32) -> String;

    /// Raw JSON: `{"comments": [...]}` for a PR. Used by the conflicts module
    /// to parse latest marker context out of comment metadata.
    fn pr_comments_json(pr_num: u32) -> Option<String>;

    /// Raw JSON of the fields needed for conflict-aware PR views:
    /// `headRefName`, `baseRefName`, `mergeStateStatus`, `title`.
    fn pr_conflict_view_json(pr_num: u32) -> Option<String>;

    /// Raw JSON of the fields needed for [`crate::agent::fix_pr`]'s
    /// diagnostic — `number`, `title`, `headRefName`, `baseRefName`,
    /// `isDraft`, `mergeStateStatus`, `reviewDecision`, `statusCheckRollup`.
    fn pr_diagnostic_json(pr_num: u32) -> Option<String>;

    /// Raw JSON of the fields auto-merge refreshes after a retarget:
    /// `mergeStateStatus`, `reviewDecision`, `isDraft`.
    fn pr_status_refresh_json(pr_num: u32) -> Option<String>;

    /// Raw JSON of `reviews` on a PR — list of submitted reviews used to
    /// render prior-review context for the agent.
    fn pr_reviews_json(pr_num: u32) -> Option<String>;

    /// Raw JSON of `number,title,headRefName` for the PR backing the
    /// current branch (no PR number argument — `gh pr view` infers it).
    fn current_branch_pr_summary_json() -> Option<String>;

    /// URL of the first open PR whose head matches `branch`, or empty
    /// string when none exist. Returns `(gh_succeeded, url_or_empty)` so
    /// callers can distinguish "no open PR" from "gh failed".
    fn find_open_pr_url_for_head(branch: &str) -> (bool, String);

    /// Head ref of the first open PR whose head matches `branch` — the
    /// caller asks "is the upstream branch still alive on GitHub?". `None`
    /// when no PR matches or `gh` fails.
    fn find_open_pr_head_for_head(branch: &str) -> Option<String>;

    /// Number of the first open PR whose head matches `branch`. `None` when
    /// no PR matches or `gh` fails.
    fn find_open_pr_number_for_head(branch: &str) -> Option<u32>;

    /// Open PR summaries (`number`, `title`, `headRefName`, `author`) up to
    /// `limit`. Used to populate the tracker sidebar and workflow context.
    fn open_pr_summaries_json(limit: u32) -> Option<String>;

    /// Recently merged PR summaries (`number`, `title`, `mergedAt`) up to
    /// `limit`. Used by the retrospective workflow.
    fn merged_pr_summaries_json(limit: u32) -> Option<String>;

    /// Open PR rows shaped for auto-merge lineage analysis: `number`,
    /// `headRefName`, `baseRefName`, `isDraft`, `mergeStateStatus`,
    /// `reviewDecision`. Aborts with `context` on failure since auto-merge
    /// can't proceed without the roster.
    fn open_merge_candidate_pr_rows_or_die(context: &str) -> String;

    /// Open PR rows shaped for auto-merge lineage analysis, returned as raw
    /// JSON or `None` when `gh` fails (best-effort variant used by the
    /// lineage refresh pass).
    fn try_open_merge_candidate_pr_rows() -> Option<String>;

    /// `gh pr create --head --base --title --body` — true on success.
    /// Caller is responsible for checking no PR already exists for `head`.
    fn create_pr(head: &str, base: &str, title: &str, body: &str) -> bool;

    /// `gh pr update-branch <pr>` — merge the latest base into the head
    /// branch. Inherits stdio.
    fn update_pr_branch(pr_num: u32) -> bool;

    /// Like [`update_pr_branch`](Self::update_pr_branch), but captures the
    /// combined output so the caller can log the failure reason.
    fn update_pr_branch_capture(pr_num: u32) -> (bool, String);

    /// `gh pr edit <pr> --base <new_base>` — retarget a PR at a new base
    /// branch.
    fn edit_pr_base(pr_num: u32, new_base: &str) -> bool;

    /// `gh pr merge <pr> --squash` — immediate squash merge. Captures
    /// combined output for the failure path.
    fn merge_pr_squash(pr_num: u32) -> (bool, String);

    /// `gh pr merge <pr> --auto --squash` — turn on GitHub auto-merge with
    /// the squash strategy (merges once branch protection allows it).
    fn enable_pr_auto_merge_squash(pr_num: u32) -> (bool, String);

    /// `gh pr comment <pr> --body <body>` — post a PR comment.
    fn comment_on_pr(pr_num: u32, body: &str) -> (bool, String);

    /// `gh pr review <pr> <action> --body <body>` with extra env (typically
    /// a bot `GH_TOKEN`, since GitHub rejects self-approval). `action` is the
    /// argv flag — `"--approve"`, `"--comment"`, or `"--request-changes"`.
    fn submit_pr_review_with_env(
        pr_num: u32,
        action: &str,
        body: &str,
        env: &[(String, String)],
    ) -> bool;

    /// Run the `resolveReviewThread` GraphQL mutation against `thread_id`.
    /// Returns the raw response body so callers can confirm
    /// `data.resolveReviewThread.thread.isResolved`; `None` on `gh` failure.
    fn mark_review_thread_resolved(thread_id: &str) -> Option<String>;

    /// Raw JSON from the `reviewThreads` GraphQL query for `pr_num`.
    /// Resolves the working directory's repository via
    /// [`RepoActions::repo_name_with_owner`] internally. `None` when the
    /// repo can't be identified or `gh` fails.
    fn fetch_pr_review_threads_json(pr_num: u32) -> Option<String>;

    /// Raw JSON from the batched `repository.pullRequests` GraphQL query
    /// — every open PR's `reviewThreads` in one round-trip. Resolves the
    /// working directory's repository internally. Used by tracker refresh
    /// so per-PR `(N)` badges stay in sync without N round-trips.
    fn fetch_open_pr_review_threads_batched_json() -> Option<String>;
}

impl PullRequestActions for Gh {
    fn pr_body_or_die(pr_num: u32) -> String {
        let num_s = pr_num.to_string();
        Self::stdout_or_die(
            &["pr", "view", &num_s, "--json", "body", "--jq", ".body"],
            "failed to fetch PR body",
        )
    }

    fn pr_head_ref_or_die(pr_num: u32) -> String {
        let num_s = pr_num.to_string();
        Self::stdout_or_die(
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

    fn pr_base_ref(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "baseRefName",
            "--jq",
            ".baseRefName",
        ])
    }

    fn pr_review_decision(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "reviewDecision",
            "--jq",
            ".reviewDecision // \"\"",
        ])
    }

    fn pr_auto_merge_status_raw(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "autoMergeRequest",
            "--jq",
            ".autoMergeRequest",
        ])
    }

    fn pr_is_auto_merge_enabled(pr_num: u32) -> bool {
        match Self::pr_auto_merge_status_raw(pr_num) {
            Some(s) => !s.is_empty() && s != "null",
            None => false,
        }
    }

    fn pr_comment_bodies(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "comments",
            "--jq",
            ".comments[].body",
        ])
    }

    fn pr_diff_or_die(pr_num: u32) -> String {
        let num_s = pr_num.to_string();
        Self::stdout_or_die(&["pr", "diff", &num_s], "failed to fetch PR diff")
    }

    fn pr_comments_json(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&["pr", "view", &num_s, "--json", "comments"])
    }

    fn pr_conflict_view_json(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "headRefName,baseRefName,mergeStateStatus,title",
        ])
    }

    fn pr_diagnostic_json(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "number,title,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision,statusCheckRollup",
        ])
    }

    fn pr_status_refresh_json(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "mergeStateStatus,reviewDecision,isDraft",
        ])
    }

    fn pr_reviews_json(pr_num: u32) -> Option<String> {
        let num_s = pr_num.to_string();
        Self::stdout(&["pr", "view", &num_s, "--json", "reviews"])
    }

    fn current_branch_pr_summary_json() -> Option<String> {
        Self::stdout(&["pr", "view", "--json", "number,title,headRefName"])
    }

    fn find_open_pr_url_for_head(branch: &str) -> (bool, String) {
        Self::capture(&[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "url",
            "-q",
            ".[0].url // empty",
        ])
    }

    fn find_open_pr_head_for_head(branch: &str) -> Option<String> {
        Self::stdout(&[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "headRefName",
            "--jq",
            ".[0].headRefName",
        ])
    }

    fn find_open_pr_number_for_head(branch: &str) -> Option<u32> {
        let raw = Self::stdout(&[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            ".[0].number // empty",
        ])?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse().ok()
    }

    fn open_pr_summaries_json(limit: u32) -> Option<String> {
        let limit_s = limit.to_string();
        Self::stdout(&[
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,headRefName,author",
            "--limit",
            &limit_s,
        ])
    }

    fn merged_pr_summaries_json(limit: u32) -> Option<String> {
        let limit_s = limit.to_string();
        Self::stdout(&[
            "pr",
            "list",
            "--state",
            "merged",
            "--json",
            "number,title,mergedAt",
            "--limit",
            &limit_s,
        ])
    }

    fn open_merge_candidate_pr_rows_or_die(context: &str) -> String {
        Self::stdout_or_die(
            &[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "150",
                "--json",
                "number,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision",
            ],
            context,
        )
    }

    fn try_open_merge_candidate_pr_rows() -> Option<String> {
        Self::stdout(&[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "150",
            "--json",
            "number,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision",
        ])
    }

    fn create_pr(head: &str, base: &str, title: &str, body: &str) -> bool {
        Self::run(&[
            "pr", "create", "--head", head, "--base", base, "--title", title, "--body", body,
        ])
    }

    fn update_pr_branch(pr_num: u32) -> bool {
        Self::run(&["pr", "update-branch", &pr_num.to_string()])
    }

    fn update_pr_branch_capture(pr_num: u32) -> (bool, String) {
        Self::capture(&["pr", "update-branch", &pr_num.to_string()])
    }

    fn edit_pr_base(pr_num: u32, new_base: &str) -> bool {
        Self::run(&["pr", "edit", &pr_num.to_string(), "--base", new_base])
    }

    fn merge_pr_squash(pr_num: u32) -> (bool, String) {
        Self::capture(&["pr", "merge", &pr_num.to_string(), "--squash"])
    }

    fn enable_pr_auto_merge_squash(pr_num: u32) -> (bool, String) {
        Self::capture(&["pr", "merge", &pr_num.to_string(), "--auto", "--squash"])
    }

    fn comment_on_pr(pr_num: u32, body: &str) -> (bool, String) {
        Self::capture(&["pr", "comment", &pr_num.to_string(), "--body", body])
    }

    fn submit_pr_review_with_env(
        pr_num: u32,
        action: &str,
        body: &str,
        env: &[(String, String)],
    ) -> bool {
        Self::run_env(
            &["pr", "review", &pr_num.to_string(), action, "--body", body],
            env,
        )
    }

    fn mark_review_thread_resolved(thread_id: &str) -> Option<String> {
        Self::graphql_query(RESOLVE_REVIEW_THREAD_MUTATION, &[("threadId", thread_id)])
    }

    fn fetch_pr_review_threads_json(pr_num: u32) -> Option<String> {
        let owner_repo = Self::repo_name_with_owner().filter(|s| !s.is_empty())?;
        let (owner, repo) = owner_repo.split_once('/')?;
        let owner = owner.to_string();
        let repo = repo.to_string();

        const QUERY: &str = "\nquery($owner: String!, $repo: String!, $number: Int!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequest(number: $number) {\n      reviewThreads(first: 100) {\n        nodes {\n          id\n          isResolved\n          comments(first: 100) {\n            nodes {\n              author { login __typename }\n              path\n              line\n              originalLine\n              body\n            }\n          }\n        }\n      }\n    }\n  }\n}";

        let pr_num_s = pr_num.to_string();
        Self::graphql_query(
            QUERY,
            &[
                ("owner", owner.as_str()),
                ("repo", repo.as_str()),
                ("number", pr_num_s.as_str()),
            ],
        )
    }

    fn fetch_open_pr_review_threads_batched_json() -> Option<String> {
        let owner_repo = Self::repo_name_with_owner().filter(|s| !s.is_empty())?;
        let (owner, repo) = owner_repo.split_once('/')?;
        let owner = owner.to_string();
        let repo = repo.to_string();

        const QUERY: &str = "\nquery($owner: String!, $repo: String!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequests(states: OPEN, first: 100) {\n      nodes {\n        number\n        reviewThreads(first: 100) {\n          nodes {\n            isResolved\n            comments(first: 1) {\n              nodes {\n                author { login __typename }\n                body\n              }\n            }\n          }\n        }\n      }\n    }\n  }\n}";

        Self::graphql_query(QUERY, &[("owner", owner.as_str()), ("repo", repo.as_str())])
    }
}

// ── Issues ──────────────────────────────────────────────────────────────

/// `gh issue ...` behaviours. Field projections are baked into each method
/// so call sites don't have to know the GraphQL field names.
pub trait IssueActions {
    /// Issue body / description, aborting with `context` on failure.
    fn issue_body_or_die(issue_num: u32, context: &str) -> String;

    /// Issue title, aborting with `context` on failure.
    fn issue_title_or_die(issue_num: u32, context: &str) -> String;

    /// Rewrite the issue body (`gh issue edit <num> --body <body>`).
    fn edit_issue_body(issue_num: u32, body: &str) -> bool;

    /// Close an issue (`gh issue close <num>`).
    fn close_issue(issue_num: u32) -> bool;

    /// Open issue summaries (`number`, `title`) for issues carrying `label`.
    fn open_issue_summaries_with_label_json(label: &str) -> Option<String>;

    /// Open issue summaries (`number`, `title`, `labels`) up to `limit`.
    /// Used by workflow context gatherers that need a quick roster.
    fn open_issue_summaries_json(limit: u32) -> Option<String>;

    /// Open issue rows with the extra fields housekeeping needs
    /// (`number`, `title`, `labels`, `updatedAt`, `assignees`) up to `limit`.
    fn open_issue_housekeeping_json(limit: u32) -> Option<String>;

    /// Recently closed issue summaries (`number`, `title`, `closedAt`)
    /// up to `limit`. Used by the retrospective workflow.
    fn closed_issue_summaries_json(limit: u32) -> Option<String>;

    /// Open issue numbers whose titles match the gh search expression
    /// `search` (e.g. `"retro in:title"`). Returned as one newline-delimited
    /// number per line (raw `gh` output).
    fn open_issue_numbers_matching_title(search: &str) -> Option<String>;

    /// Body of the most recent open issue carrying `label`, formatted as
    /// `# <title>\n\n<body>`. Empty string when none found or `gh` fails.
    fn first_open_issue_body_for_label(label: &str) -> String;
}

impl IssueActions for Gh {
    fn issue_body_or_die(issue_num: u32, context: &str) -> String {
        let num_s = issue_num.to_string();
        Self::stdout_or_die(
            &["issue", "view", &num_s, "--json", "body", "--jq", ".body"],
            context,
        )
    }

    fn issue_title_or_die(issue_num: u32, context: &str) -> String {
        let num_s = issue_num.to_string();
        Self::stdout_or_die(
            &["issue", "view", &num_s, "--json", "title", "--jq", ".title"],
            context,
        )
    }

    fn edit_issue_body(issue_num: u32, body: &str) -> bool {
        Self::run(&["issue", "edit", &issue_num.to_string(), "--body", body])
    }

    fn close_issue(issue_num: u32) -> bool {
        Self::run(&["issue", "close", &issue_num.to_string()])
    }

    fn open_issue_summaries_with_label_json(label: &str) -> Option<String> {
        Self::stdout(&[
            "issue",
            "list",
            "--label",
            label,
            "--state",
            "open",
            "--json",
            "number,title",
        ])
    }

    fn open_issue_summaries_json(limit: u32) -> Option<String> {
        let limit_s = limit.to_string();
        Self::stdout(&[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,labels",
            "--limit",
            &limit_s,
        ])
    }

    fn open_issue_housekeeping_json(limit: u32) -> Option<String> {
        let limit_s = limit.to_string();
        Self::stdout(&[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,labels,updatedAt,assignees",
            "--limit",
            &limit_s,
        ])
    }

    fn closed_issue_summaries_json(limit: u32) -> Option<String> {
        let limit_s = limit.to_string();
        Self::stdout(&[
            "issue",
            "list",
            "--state",
            "closed",
            "--json",
            "number,title,closedAt",
            "--limit",
            &limit_s,
        ])
    }

    fn open_issue_numbers_matching_title(search: &str) -> Option<String> {
        Self::stdout(&[
            "issue",
            "list",
            "--search",
            search,
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            ".[].number",
        ])
    }

    fn first_open_issue_body_for_label(label: &str) -> String {
        Self::stdout(&[
            "issue",
            "list",
            "--label",
            label,
            "--state",
            "open",
            "--limit",
            "1",
            "--json",
            "number,title,body",
            "--jq",
            ".[0] // empty | \"# \\(.title)\\n\\n\\(.body)\"",
        ])
        .unwrap_or_default()
    }
}

// ── Repository ──────────────────────────────────────────────────────────

/// `gh repo ...` behaviours. Just enough to identify the repo today;
/// extend as new call sites need it.
pub trait RepoActions {
    /// `owner/repo` slug for the working directory's repository, or `None`
    /// when `gh` cannot resolve it.
    fn repo_name_with_owner() -> Option<String>;
}

impl RepoActions for Gh {
    fn repo_name_with_owner() -> Option<String> {
        Self::stdout(&[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ])
    }
}

// ── GraphQL (internal) ──────────────────────────────────────────────────

impl Gh {
    /// Run a GraphQL query/mutation via `gh api graphql`. Private — the
    /// semantic GraphQL-backed methods on [`PullRequestActions`]
    /// (`mark_review_thread_resolved`, `fetch_pr_review_threads_json`,
    /// `fetch_open_pr_review_threads_batched_json`) are the public surface.
    ///
    /// Variables are passed as `-F key=value` pairs so `gh` can apply its
    /// usual type inference (numeric strings become `Int`, `true`/`false`
    /// become `Boolean`, anything else is a `String`). The query is passed
    /// via `-f query=<query>` so `gh` does not interpret a leading `@` as a
    /// file reference. Returns the raw response body, or `None` if the call
    /// fails.
    fn graphql_query(query: &str, vars: &[(&str, &str)]) -> Option<String> {
        let var_strings: Vec<String> = vars.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let query_arg = format!("query={query}");

        let mut args: Vec<&str> = vec!["api", "graphql"];
        for s in &var_strings {
            args.push("-F");
            args.push(s.as_str());
        }
        args.push("-f");
        args.push(&query_arg);

        Self::stdout(&args)
    }
}
