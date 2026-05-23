//! `caretta fix-pr <N>` entry point.
//!
//! Diagnoses why a PR is stuck and dispatches the appropriate remediation:
//!
//! - `mergeStateStatus = DIRTY` → recommend `caretta fix-conflicts <N>` (we
//!   don't auto-invoke it because that flow expects a caretta branch-sync
//!   marker comment that signals which base to merge in).
//! - `mergeStateStatus = BEHIND` → run `gh pr update-branch` so the head
//!   branch picks up the latest base.
//! - One or more checks in a failed state (`FAILURE`, `ERROR`, `CANCELLED`,
//!   `TIMED_OUT`, `STARTUP_FAILURE`, or a `StatusContext` reporting `FAILURE`/
//!   `ERROR`) → run the agent in a throwaway worktree on the PR head with a
//!   prompt that names the failing checks and points at their `details_url`/
//!   `target_url`; commit and push.
//! - Unresolved bot-authored review threads → existing `run_pr_review_fix`
//!   flow.
//!
//! Multiple remediations can run in a single invocation (e.g. update-branch
//! then fix failing checks). The DIRTY case short-circuits because nothing
//! else can safely run until conflicts are resolved.

use crate::agent::cmd::{cmd_run, cmd_stdout, log};
use crate::agent::gh::Gh;
use crate::agent::issue::preflight;
use crate::agent::platform::{
    ApprovalGate, IntegrationReadiness, PrDiagnostic, PullRequestActions,
};
use crate::agent::process::emit_event;
use crate::agent::review::{
    commit_and_push_worktree_changes, run_pr_review_fix, setup_pr_worktree,
};
use crate::agent::run::run_agent_with_env_in_dir;
use crate::agent::tracker::{
    DEFAULT_REVIEW_BOT_LOGIN, build_pr_failing_checks_fix_prompt, fetch_unresolved_review_threads,
    list_open_prs, pr_diff,
};
use crate::agent::types::{AgentEvent, Config};
use serde::Deserialize;

/// Full snapshot of the PR state surfaced by `caretta fix-pr`.
///
/// Built from `gh pr view --json …` plus a separate query for unresolved
/// review threads and (when `mergeStateStatus` is inconclusive) a git
/// ancestry check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFixDiagnostic {
    pub number: u32,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub is_draft: bool,
    /// One of `CLEAN`, `BLOCKED`, `BEHIND`, `DIRTY`, `UNSTABLE`, `HAS_HOOKS`,
    /// `DRAFT`, `UNKNOWN`, or absent if `gh` didn't return it.
    pub merge_state: Option<String>,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or empty.
    pub review_decision: Option<String>,
    pub failing_checks: Vec<CheckStatus>,
    pub pending_checks: Vec<CheckStatus>,
    pub unresolved_bot_thread_count: usize,
    /// True when the head branch should be updated with the base. Set when
    /// `mergeStateStatus = BEHIND`, or — as a fallback when `gh` reports
    /// `UNKNOWN`/empty — when `git rev-list origin/{head}..origin/{base}` is
    /// non-empty. `parse_pr_view_json` only knows about the gh-reported case;
    /// `diagnose_pr` overlays the git-ancestry fallback.
    pub head_behind_base: bool,
}

/// One entry from GitHub's `statusCheckRollup`. Holds the union of the fields
/// we need across the two underlying types (`StatusContext` and `CheckRun`).
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckStatus {
    #[serde(default, rename = "__typename")]
    pub typename: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    /// `StatusContext.state`: SUCCESS, FAILURE, PENDING, EXPECTED, ERROR.
    #[serde(default)]
    pub state: Option<String>,
    /// `CheckRun.conclusion`: SUCCESS, FAILURE, NEUTRAL, CANCELLED,
    /// TIMED_OUT, ACTION_REQUIRED, STALE, STARTUP_FAILURE, SKIPPED, or null
    /// when still running.
    #[serde(default)]
    pub conclusion: Option<String>,
    /// `CheckRun.status`: QUEUED, IN_PROGRESS, COMPLETED.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "targetUrl")]
    pub target_url: Option<String>,
    #[serde(default, rename = "detailsUrl")]
    pub details_url: Option<String>,
}

impl CheckStatus {
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.context.as_deref())
            .unwrap_or("(unnamed)")
    }

    pub fn link(&self) -> Option<&str> {
        self.details_url
            .as_deref()
            .or(self.target_url.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// True when the check has reached a terminal failing state.
    pub fn is_failing(&self) -> bool {
        if let Some(c) = self.conclusion.as_deref().filter(|s| !s.is_empty()) {
            return matches!(
                c.to_ascii_uppercase().as_str(),
                "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
            );
        }
        if let Some(s) = self.state.as_deref().filter(|s| !s.is_empty()) {
            return matches!(s.to_ascii_uppercase().as_str(), "FAILURE" | "ERROR");
        }
        false
    }

    /// True when the check is still in progress / hasn't reported a result.
    pub fn is_pending(&self) -> bool {
        if self.is_failing() {
            return false;
        }
        if let Some(c) = self.conclusion.as_deref().filter(|s| !s.is_empty()) {
            // A terminal non-failing conclusion (SUCCESS/NEUTRAL/SKIPPED/STALE) is not pending.
            let upper = c.to_ascii_uppercase();
            return matches!(upper.as_str(), "" | "NEUTRAL")
                && !matches!(upper.as_str(), "SUCCESS" | "SKIPPED" | "STALE");
        }
        // No conclusion → check the run-status / context-state for pending markers.
        if let Some(status) = self.status.as_deref().filter(|s| !s.is_empty()) {
            return matches!(
                status.to_ascii_uppercase().as_str(),
                "QUEUED" | "IN_PROGRESS" | "WAITING" | "PENDING" | "REQUESTED"
            );
        }
        if let Some(s) = self.state.as_deref().filter(|s| !s.is_empty()) {
            return matches!(s.to_ascii_uppercase().as_str(), "PENDING" | "EXPECTED");
        }
        false
    }
}

pub fn build_pr_fix_diagnostic(
    view: PrDiagnostic,
    unresolved_bot_thread_count: usize,
) -> PrFixDiagnostic {
    let rollup = view
        .status_check_rollup
        .into_iter()
        .map(|c| CheckStatus {
            typename: c.typename,
            name: c.name,
            context: c.context,
            state: c.state,
            conclusion: c.conclusion,
            status: c.status,
            target_url: c.target_url,
            details_url: c.details_url,
        })
        .collect::<Vec<_>>();
    let (failing_checks, rest): (Vec<_>, Vec<_>) = rollup.into_iter().partition(|c| c.is_failing());
    let pending_checks = rest.into_iter().filter(|c| c.is_pending()).collect();
    let head_behind_base = matches!(
        view.integration_readiness,
        Some(IntegrationReadiness::Behind)
    );
    PrFixDiagnostic {
        number: view.number,
        title: view.title,
        head_branch: view.head_ref,
        base_branch: view.base_ref,
        is_draft: view.is_draft,
        merge_state: view
            .integration_readiness
            .map(|v| format!("{v:?}").to_ascii_uppercase()),
        review_decision: view.approval_gate.map(|v| match v {
            ApprovalGate::Approved => "APPROVED".to_string(),
            ApprovalGate::ChangesRequested => "CHANGES_REQUESTED".to_string(),
            ApprovalGate::ReviewRequired => "REVIEW_REQUIRED".to_string(),
            ApprovalGate::None => String::new(),
            ApprovalGate::Unknown(s) => s,
        }),
        failing_checks,
        pending_checks,
        unresolved_bot_thread_count,
        head_behind_base,
    }
}

/// True when `origin/{base}` has commits not on `origin/{head}` — i.e. the
/// head branch would benefit from `gh pr update-branch` even if GitHub
/// hasn't reported `BEHIND`. Best-effort: returns `false` if either branch is
/// empty, if the fetch fails, or if `rev-list` can't run.
///
/// Run BEFORE the throwaway worktree setup so the ancestry check uses the
/// user's main checkout's remotes. The `git fetch` here is the same one the
/// dispatcher would issue anyway; it's still useful to keep the function
/// idempotent (running it twice is cheap).
fn head_is_behind_base_via_git(head_branch: &str, base_branch: &str) -> bool {
    if head_branch.is_empty() || base_branch.is_empty() {
        return false;
    }
    if !cmd_run("git", &["fetch", "origin", base_branch, head_branch]) {
        return false;
    }
    let range = format!("origin/{head_branch}..origin/{base_branch}");
    let out = cmd_stdout("git", &["rev-list", "--count", &range]).unwrap_or_default();
    out.trim().parse::<u32>().unwrap_or(0) > 0
}

fn diagnose_pr(pr_num: u32) -> Option<PrFixDiagnostic> {
    let view = Gh::pr_diagnostic(pr_num)?;
    let thread_count = fetch_unresolved_review_threads(pr_num, DEFAULT_REVIEW_BOT_LOGIN).len();
    let mut diag = build_pr_fix_diagnostic(view, thread_count);

    // GitHub computes mergeStateStatus asynchronously; a freshly-opened PR
    // (or one whose mergeability check is stuck) can report UNKNOWN/empty
    // even when its head branch is genuinely behind base. Fall back to git
    // ancestry in those cases so fix-pr still picks up the BEHIND condition.
    if !diag.head_behind_base {
        let inconclusive = diag
            .merge_state
            .as_deref()
            .is_none_or(|s| s.is_empty() || s.eq_ignore_ascii_case("UNKNOWN"));
        if inconclusive && head_is_behind_base_via_git(&diag.head_branch, &diag.base_branch) {
            log(&format!(
                "PR #{pr_num}: mergeStateStatus={:?} is inconclusive, but git ancestry shows origin/{} has commits not on origin/{} — treating as BEHIND.",
                diag.merge_state.as_deref().unwrap_or(""),
                diag.base_branch,
                diag.head_branch,
            ));
            diag.head_behind_base = true;
        }
    }
    Some(diag)
}

fn log_diagnostic(diag: &PrFixDiagnostic) {
    log(&format!(
        "PR #{n} state: mergeStateStatus={merge:?} reviewDecision={rev:?} draft={draft} head_behind_base={hb} failing_checks={fc} pending_checks={pc} unresolved_bot_threads={th}",
        n = diag.number,
        merge = diag.merge_state.as_deref().unwrap_or("UNKNOWN"),
        rev = diag.review_decision.as_deref().unwrap_or(""),
        draft = diag.is_draft,
        hb = diag.head_behind_base,
        fc = diag.failing_checks.len(),
        pc = diag.pending_checks.len(),
        th = diag.unresolved_bot_thread_count,
    ));
}

/// What `run_fix_pr` should do based on a diagnostic. Pure for testability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixAction {
    /// PR is draft — nothing to do.
    SkipDraft,
    /// Merge state is DIRTY (conflicts) — caller should bail and recommend
    /// `caretta fix-conflicts <N>`.
    Conflicts,
    /// One or more handlers should run, in the order listed.
    Run(Vec<FixHandler>),
    /// No actionable issues. Pending check count surfaced so the operator can
    /// distinguish "all green" from "still running."
    Nothing { pending_checks: usize },
}

/// Atomic remediation steps. The dispatcher runs them in `Run`'s order, so
/// list update-branch before failing-checks (a stale base is a likely cause of
/// CI failures), and failing-checks before review-threads (review threads
/// often comment on code that hasn't been touched since the last CI run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixHandler {
    UpdateBranchFromBase,
    FixFailingChecks,
    FixReviewComments,
}

/// Pure decision logic for [`run_fix_pr`]. Given a diagnostic, returns what
/// the dispatcher should do.
pub fn plan_actions(diag: &PrFixDiagnostic) -> FixAction {
    if diag.is_draft {
        return FixAction::SkipDraft;
    }
    if diag
        .merge_state
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("DIRTY"))
    {
        return FixAction::Conflicts;
    }
    let mut handlers = Vec::new();
    if diag.head_behind_base {
        handlers.push(FixHandler::UpdateBranchFromBase);
    }
    if !diag.failing_checks.is_empty() {
        handlers.push(FixHandler::FixFailingChecks);
    }
    if diag.unresolved_bot_thread_count > 0 {
        handlers.push(FixHandler::FixReviewComments);
    }
    if handlers.is_empty() {
        return FixAction::Nothing {
            pending_checks: diag.pending_checks.len(),
        };
    }
    FixAction::Run(handlers)
}

/// Entry point for `caretta fix-pr <N>`.
pub fn run_fix_pr(cfg: &Config, pr_num: u32) {
    preflight(cfg);
    log(&format!("Diagnosing PR #{pr_num}..."));

    if cfg.dry_run {
        log(&format!(
            "[dry-run] Would diagnose and dispatch fixes for PR #{pr_num}"
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let Some(diag) = diagnose_pr(pr_num) else {
        log(&format!(
            "Could not fetch PR #{pr_num} state from GitHub; aborting."
        ));
        emit_event(AgentEvent::Done);
        return;
    };
    log_diagnostic(&diag);

    match plan_actions(&diag) {
        FixAction::SkipDraft => {
            log(&format!(
                "PR #{pr_num} is a draft; not running any fixes. Mark it ready for review first."
            ));
            emit_event(AgentEvent::Done);
        }
        FixAction::Conflicts => {
            log(&format!(
                "PR #{pr_num} has merge conflicts (mergeStateStatus=DIRTY). Run `caretta fix-conflicts {pr_num}` to resolve them, then re-run fix-pr."
            ));
            emit_event(AgentEvent::Done);
        }
        FixAction::Nothing { pending_checks } => {
            if pending_checks > 0 {
                log(&format!(
                    "PR #{pr_num} has no actionable issues — {pending_checks} check(s) still pending. Re-run fix-pr after they complete."
                ));
            } else {
                log(&format!(
                    "PR #{pr_num} has no actionable issues — nothing to fix."
                ));
            }
            emit_event(AgentEvent::Done);
        }
        FixAction::Run(handlers) => {
            for handler in handlers {
                match handler {
                    FixHandler::UpdateBranchFromBase => {
                        run_update_branch_from_base(pr_num);
                    }
                    FixHandler::FixFailingChecks => {
                        run_pr_failing_checks_fix(cfg, pr_num, &diag);
                    }
                    FixHandler::FixReviewComments => {
                        run_pr_review_fix(cfg, pr_num);
                    }
                }
            }
            emit_event(AgentEvent::Done);
        }
    }
}

fn run_update_branch_from_base(pr_num: u32) {
    log(&format!(
        "PR #{pr_num} is BEHIND base; running `gh pr update-branch`..."
    ));
    if !Gh::update_pr_branch(pr_num) {
        log(&format!(
            "`gh pr update-branch` failed for PR #{pr_num}; downstream fixes may not stick until the head branch is current with its base."
        ));
        return;
    }
    log(&format!(
        "Updated PR #{pr_num} head branch with base; CI should re-run automatically."
    ));
}

/// Worktree + agent + commit + push flow for "fix the failing CI checks on
/// this PR." Reuses [`setup_pr_worktree`] / [`commit_and_push_worktree_changes`]
/// so failures and stop-requested handling stay consistent with the Fix
/// Comments flow.
pub(crate) fn run_pr_failing_checks_fix(cfg: &Config, pr_num: u32, diag: &PrFixDiagnostic) {
    log(&format!(
        "Starting Fix Failing Checks run for PR #{pr_num} ({} check(s) failing)...",
        diag.failing_checks.len()
    ));

    let branch = if !diag.head_branch.is_empty() {
        diag.head_branch.clone()
    } else {
        log(&format!(
            "PR #{pr_num} headRefName missing from diagnostic; cannot set up worktree."
        ));
        return;
    };
    let title = if !diag.title.is_empty() {
        diag.title.clone()
    } else {
        list_open_prs()
            .into_iter()
            .find(|pr| pr.number == pr_num)
            .map(|pr| pr.title)
            .unwrap_or_else(|| format!("PR #{pr_num}"))
    };

    let Some((_guard, worktree_path)) =
        setup_pr_worktree(cfg, pr_num, &branch, "Fix Failing Checks")
    else {
        return;
    };

    let diff = pr_diff(pr_num);
    let check_pairs: Vec<(&str, Option<&str>)> = diag
        .failing_checks
        .iter()
        .map(|c| (c.display_name(), c.link()))
        .collect();
    let prompt = build_pr_failing_checks_fix_prompt(
        &cfg.project_name,
        pr_num,
        &title,
        &branch,
        &diff,
        &check_pairs,
    );

    if !run_agent_with_env_in_dir(cfg, &prompt, &[], &worktree_path) {
        log(&format!(
            "Fix Failing Checks agent failed for PR #{pr_num}."
        ));
        return;
    }
    if crate::agent::process::stop_requested() {
        log("Stop requested. Fix Failing Checks run cancelled.");
        return;
    }

    let worktree_str = worktree_path.to_string_lossy().to_string();
    let status =
        cmd_stdout("git", &["-C", &worktree_str, "status", "--porcelain"]).unwrap_or_default();
    if status.trim().is_empty() {
        log(&format!(
            "Fix Failing Checks made no file changes for PR #{pr_num}; CI status unchanged."
        ));
        return;
    }

    let message = format!(
        "fix failing CI checks on PR #{pr_num}\n\n{}",
        cfg.agent.co_author()
    );
    if !commit_and_push_worktree_changes(
        cfg,
        pr_num,
        &branch,
        &worktree_path,
        &message,
        "Fix Failing Checks",
    ) {
        return;
    }
    log(&format!(
        "Fix Failing Checks complete for PR #{pr_num}: pushed changes; CI will re-run."
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag_template() -> PrFixDiagnostic {
        PrFixDiagnostic {
            number: 141,
            title: "test".into(),
            head_branch: "agent/issue-1".into(),
            base_branch: "main".into(),
            is_draft: false,
            merge_state: Some("CLEAN".into()),
            review_decision: Some("APPROVED".into()),
            failing_checks: Vec::new(),
            pending_checks: Vec::new(),
            unresolved_bot_thread_count: 0,
            head_behind_base: false,
        }
    }

    #[test]
    fn check_run_failure_conclusion_is_failing() {
        let c = CheckStatus {
            typename: Some("CheckRun".into()),
            name: Some("Test".into()),
            conclusion: Some("FAILURE".into()),
            status: Some("COMPLETED".into()),
            ..CheckStatus::default()
        };
        assert!(c.is_failing());
        assert!(!c.is_pending());
    }

    #[test]
    fn status_context_failure_state_is_failing() {
        let c = CheckStatus {
            typename: Some("StatusContext".into()),
            context: Some("Test".into()),
            state: Some("FAILURE".into()),
            ..CheckStatus::default()
        };
        assert!(c.is_failing());
        assert!(!c.is_pending());
    }

    #[test]
    fn check_run_in_progress_is_pending() {
        let c = CheckStatus {
            typename: Some("CheckRun".into()),
            name: Some("Test".into()),
            status: Some("IN_PROGRESS".into()),
            ..CheckStatus::default()
        };
        assert!(!c.is_failing());
        assert!(c.is_pending());
    }

    #[test]
    fn status_context_pending_state_is_pending() {
        let c = CheckStatus {
            typename: Some("StatusContext".into()),
            context: Some("Test".into()),
            state: Some("PENDING".into()),
            ..CheckStatus::default()
        };
        assert!(!c.is_failing());
        assert!(c.is_pending());
    }

    #[test]
    fn check_run_success_is_neither() {
        let c = CheckStatus {
            typename: Some("CheckRun".into()),
            name: Some("Test".into()),
            conclusion: Some("SUCCESS".into()),
            status: Some("COMPLETED".into()),
            ..CheckStatus::default()
        };
        assert!(!c.is_failing());
        assert!(!c.is_pending());
    }

    #[test]
    fn plan_skips_draft_even_with_problems() {
        let mut d = diag_template();
        d.is_draft = true;
        d.merge_state = Some("BEHIND".into());
        d.unresolved_bot_thread_count = 3;
        assert_eq!(plan_actions(&d), FixAction::SkipDraft);
    }

    #[test]
    fn plan_dirty_short_circuits() {
        let mut d = diag_template();
        d.merge_state = Some("DIRTY".into());
        d.unresolved_bot_thread_count = 3;
        d.failing_checks.push(CheckStatus {
            conclusion: Some("FAILURE".into()),
            ..CheckStatus::default()
        });
        assert_eq!(plan_actions(&d), FixAction::Conflicts);
    }

    #[test]
    fn plan_clean_with_nothing_returns_nothing() {
        let d = diag_template();
        assert_eq!(plan_actions(&d), FixAction::Nothing { pending_checks: 0 });
    }

    #[test]
    fn plan_clean_with_pending_only_reports_pending() {
        let mut d = diag_template();
        d.pending_checks.push(CheckStatus {
            state: Some("PENDING".into()),
            ..CheckStatus::default()
        });
        assert_eq!(plan_actions(&d), FixAction::Nothing { pending_checks: 1 });
    }

    #[test]
    fn plan_behind_alone_runs_update_branch() {
        // Reproduces PR #141: BEHIND, approved, no threads, no failing checks
        // (the test was just (re)queued by autopilot and is still PENDING).
        let mut d = diag_template();
        d.merge_state = Some("BEHIND".into());
        d.head_behind_base = true;
        d.pending_checks.push(CheckStatus {
            state: Some("PENDING".into()),
            ..CheckStatus::default()
        });
        assert_eq!(
            plan_actions(&d),
            FixAction::Run(vec![FixHandler::UpdateBranchFromBase])
        );
    }

    #[test]
    fn plan_ancestry_fallback_runs_update_branch_even_when_gh_unknown() {
        // GitHub hasn't computed mergeStateStatus yet (UNKNOWN), but the git
        // ancestry overlay determined the head is behind base. The dispatcher
        // should still update-branch.
        let mut d = diag_template();
        d.merge_state = Some("UNKNOWN".into());
        d.head_behind_base = true;
        assert_eq!(
            plan_actions(&d),
            FixAction::Run(vec![FixHandler::UpdateBranchFromBase])
        );
    }

    #[test]
    fn build_pr_fix_diagnostic_sets_head_behind_base_for_behind() {
        let diag = build_pr_fix_diagnostic(
            PrDiagnostic {
                number: 0,
                title: String::new(),
                head_ref: String::new(),
                base_ref: String::new(),
                is_draft: false,
                integration_readiness: Some(IntegrationReadiness::Behind),
                approval_gate: None,
                status_check_rollup: Vec::new(),
            },
            0,
        );
        assert!(diag.head_behind_base);
    }

    #[test]
    fn build_pr_fix_diagnostic_leaves_head_behind_base_false_for_unknown() {
        // Without git ancestry data, the pure parser can't decide — that
        // overlay is applied by `diagnose_pr`. Pure parser keeps `false` here.
        let diag = build_pr_fix_diagnostic(
            PrDiagnostic {
                number: 0,
                title: String::new(),
                head_ref: String::new(),
                base_ref: String::new(),
                is_draft: false,
                integration_readiness: Some(IntegrationReadiness::Unknown("UNKNOWN".to_string())),
                approval_gate: None,
                status_check_rollup: Vec::new(),
            },
            0,
        );
        assert!(!diag.head_behind_base);
    }

    #[test]
    fn plan_behind_plus_failing_plus_threads_runs_all_three_in_order() {
        let mut d = diag_template();
        d.merge_state = Some("BEHIND".into());
        d.head_behind_base = true;
        d.failing_checks.push(CheckStatus {
            conclusion: Some("FAILURE".into()),
            ..CheckStatus::default()
        });
        d.unresolved_bot_thread_count = 2;
        assert_eq!(
            plan_actions(&d),
            FixAction::Run(vec![
                FixHandler::UpdateBranchFromBase,
                FixHandler::FixFailingChecks,
                FixHandler::FixReviewComments,
            ])
        );
    }

    #[test]
    fn build_pr_fix_diagnostic_extracts_full_state() {
        let diag = build_pr_fix_diagnostic(
            PrDiagnostic {
                number: 141,
                title: "implement #135".to_string(),
                head_ref: "agent/issue-135".to_string(),
                base_ref: "main".to_string(),
                is_draft: false,
                integration_readiness: Some(IntegrationReadiness::Behind),
                approval_gate: Some(ApprovalGate::Approved),
                status_check_rollup: vec![crate::agent::platform::PlatformCheckStatus {
                    typename: Some("StatusContext".to_string()),
                    name: None,
                    context: Some("Test".to_string()),
                    state: Some("PENDING".to_string()),
                    conclusion: None,
                    status: None,
                    target_url: Some(String::new()),
                    details_url: None,
                }],
            },
            0,
        );
        assert_eq!(diag.number, 141);
        assert_eq!(diag.head_branch, "agent/issue-135");
        assert_eq!(diag.base_branch, "main");
        assert!(!diag.is_draft);
        assert_eq!(diag.merge_state.as_deref(), Some("BEHIND"));
        assert_eq!(diag.review_decision.as_deref(), Some("APPROVED"));
        assert_eq!(diag.failing_checks.len(), 0);
        assert_eq!(diag.pending_checks.len(), 1);
        assert_eq!(diag.pending_checks[0].display_name(), "Test");
        assert_eq!(diag.unresolved_bot_thread_count, 0);
    }

    #[test]
    fn build_pr_fix_diagnostic_with_failing_check_run() {
        let diag = build_pr_fix_diagnostic(
            PrDiagnostic {
                number: 200,
                title: "x".to_string(),
                head_ref: "agent/issue-200".to_string(),
                base_ref: "main".to_string(),
                is_draft: false,
                integration_readiness: Some(IntegrationReadiness::Clean),
                approval_gate: Some(ApprovalGate::None),
                status_check_rollup: vec![
                    crate::agent::platform::PlatformCheckStatus {
                        typename: Some("CheckRun".to_string()),
                        name: Some("Test".to_string()),
                        context: None,
                        state: None,
                        conclusion: Some("FAILURE".to_string()),
                        status: Some("COMPLETED".to_string()),
                        target_url: None,
                        details_url: Some("https://example/run/123".to_string()),
                    },
                    crate::agent::platform::PlatformCheckStatus {
                        typename: Some("CheckRun".to_string()),
                        name: Some("Lint".to_string()),
                        context: None,
                        state: None,
                        conclusion: Some("SUCCESS".to_string()),
                        status: Some("COMPLETED".to_string()),
                        target_url: None,
                        details_url: None,
                    },
                ],
            },
            0,
        );
        assert_eq!(diag.failing_checks.len(), 1);
        assert_eq!(diag.failing_checks[0].display_name(), "Test");
        assert_eq!(
            diag.failing_checks[0].link(),
            Some("https://example/run/123")
        );
        assert_eq!(diag.pending_checks.len(), 0);
    }

    #[test]
    fn build_pr_fix_diagnostic_tolerates_missing_optional_fields() {
        let diag = build_pr_fix_diagnostic(
            PrDiagnostic {
                number: 0,
                title: String::new(),
                head_ref: String::new(),
                base_ref: String::new(),
                is_draft: false,
                integration_readiness: None,
                approval_gate: None,
                status_check_rollup: Vec::new(),
            },
            5,
        );
        assert_eq!(diag.number, 0);
        assert_eq!(diag.head_branch, "");
        assert!(!diag.is_draft);
        assert_eq!(diag.failing_checks.len(), 0);
        assert_eq!(diag.unresolved_bot_thread_count, 5);
    }
}
