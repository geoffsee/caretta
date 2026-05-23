//! GitHub `gh` CLI binding for the developer platform.
//!
//! All caretta code that shells out to `gh` should go through [`Gh`] so the
//! program name, default behaviour, and any cross-cutting concerns (logging,
//! redaction, future auth wiring) live in one place.
//!
//! The trait surface this implements lives in
//! [`crate::agent::platform`] — [`PullRequestActions`], [`IssueActions`],
//! [`RepoActions`], and the [`DeveloperPlatform`] umbrella. Call sites read
//! like English: `Gh::merge_pr_squash(n)`, `Gh::close_issue(n)`,
//! `Gh::repo_name_with_owner()`. JSON projections, jq selectors, GraphQL
//! plumbing, and other gh-flag noise are hidden inside the wrapper —
//! callers ask for a named piece of information (`pr_body_or_die`,
//! `pr_diagnostic_json`, `fetch_pr_review_threads_json`) and the wrapper
//! handles the encoding. The low-level [`Gh::stdout`] / [`Gh::run`] escape
//! hatches remain for one-off invocations with no semantic counterpart yet.
//!
//! `Gh` is one binding of the platform abstraction; future bindings
//! (e.g. a self-hosted Gitea instance) implement the same trait set
//! against their native APIs.

use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_run_env, cmd_stdout, cmd_stdout_or_die, die, has_command,
};
use crate::agent::platform::{
    ClosedIssueSummary, CurrentBranchPrSummary, DeveloperPlatform, IssueActions, IssueAssignee,
    IssueLabel, IssueSummary, MergedPrSummary, OpenIssueHousekeeping, OpenIssueSummary,
    OpenMergeCandidatePr, OpenPrReviewThreads, PlatformCheckStatus, PrCommentRecord, PrComments,
    PrConflictView, PrDiagnostic, PrReviewSummaryRecord, PrReviewThread, PrReviewThreadComment,
    PrStatusRefresh, PullRequestActions, RepoActions, map_approval_gate, map_integration_readiness,
};
use serde::Deserialize;

const GH: &str = "gh";

/// GraphQL mutation that marks one review thread as resolved on a pull
/// request. Exposed at module scope so contract tests can assert the
/// operation name, variables, and selected fields remain intact across
/// refactors. The leading newline keeps `gh` from interpreting a leading
/// `@` (if one were ever introduced) as a file reference.
pub(crate) const RESOLVE_REVIEW_THREAD_MUTATION: &str = "\nmutation($threadId: ID!) {\n  resolveReviewThread(input: {threadId: $threadId}) {\n    thread { id isResolved }\n  }\n}";

/// Namespace handle for `gh` CLI invocations.
pub struct Gh;

#[derive(Debug, Deserialize)]
struct GhPrComments {
    comments: Vec<GhPrComment>,
}

#[derive(Debug, Deserialize)]
struct GhPrComment {
    body: String,
}

#[derive(Debug, Deserialize)]
struct GhPrConflictView {
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    title: String,
}

#[derive(Debug, Deserialize)]
struct GhPrDiagnostic {
    number: u32,
    title: String,
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(rename = "statusCheckRollup", default)]
    status_check_rollup: Vec<GhPlatformCheckStatus>,
}

#[derive(Debug, Deserialize)]
struct GhPlatformCheckStatus {
    #[serde(rename = "__typename", default)]
    typename: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "targetUrl", default)]
    target_url: Option<String>,
    #[serde(rename = "detailsUrl", default)]
    details_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrStatusRefresh {
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(rename = "isDraft")]
    is_draft: bool,
}

#[derive(Debug, Deserialize)]
struct GhPrReviews {
    reviews: Vec<GhPrReview>,
}

#[derive(Debug, Deserialize)]
struct GhPrReview {
    author: Option<GhAuthor>,
    state: Option<String>,
    #[serde(rename = "submittedAt")]
    submitted_at: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhAuthor {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadsResponse {
    data: GhReviewThreadsData,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadsData {
    repository: GhReviewThreadsRepo,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadsRepo {
    #[serde(rename = "pullRequest")]
    pull_request: GhPullRequestThreads,
}

#[derive(Debug, Deserialize)]
struct GhPullRequestThreads {
    #[serde(rename = "reviewThreads")]
    review_threads: GhReviewThreadsNodes,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadsNodes {
    nodes: Vec<GhReviewThread>,
}

#[derive(Debug, Deserialize)]
struct GhReviewThread {
    id: String,
    #[serde(rename = "isResolved")]
    is_resolved: bool,
    comments: GhReviewThreadComments,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadComments {
    nodes: Vec<GhReviewComment>,
}

#[derive(Debug, Deserialize)]
struct GhReviewComment {
    author: Option<GhReviewCommentAuthor>,
    path: Option<String>,
    line: Option<u32>,
    #[serde(rename = "originalLine")]
    original_line: Option<u32>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhReviewCommentAuthor {
    login: Option<String>,
    #[serde(rename = "__typename")]
    typename: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhOpenMergeCandidatePr {
    number: u32,
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
}

impl From<GhOpenMergeCandidatePr> for OpenMergeCandidatePr {
    fn from(value: GhOpenMergeCandidatePr) -> Self {
        Self {
            number: value.number,
            head_ref: value.head_ref,
            base_ref: value.base_ref,
            is_draft: value.is_draft,
            integration_readiness: value
                .merge_state_status
                .map(|s| map_integration_readiness(&s)),
            approval_gate: value.review_decision.map(|s| map_approval_gate(&s)),
        }
    }
}

fn parse_open_merge_candidate_prs_or_die(raw: &str, context: &str) -> Vec<OpenMergeCandidatePr> {
    match serde_json::from_str::<Vec<GhOpenMergeCandidatePr>>(raw) {
        Ok(rows) => rows.into_iter().map(Into::into).collect(),
        Err(err) => die(&format!("{context}: failed to parse open PR rows: {err}")),
    }
}

fn map_review_thread(thread: GhReviewThread) -> PrReviewThread {
    PrReviewThread {
        id: thread.id,
        is_resolved: thread.is_resolved,
        comments: thread
            .comments
            .nodes
            .into_iter()
            .map(|c| PrReviewThreadComment {
                author_login: c
                    .author
                    .as_ref()
                    .and_then(|a| a.login.clone())
                    .unwrap_or_default(),
                author_type: c.author.and_then(|a| a.typename),
                path: c.path,
                line: c.line,
                original_line: c.original_line,
                body: c.body.unwrap_or_default(),
            })
            .collect(),
    }
}

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

    fn pr_comments(pr_num: u32) -> Option<PrComments> {
        let num_s = pr_num.to_string();
        let raw = Self::stdout(&["pr", "view", &num_s, "--json", "comments"])?;
        let parsed: GhPrComments = serde_json::from_str(&raw).ok()?;
        Some(PrComments {
            comments: parsed
                .comments
                .into_iter()
                .map(|c| PrCommentRecord { body: c.body })
                .collect(),
        })
    }

    fn pr_conflict_view(pr_num: u32) -> Option<PrConflictView> {
        let num_s = pr_num.to_string();
        let raw = Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "headRefName,baseRefName,mergeStateStatus,title",
        ])?;
        let parsed: GhPrConflictView = serde_json::from_str(&raw).ok()?;
        Some(PrConflictView {
            head_ref: parsed.head_ref,
            base_ref: parsed.base_ref,
            integration_readiness: parsed
                .merge_state_status
                .map(|s| map_integration_readiness(&s)),
            title: parsed.title,
        })
    }

    fn pr_diagnostic(pr_num: u32) -> Option<PrDiagnostic> {
        let num_s = pr_num.to_string();
        let raw = Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "number,title,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision,statusCheckRollup",
        ])?;
        let parsed: GhPrDiagnostic = serde_json::from_str(&raw).ok()?;
        Some(PrDiagnostic {
            number: parsed.number,
            title: parsed.title,
            head_ref: parsed.head_ref,
            base_ref: parsed.base_ref,
            is_draft: parsed.is_draft,
            integration_readiness: parsed
                .merge_state_status
                .map(|s| map_integration_readiness(&s)),
            approval_gate: parsed.review_decision.map(|s| map_approval_gate(&s)),
            status_check_rollup: parsed
                .status_check_rollup
                .into_iter()
                .map(|c| PlatformCheckStatus {
                    typename: c.typename,
                    name: c.name,
                    context: c.context,
                    state: c.state,
                    conclusion: c.conclusion,
                    status: c.status,
                    target_url: c.target_url,
                    details_url: c.details_url,
                })
                .collect(),
        })
    }

    fn pr_status_refresh(pr_num: u32) -> Option<PrStatusRefresh> {
        let num_s = pr_num.to_string();
        let raw = Self::stdout(&[
            "pr",
            "view",
            &num_s,
            "--json",
            "mergeStateStatus,reviewDecision,isDraft",
        ])?;
        let parsed: GhPrStatusRefresh = serde_json::from_str(&raw).ok()?;
        Some(PrStatusRefresh {
            integration_readiness: parsed
                .merge_state_status
                .map(|s| map_integration_readiness(&s)),
            approval_gate: parsed.review_decision.map(|s| map_approval_gate(&s)),
            is_draft: parsed.is_draft,
        })
    }

    fn pr_reviews(pr_num: u32) -> Option<Vec<PrReviewSummaryRecord>> {
        let num_s = pr_num.to_string();
        let raw = Self::stdout(&["pr", "view", &num_s, "--json", "reviews"])?;
        let parsed: GhPrReviews = serde_json::from_str(&raw).ok()?;
        Some(
            parsed
                .reviews
                .into_iter()
                .map(|r| PrReviewSummaryRecord {
                    author_login: r.author.map(|a| a.login).unwrap_or_default(),
                    state: r.state.unwrap_or_default(),
                    submitted_at: r.submitted_at.unwrap_or_default(),
                    body: r.body.unwrap_or_default(),
                })
                .collect(),
        )
    }

    fn current_branch_pr_summary() -> Option<CurrentBranchPrSummary> {
        let raw = Self::stdout(&["pr", "view", "--json", "number,title,headRefName"])?;
        #[derive(Deserialize)]
        struct Row {
            number: u32,
            title: String,
            #[serde(rename = "headRefName")]
            head_ref: String,
        }
        let row: Row = serde_json::from_str(&raw).ok()?;
        Some(CurrentBranchPrSummary {
            number: row.number,
            title: row.title,
            head_ref: row.head_ref,
        })
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

    fn open_pr_summaries(limit: u32) -> Option<Vec<cli_common::PrSummary>> {
        let limit_s = limit.to_string();
        let raw = Self::stdout(&[
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,headRefName,author",
            "--limit",
            &limit_s,
        ])?;
        serde_json::from_str(&raw).ok()
    }

    fn merged_pr_summaries(limit: u32) -> Option<Vec<MergedPrSummary>> {
        let limit_s = limit.to_string();
        let raw = Self::stdout(&[
            "pr",
            "list",
            "--state",
            "merged",
            "--json",
            "number,title,mergedAt",
            "--limit",
            &limit_s,
        ])?;
        #[derive(Deserialize)]
        struct Row {
            number: u32,
            title: String,
            #[serde(rename = "mergedAt")]
            merged_at: String,
        }
        let rows: Vec<Row> = serde_json::from_str(&raw).ok()?;
        Some(
            rows.into_iter()
                .map(|r| MergedPrSummary {
                    number: r.number,
                    title: r.title,
                    merged_at: r.merged_at,
                })
                .collect(),
        )
    }

    fn open_merge_candidate_prs_or_die(context: &str) -> Vec<OpenMergeCandidatePr> {
        let raw = Self::stdout_or_die(
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
        );
        parse_open_merge_candidate_prs_or_die(&raw, context)
    }

    fn try_open_merge_candidate_prs() -> Option<Vec<OpenMergeCandidatePr>> {
        let raw = Self::stdout(&[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "150",
            "--json",
            "number,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision",
        ])?;
        serde_json::from_str::<Vec<GhOpenMergeCandidatePr>>(&raw)
            .ok()
            .map(|rows| rows.into_iter().map(Into::into).collect())
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

    fn fetch_pr_review_threads(pr_num: u32) -> Option<Vec<PrReviewThread>> {
        let owner_repo = Self::repo_name_with_owner().filter(|s| !s.is_empty())?;
        let (owner, repo) = owner_repo.split_once('/')?;
        let owner = owner.to_string();
        let repo = repo.to_string();

        const QUERY: &str = "\nquery($owner: String!, $repo: String!, $number: Int!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequest(number: $number) {\n      reviewThreads(first: 100) {\n        nodes {\n          id\n          isResolved\n          comments(first: 100) {\n            nodes {\n              author { login __typename }\n              path\n              line\n              originalLine\n              body\n            }\n          }\n        }\n      }\n    }\n  }\n}";

        let pr_num_s = pr_num.to_string();
        let raw = Self::graphql_query(
            QUERY,
            &[
                ("owner", owner.as_str()),
                ("repo", repo.as_str()),
                ("number", pr_num_s.as_str()),
            ],
        )?;
        let parsed: GhReviewThreadsResponse = serde_json::from_str(&raw).ok()?;
        Some(
            parsed
                .data
                .repository
                .pull_request
                .review_threads
                .nodes
                .into_iter()
                .map(map_review_thread)
                .collect(),
        )
    }

    fn fetch_open_pr_review_threads_batched() -> Option<Vec<OpenPrReviewThreads>> {
        let owner_repo = Self::repo_name_with_owner().filter(|s| !s.is_empty())?;
        let (owner, repo) = owner_repo.split_once('/')?;
        let owner = owner.to_string();
        let repo = repo.to_string();

        const QUERY: &str = "\nquery($owner: String!, $repo: String!) {\n  repository(owner: $owner, name: $repo) {\n    pullRequests(states: OPEN, first: 100) {\n      nodes {\n        number\n        reviewThreads(first: 100) {\n          nodes {\n            isResolved\n            comments(first: 1) {\n              nodes {\n                author { login __typename }\n                body\n              }\n            }\n          }\n        }\n      }\n    }\n  }\n}";

        let raw =
            Self::graphql_query(QUERY, &[("owner", owner.as_str()), ("repo", repo.as_str())])?;
        #[derive(Deserialize)]
        struct Root {
            data: Data,
        }
        #[derive(Deserialize)]
        struct Data {
            repository: Repo,
        }
        #[derive(Deserialize)]
        struct Repo {
            #[serde(rename = "pullRequests")]
            pull_requests: PullRequests,
        }
        #[derive(Deserialize)]
        struct PullRequests {
            nodes: Vec<PullRequestNode>,
        }
        #[derive(Deserialize)]
        struct PullRequestNode {
            number: u32,
            #[serde(rename = "reviewThreads")]
            review_threads: GhReviewThreadsNodes,
        }
        let parsed: Root = serde_json::from_str(&raw).ok()?;
        Some(
            parsed
                .data
                .repository
                .pull_requests
                .nodes
                .into_iter()
                .map(|pr| OpenPrReviewThreads {
                    pr_number: pr.number,
                    review_threads: pr
                        .review_threads
                        .nodes
                        .into_iter()
                        .map(map_review_thread)
                        .collect(),
                })
                .collect(),
        )
    }
}

// ── Issues ──────────────────────────────────────────────────────────────

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

    fn open_issue_summaries_with_label(label: &str) -> Option<Vec<IssueSummary>> {
        let raw = Self::stdout(&[
            "issue",
            "list",
            "--label",
            label,
            "--state",
            "open",
            "--json",
            "number,title",
        ])?;
        serde_json::from_str(&raw).ok()
    }

    fn open_issue_summaries(limit: u32) -> Option<Vec<OpenIssueSummary>> {
        let limit_s = limit.to_string();
        let raw = Self::stdout(&[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,labels",
            "--limit",
            &limit_s,
        ])?;
        #[derive(Deserialize)]
        struct Row {
            number: u32,
            title: String,
            labels: Vec<Label>,
        }
        #[derive(Deserialize)]
        struct Label {
            name: String,
        }
        let rows: Vec<Row> = serde_json::from_str(&raw).ok()?;
        Some(
            rows.into_iter()
                .map(|r| OpenIssueSummary {
                    number: r.number,
                    title: r.title,
                    labels: r
                        .labels
                        .into_iter()
                        .map(|l| IssueLabel { name: l.name })
                        .collect(),
                })
                .collect(),
        )
    }

    fn open_issue_housekeeping(limit: u32) -> Option<Vec<OpenIssueHousekeeping>> {
        let limit_s = limit.to_string();
        let raw = Self::stdout(&[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,labels,updatedAt,assignees",
            "--limit",
            &limit_s,
        ])?;
        #[derive(Deserialize)]
        struct Row {
            number: u32,
            title: String,
            labels: Vec<Label>,
            #[serde(rename = "updatedAt")]
            updated_at: String,
            assignees: Vec<Assignee>,
        }
        #[derive(Deserialize)]
        struct Label {
            name: String,
        }
        #[derive(Deserialize)]
        struct Assignee {
            login: String,
        }
        let rows: Vec<Row> = serde_json::from_str(&raw).ok()?;
        Some(
            rows.into_iter()
                .map(|r| OpenIssueHousekeeping {
                    number: r.number,
                    title: r.title,
                    labels: r
                        .labels
                        .into_iter()
                        .map(|l| IssueLabel { name: l.name })
                        .collect(),
                    updated_at: r.updated_at,
                    assignees: r
                        .assignees
                        .into_iter()
                        .map(|a| IssueAssignee { login: a.login })
                        .collect(),
                })
                .collect(),
        )
    }

    fn closed_issue_summaries(limit: u32) -> Option<Vec<ClosedIssueSummary>> {
        let limit_s = limit.to_string();
        let raw = Self::stdout(&[
            "issue",
            "list",
            "--state",
            "closed",
            "--json",
            "number,title,closedAt",
            "--limit",
            &limit_s,
        ])?;
        #[derive(Deserialize)]
        struct Row {
            number: u32,
            title: String,
            #[serde(rename = "closedAt")]
            closed_at: String,
        }
        let rows: Vec<Row> = serde_json::from_str(&raw).ok()?;
        Some(
            rows.into_iter()
                .map(|r| ClosedIssueSummary {
                    number: r.number,
                    title: r.title,
                    closed_at: r.closed_at,
                })
                .collect(),
        )
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

// ── Umbrella ────────────────────────────────────────────────────────────

impl DeveloperPlatform for Gh {}

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
