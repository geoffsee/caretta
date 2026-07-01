// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Developer platform abstraction.
//!
//! A *developer platform* is the system that hosts source code and the
//! collaboration around it — repositories, change proposals (a.k.a. pull
//! requests / merge requests), work items (issues), and review discussions.
//! Caretta talks to one through a small set of capability traits defined
//! here:
//!
//! - [`PullRequestActions`] — single-PR reads, lifecycle, communication,
//!   review-thread plumbing.
//! - [`IssueActions`] — work-item reads and lifecycle.
//! - [`RepoActions`] — repository identity.
//!
//! [`DeveloperPlatform`] is the umbrella supertrait. Today there is exactly
//! one implementor — [`crate::agent::gh::Gh`], which drives the GitHub `gh`
//! CLI. A future Gitea (or other) binding plugs in by implementing the same
//! capability traits.
//!
//! See `docs/internal/developer-platform-mapping.md` for the design notes
//! behind this split, the capability table, and the GitHub ↔ abstract ↔
//! Gitea glossary.

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

    /// PR base branch (ref name), or `None` when the platform cannot
    /// resolve it.
    fn pr_base_ref(pr_num: u32) -> Option<String>;

    /// GitHub `reviewDecision` for a PR — one of `APPROVED`,
    /// `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or an empty string when no
    /// reviews have been submitted. `None` on platform failure.
    fn pr_review_decision(pr_num: u32) -> Option<String>;

    /// Raw `autoMergeRequest` jq projection — `Some("null")` when auto-merge
    /// is disabled, `Some("{...}")` when enabled, `None` on platform
    /// failure. Most callers want
    /// [`pr_is_auto_merge_enabled`](Self::pr_is_auto_merge_enabled).
    fn pr_auto_merge_status_raw(pr_num: u32) -> Option<String>;

    /// True when the platform's squash auto-merge is currently enabled on a
    /// PR. `false` when not enabled, when the platform call fails, or when
    /// the response shape can't be parsed — callers that need to
    /// distinguish those cases should reach for
    /// [`pr_auto_merge_status_raw`](Self::pr_auto_merge_status_raw).
    fn pr_is_auto_merge_enabled(pr_num: u32) -> bool;

    /// Concatenated bodies of every comment on a PR, separated by newlines.
    /// Useful for "does any comment contain marker X?" checks. `None` on
    /// platform failure.
    fn pr_comment_bodies(pr_num: u32) -> Option<String>;

    /// Full PR diff, aborting the process on failure.
    fn pr_diff_or_die(pr_num: u32) -> String;

    /// Raw JSON: `{"comments": [...]}` for a PR. Used by the conflicts
    /// module to parse latest marker context out of comment metadata.
    fn pr_comments(pr_num: u32) -> Option<PrComments>;

    /// Raw JSON of the fields needed for conflict-aware PR views:
    /// `headRefName`, `baseRefName`, `mergeStateStatus`, `title`.
    fn pr_conflict_view(pr_num: u32) -> Option<PrConflictView>;

    /// Raw JSON of the fields needed for [`crate::agent::fix_pr`]'s
    /// diagnostic — `number`, `title`, `headRefName`, `baseRefName`,
    /// `isDraft`, `mergeStateStatus`, `reviewDecision`, `statusCheckRollup`.
    fn pr_diagnostic(pr_num: u32) -> Option<PrDiagnostic>;

    /// Raw JSON of the fields auto-merge refreshes after a retarget:
    /// `mergeStateStatus`, `reviewDecision`, `isDraft`.
    fn pr_status_refresh(pr_num: u32) -> Option<PrStatusRefresh>;

    /// Raw JSON of `reviews` on a PR — list of submitted reviews used to
    /// render prior-review context for the agent.
    fn pr_reviews(pr_num: u32) -> Option<Vec<PrReviewSummaryRecord>>;

    /// Raw JSON of `number,title,headRefName` for the PR backing the
    /// current branch (no PR number argument — the platform infers it from
    /// the working tree).
    fn current_branch_pr_summary() -> Option<CurrentBranchPrSummary>;

    /// URL of the first open PR whose head matches `branch`, or empty
    /// string when none exist. Returns `(call_succeeded, url_or_empty)` so
    /// callers can distinguish "no open PR" from "platform call failed".
    fn find_open_pr_url_for_head(branch: &str) -> (bool, String);

    /// Head ref of the first open PR whose head matches `branch` — the
    /// caller asks "is the upstream branch still alive on the platform?".
    /// `None` when no PR matches or the platform call fails.
    fn find_open_pr_head_for_head(branch: &str) -> Option<String>;

    /// Number of the first open PR whose head matches `branch`. `None`
    /// when no PR matches or the platform call fails.
    fn find_open_pr_number_for_head(branch: &str) -> Option<u32>;

    /// Open PR summaries (`number`, `title`, `headRefName`, `author`) up to
    /// `limit`. Used to populate the tracker sidebar and workflow context.
    fn open_pr_summaries(limit: u32) -> Option<Vec<cli_common::PrSummary>>;

    /// Recently merged PR summaries (`number`, `title`, `mergedAt`) up to
    /// `limit`. Used by the retrospective workflow.
    fn merged_pr_summaries(limit: u32) -> Option<Vec<MergedPrSummary>>;

    /// Open PR rows shaped for auto-merge lineage analysis: `number`,
    /// `headRefName`, `baseRefName`, `isDraft`, `mergeStateStatus`,
    /// `reviewDecision`. Aborts with `context` on failure since auto-merge
    /// can't proceed without the roster.
    fn open_merge_candidate_prs_or_die(context: &str) -> Vec<OpenMergeCandidatePr>;

    /// Open PR rows shaped for auto-merge lineage analysis, returned as
    /// raw JSON or `None` when the platform call fails (best-effort
    /// variant used by the lineage refresh pass).
    fn try_open_merge_candidate_prs() -> Option<Vec<OpenMergeCandidatePr>>;

    /// Create a PR with the given head, base, title, and body. True on
    /// success. Caller is responsible for checking no PR already exists
    /// for `head`.
    fn create_pr(head: &str, base: &str, title: &str, body: &str) -> bool;

    /// Merge the latest base into the head branch. Inherits stdio.
    fn update_pr_branch(pr_num: u32) -> bool;

    /// Like [`update_pr_branch`](Self::update_pr_branch), but captures the
    /// combined output so the caller can log the failure reason.
    fn update_pr_branch_capture(pr_num: u32) -> (bool, String);

    /// Retarget a PR at a new base branch.
    fn edit_pr_base(pr_num: u32, new_base: &str) -> bool;

    /// Immediate squash merge. Captures combined output for the failure
    /// path.
    fn merge_pr_squash(pr_num: u32) -> (bool, String);

    /// Turn on auto-merge with the squash strategy (merges once branch
    /// protection allows it).
    fn enable_pr_auto_merge_squash(pr_num: u32) -> (bool, String);

    /// Post a PR comment.
    fn comment_on_pr(pr_num: u32, body: &str) -> (bool, String);

    /// Submit a PR review with extra env (typically a bot token, since
    /// GitHub rejects self-approval). `action` is the argv flag —
    /// `"--approve"`, `"--comment"`, or `"--request-changes"`.
    fn submit_pr_review_with_env(
        pr_num: u32,
        action: &str,
        body: &str,
        env: &[(String, String)],
    ) -> bool;

    /// Mark one review thread as resolved. Returns the raw response body
    /// so callers can confirm the resolution; `None` on platform failure.
    fn mark_review_thread_resolved(thread_id: &str) -> Option<String>;

    /// Raw JSON of review threads for `pr_num`. Resolves the working
    /// directory's repository internally. `None` when the repo can't be
    /// identified or the platform call fails.
    fn fetch_pr_review_threads(pr_num: u32) -> Option<Vec<PrReviewThread>>;

    /// Raw JSON of every open PR's review threads in one round-trip.
    /// Resolves the working directory's repository internally. Used by
    /// tracker refresh so per-PR `(N)` badges stay in sync without N
    /// round-trips.
    fn fetch_open_pr_review_threads_batched() -> Option<Vec<OpenPrReviewThreads>>;
}

// ── Issues ──────────────────────────────────────────────────────────────

/// Work-item (issue) reads and lifecycle. Field projections are baked into
/// each method so call sites don't have to know the underlying field names.
pub trait IssueActions {
    /// Issue body / description, aborting with `context` on failure.
    fn issue_body_or_die(issue_num: u32, context: &str) -> String;

    /// Issue title, aborting with `context` on failure.
    fn issue_title_or_die(issue_num: u32, context: &str) -> String;

    /// Rewrite the issue body.
    fn edit_issue_body(issue_num: u32, body: &str) -> bool;

    /// Close an issue.
    fn close_issue(issue_num: u32) -> bool;

    /// Open issue summaries (`number`, `title`) for issues carrying
    /// `label`.
    fn open_issue_summaries_with_label(label: &str) -> Option<Vec<IssueSummary>>;

    /// Open issue summaries (`number`, `title`, `labels`) up to `limit`.
    /// Used by workflow context gatherers that need a quick roster.
    fn open_issue_summaries(limit: u32) -> Option<Vec<OpenIssueSummary>>;

    /// Open issue rows with the extra fields housekeeping needs
    /// (`number`, `title`, `labels`, `updatedAt`, `assignees`) up to
    /// `limit`.
    fn open_issue_housekeeping(limit: u32) -> Option<Vec<OpenIssueHousekeeping>>;

    /// Recently closed issue summaries (`number`, `title`, `closedAt`)
    /// up to `limit`. Used by the retrospective workflow.
    fn closed_issue_summaries(limit: u32) -> Option<Vec<ClosedIssueSummary>>;

    /// Open issue numbers whose titles match the search expression
    /// `search` (e.g. `"retro in:title"`). Returned as one
    /// newline-delimited number per line (raw platform output).
    fn open_issue_numbers_matching_title(search: &str) -> Option<String>;

    /// Body of the most recent open issue carrying `label`, formatted as
    /// `# <title>\n\n<body>`. Empty string when none found or the platform
    /// call fails.
    fn first_open_issue_body_for_label(label: &str) -> String;
}

// ── Repository ──────────────────────────────────────────────────────────

/// Repository identity. Just enough to identify the repo today; extend as
/// new call sites need it.
pub trait RepoActions {
    /// `owner/repo` slug for the working directory's repository, or `None`
    /// when the platform cannot resolve it.
    fn repo_name_with_owner() -> Option<String>;
}

// ── Umbrella ────────────────────────────────────────────────────────────

/// Umbrella supertrait that bundles every capability a developer platform
/// binding must provide. Today this is purely a marker — caretta does not
/// yet hold platform bindings behind a trait object, since the capability
/// methods are associated functions (no `&self`). Once the migration to
/// typed return values is done and the methods take a receiver, this trait
/// becomes the dyn-compatible surface used by a future factory returning
/// `Box<dyn DeveloperPlatform>`.
pub trait DeveloperPlatform: PullRequestActions + IssueActions + RepoActions {}
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrationReadiness {
    Clean,
    Behind,
    Dirty,
    Blocked,
    Unstable,
    HasHooks,
    Draft,
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalGate {
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
    Unknown(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCheckStatus {
    pub typename: Option<String>,
    pub name: Option<String>,
    pub context: Option<String>,
    pub state: Option<String>,
    pub conclusion: Option<String>,
    pub status: Option<String>,
    pub target_url: Option<String>,
    pub details_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrCommentRecord {
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrComments {
    pub comments: Vec<PrCommentRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrConflictView {
    pub head_ref: String,
    pub base_ref: String,
    pub integration_readiness: Option<IntegrationReadiness>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrDiagnostic {
    pub number: u32,
    pub title: String,
    pub head_ref: String,
    pub base_ref: String,
    pub is_draft: bool,
    pub integration_readiness: Option<IntegrationReadiness>,
    pub approval_gate: Option<ApprovalGate>,
    pub status_check_rollup: Vec<PlatformCheckStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrStatusRefresh {
    pub integration_readiness: Option<IntegrationReadiness>,
    pub approval_gate: Option<ApprovalGate>,
    pub is_draft: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReviewSummaryRecord {
    pub author_login: String,
    pub state: String,
    pub submitted_at: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentBranchPrSummary {
    pub number: u32,
    pub title: String,
    pub head_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedPrSummary {
    pub number: u32,
    pub title: String,
    pub merged_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenMergeCandidatePr {
    pub number: u32,
    pub head_ref: String,
    pub base_ref: String,
    pub is_draft: bool,
    pub integration_readiness: Option<IntegrationReadiness>,
    pub approval_gate: Option<ApprovalGate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReviewThreadComment {
    pub author_login: String,
    pub author_type: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub original_line: Option<u32>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReviewThread {
    pub id: String,
    pub is_resolved: bool,
    pub comments: Vec<PrReviewThreadComment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPrReviewThreads {
    pub pr_number: u32,
    pub review_threads: Vec<PrReviewThread>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueSummary {
    pub number: u32,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueLabel {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueAssignee {
    pub login: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenIssueSummary {
    pub number: u32,
    pub title: String,
    pub labels: Vec<IssueLabel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenIssueHousekeeping {
    pub number: u32,
    pub title: String,
    pub labels: Vec<IssueLabel>,
    pub updated_at: String,
    pub assignees: Vec<IssueAssignee>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedIssueSummary {
    pub number: u32,
    pub title: String,
    pub closed_at: String,
}

pub fn map_integration_readiness(raw: &str) -> IntegrationReadiness {
    match raw.trim().to_ascii_uppercase().as_str() {
        "CLEAN" => IntegrationReadiness::Clean,
        "BEHIND" => IntegrationReadiness::Behind,
        "DIRTY" => IntegrationReadiness::Dirty,
        "BLOCKED" => IntegrationReadiness::Blocked,
        "UNSTABLE" => IntegrationReadiness::Unstable,
        "HAS_HOOKS" => IntegrationReadiness::HasHooks,
        "DRAFT" => IntegrationReadiness::Draft,
        other => IntegrationReadiness::Unknown(other.to_string()),
    }
}

pub fn map_approval_gate(raw: &str) -> ApprovalGate {
    match raw.trim().to_ascii_uppercase().as_str() {
        "APPROVED" => ApprovalGate::Approved,
        "CHANGES_REQUESTED" => ApprovalGate::ChangesRequested,
        "REVIEW_REQUIRED" => ApprovalGate::ReviewRequired,
        "" => ApprovalGate::None,
        other => ApprovalGate::Unknown(other.to_string()),
    }
}
