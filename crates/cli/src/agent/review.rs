use crate::agent::bot::resolve_bot_token;
use crate::agent::cmd::{cmd_capture, cmd_run, cmd_stdout, log};
use crate::agent::gh::{Gh, PullRequestActions};
use crate::agent::issue::preflight;
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::{emit_event, stop_requested};
use crate::agent::run::{run_agent_with_env, run_agent_with_env_in_dir};
use crate::agent::tracker::{
    DEFAULT_REVIEW_BOT_LOGIN, ReviewThread, build_code_review_prompt, build_commit_hook_fix_prompt,
    build_pr_review_fix_prompt, build_pr_review_verification_prompt,
    build_review_followup_code_review_prompt, build_security_review_prompt,
    fetch_all_unresolved_review_threads, fetch_pr_reviews, fetch_unresolved_review_threads,
    list_open_prs, parse_verification_verdict, pr_body, pr_diff, pr_head_branch,
    pr_review_decision, render_prior_pr_review_context, resolve_review_thread,
};
use crate::agent::types::{AgentEvent, Config, MAX_COMMIT_ATTEMPTS};
use std::path::{Path, PathBuf};

/// Where to put the main checkout's HEAD back after a fix-pr run that
/// temporarily detached it to free a branch for the throwaway worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    /// Re-attach the main checkout's HEAD to this short branch name.
    Branch(String),
    /// Re-attach as detached HEAD at this commit SHA.
    Detached(String),
}

/// Records the main checkout HEAD that [`WorktreeGuard`] should restore on
/// drop. Only populated when we had to detach the main checkout to free a
/// branch that the throwaway worktree wants to force-reset.
#[derive(Debug, Clone)]
pub struct MainHeadRestore {
    pub root: PathBuf,
    pub target: RestoreTarget,
}

pub fn run_code_review(cfg: &Config, only_pr: Option<u32>) {
    preflight(cfg);
    log("Starting code review...");

    // Resolve bot token so the review subprocess runs under the bot identity.
    let bot_token = cfg
        .effective_bot_credentials()
        .as_ref()
        .and_then(resolve_bot_token);

    if bot_token.is_none() {
        log(
            "WARNING: No bot credentials configured — reviews will run under your identity \
             (same-author approvals will fail). Set DEV_BOT_TOKEN or configure a GitHub App.",
        );
    }

    let extra_env: Vec<(String, String)> = bot_token
        .as_deref()
        .map(|t| vec![("GH_TOKEN".to_string(), t.to_string())])
        .unwrap_or_default();

    let mut prs = list_open_prs();
    if let Some(n) = only_pr {
        prs.retain(|pr| pr.number == n);
    }
    if prs.is_empty() {
        let msg = if only_pr.is_some() {
            "No open pull request matched the requested number for code review."
        } else {
            "No open PRs to review."
        };
        log(msg);
        emit_event(AgentEvent::Done);
        return;
    }

    log(&format!("Found {} open PR(s)", prs.len()));
    if cfg.dry_run {
        log_resolved_agent_launch(cfg, &[]);
    }

    for pr in &prs {
        log(&format!("Reviewing PR #{}: {}", pr.number, pr.title));

        if cfg.dry_run {
            log(&format!("[dry-run] Would review PR #{}", pr.number));
            continue;
        }

        let body = pr_body(pr.number);
        let diff = pr_diff(pr.number);
        let threads = fetch_unresolved_review_threads(pr.number, DEFAULT_REVIEW_BOT_LOGIN);
        let prior_review_context = render_prior_pr_review_context(&fetch_pr_reviews(pr.number));
        let prompt = if threads.is_empty() {
            build_code_review_prompt(
                &cfg.project_name,
                pr.number,
                &pr.title,
                &body,
                &diff,
                &prior_review_context,
            )
        } else {
            log(&format!(
                "PR #{} has {} unresolved bot-authored thread(s) — follow-up verification review (not a full audit).",
                pr.number,
                threads.len()
            ));
            build_review_followup_code_review_prompt(
                &cfg.project_name,
                pr.number,
                &pr.title,
                &body,
                &diff,
                &threads,
                &prior_review_context,
            )
        };
        run_agent_with_env(cfg, &prompt, &extra_env);
        if stop_requested() {
            log("Stop requested. Code review cancelled.");
            emit_event(AgentEvent::Done);
            return;
        }

        log(&format!("Completed review of PR #{}", pr.number));
    }

    log("All code reviews complete.");
    emit_event(AgentEvent::Done);
}

pub fn run_security_code_review(cfg: &Config) {
    use crate::agent::snapshot::generate_codebase_snapshot;
    preflight(cfg);
    log("Starting security code review...");

    let crate_tree = cmd_stdout("tree", &["-L", "2", "crates"]).unwrap_or_default();
    let snapshot = generate_codebase_snapshot(&cfg.root);
    let prompt =
        build_security_review_prompt(&cfg.project_name, &crate_tree, &snapshot, cfg.dry_run);
    run_agent_with_env(cfg, &prompt, &[]);
    emit_event(AgentEvent::Done);
}

/// RAII guard that removes a git worktree on drop, including on panic.
/// When `restore` is set, the guard also re-attaches the main checkout's HEAD
/// after the throwaway worktree is gone (the order matters: the throwaway must
/// release the branch before the main checkout can re-attach to it).
///
/// `root` is the parent repo's worktree path. We pass it via `-C` so the
/// worktree commands operate on the right repo regardless of the process cwd
/// (important when this guard runs from a test harness or library context
/// that does not chdir into `cfg.root`).
pub struct WorktreeGuard {
    pub path: PathBuf,
    pub root: PathBuf,
    pub restore: Option<MainHeadRestore>,
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        let path_str = self.path.to_string_lossy().to_string();
        let root_str = self.root.to_string_lossy().to_string();
        if !cmd_run(
            "git",
            &["-C", &root_str, "worktree", "remove", "--force", &path_str],
        ) {
            log(&format!(
                "WARNING: `git worktree remove` failed for {path_str}; falling back to fs cleanup"
            ));
            let _ = std::fs::remove_dir_all(&self.path);
            let _ = cmd_run("git", &["-C", &root_str, "worktree", "prune"]);
        }
        if let Some(restore) = &self.restore {
            restore_main_head(restore);
        }
    }
}

/// Re-attach the main checkout's HEAD per `restore`. Used by both
/// [`WorktreeGuard::drop`] and the inline error path in
/// [`run_pr_review_fix_scoped`] / [`crate::agent::conflicts::run_pr_conflict_fix`]
/// when `git worktree add` fails after we detached the main checkout.
pub(crate) fn restore_main_head(restore: &MainHeadRestore) {
    let root = restore.root.to_string_lossy().to_string();
    let ok = match &restore.target {
        RestoreTarget::Branch(name) => cmd_run("git", &["-C", &root, "checkout", name]),
        RestoreTarget::Detached(sha) => cmd_run("git", &["-C", &root, "checkout", "--detach", sha]),
    };
    let label = match &restore.target {
        RestoreTarget::Branch(name) => format!("branch '{name}'"),
        RestoreTarget::Detached(sha) => format!("commit {sha} (detached)"),
    };
    if ok {
        log(&format!(
            "Restored main checkout HEAD to {label} in {root}."
        ));
    } else {
        let short = match &restore.target {
            RestoreTarget::Branch(name) => name.clone(),
            RestoreTarget::Detached(sha) => sha.clone(),
        };
        log(&format!(
            "WARNING: failed to restore main checkout HEAD to {label} in {root}. \
             Operator may need to run `git -C {root} checkout {short}` manually."
        ));
    }
}

/// One worktree entry parsed from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeEntry {
    pub path: String,
    /// Full ref form, e.g. `refs/heads/agent/issue-67`. `None` for detached
    /// or bare worktrees.
    pub branch: Option<String>,
}

/// Pure parser for `git worktree list --porcelain` output. Each entry is a
/// block of lines (`worktree <path>`, `HEAD <sha>`, then either
/// `branch <ref>`, `detached`, or `bare`) separated by blank lines.
pub(crate) fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut out = Vec::new();
    let mut cur_path: Option<String> = None;
    let mut cur_branch: Option<String> = None;
    for line in porcelain.lines() {
        if line.is_empty() {
            if let Some(p) = cur_path.take() {
                out.push(WorktreeEntry {
                    path: p,
                    branch: cur_branch.take(),
                });
            }
            cur_branch = None;
        } else if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(p) = cur_path.take() {
                out.push(WorktreeEntry {
                    path: p,
                    branch: cur_branch.take(),
                });
            }
            cur_path = Some(rest.to_string());
            cur_branch = None;
        } else if let Some(rest) = line.strip_prefix("branch ") {
            cur_branch = Some(rest.trim().to_string());
        }
    }
    if let Some(p) = cur_path.take() {
        out.push(WorktreeEntry {
            path: p,
            branch: cur_branch,
        });
    }
    out
}

/// Does `a` resolve to the same filesystem location as `b`? Tries exact
/// equality first, then canonicalization (handles macOS `/var` vs `/private/var`,
/// trailing slashes, symlinks).
fn paths_match_canonical(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ac), Ok(bc)) => ac == bc,
        _ => false,
    }
}

/// True iff `path` is a throwaway caretta worktree directory: a child of
/// `temp_dir` whose name starts with one of the prefixes used by the caretta
/// throwaway-worktree flows (`caretta-pr-` for fix-pr, `caretta-conflicts-pr-`
/// for conflict resolution).
pub(crate) fn is_caretta_temp_worktree_path(path: &str, temp_dir: &Path) -> bool {
    const PREFIXES: &[&str] = &["caretta-pr-", "caretta-conflicts-pr-"];
    let p = Path::new(path);
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
        return false;
    }
    p.parent()
        .is_some_and(|parent| paths_match_canonical(parent, temp_dir))
}

/// Result of [`prepare_branch_for_worktree_add`].
pub(crate) enum PrepareBranchOutcome {
    /// Branch is now free; the worktree add will succeed. If we had to detach
    /// the main checkout to get here, `restore` carries what to put back when
    /// the throwaway worktree is dropped.
    Ready { restore: Option<MainHeadRestore> },
    /// Cannot safely free the branch (e.g. dirty main checkout, or some
    /// non-caretta worktree holds it). The caller should log `reason` and bail.
    Aborted { reason: String },
}

/// Free `target_branch` so a subsequent `git worktree add --force -B
/// <target_branch> <tmp>` will succeed.
///
/// Steps:
/// 1. Prune any stale caretta `caretta-pr-*` worktrees that currently hold the
///    branch (they would otherwise block `-B`'s force-reset).
/// 2. If after pruning the branch is still held by another worktree:
///    - When it's the main checkout AND its working tree is clean, detach the
///      main checkout and return the restore target so the caller can put the
///      branch back on drop.
///    - Otherwise (dirty main checkout, or some unrelated worktree holds it),
///      return `Aborted` — we won't silently risk operator work.
pub(crate) fn prepare_branch_for_worktree_add(
    repo_root: &Path,
    target_branch: &str,
) -> PrepareBranchOutcome {
    let root_str = repo_root.to_string_lossy().to_string();
    let temp_dir = std::env::temp_dir();
    let full_ref = format!("refs/heads/{target_branch}");

    let porcelain = cmd_stdout("git", &["-C", &root_str, "worktree", "list", "--porcelain"])
        .unwrap_or_default();
    let entries = parse_worktree_list(&porcelain);

    let mut pruned_any = false;
    for entry in &entries {
        if entry.branch.as_deref() == Some(&full_ref)
            && is_caretta_temp_worktree_path(&entry.path, &temp_dir)
        {
            log(&format!(
                "Pruning stale caretta worktree at {} (holds branch '{}').",
                entry.path, target_branch
            ));
            if !cmd_run(
                "git",
                &[
                    "-C",
                    &root_str,
                    "worktree",
                    "remove",
                    "--force",
                    &entry.path,
                ],
            ) {
                let _ = std::fs::remove_dir_all(&entry.path);
            }
            pruned_any = true;
        }
    }
    if pruned_any {
        let _ = cmd_run("git", &["-C", &root_str, "worktree", "prune"]);
    }

    let porcelain = cmd_stdout("git", &["-C", &root_str, "worktree", "list", "--porcelain"])
        .unwrap_or_default();
    let entries = parse_worktree_list(&porcelain);
    let Some(holder) = entries
        .iter()
        .find(|e| e.branch.as_deref() == Some(&full_ref))
    else {
        return PrepareBranchOutcome::Ready { restore: None };
    };

    let holder_path = Path::new(&holder.path);
    if !paths_match_canonical(holder_path, repo_root) {
        return PrepareBranchOutcome::Aborted {
            reason: format!(
                "Branch '{target_branch}' is still checked out at '{}' after pruning caretta orphans. \
                 That worktree was not created by caretta — refusing to disturb it. \
                 Remove it manually (`git worktree remove '{}'`) if it is stale.",
                holder.path, holder.path
            ),
        };
    }

    let dirty = cmd_stdout("git", &["-C", &root_str, "status", "--porcelain"]).unwrap_or_default();
    if !dirty.trim().is_empty() {
        return PrepareBranchOutcome::Aborted {
            reason: format!(
                "Main checkout at '{root_str}' is on branch '{target_branch}' with a dirty working tree. \
                 Refusing to detach HEAD to free the branch (would risk uncommitted work). \
                 Commit, stash, or discard changes in the main checkout, then retry."
            ),
        };
    }

    let short = cmd_stdout(
        "git",
        &[
            "-C",
            &root_str,
            "symbolic-ref",
            "--quiet",
            "--short",
            "HEAD",
        ],
    )
    .filter(|s| !s.is_empty());
    let target = if let Some(name) = short {
        RestoreTarget::Branch(name)
    } else {
        let sha = cmd_stdout("git", &["-C", &root_str, "rev-parse", "HEAD"]).unwrap_or_default();
        if sha.is_empty() {
            return PrepareBranchOutcome::Aborted {
                reason: format!(
                    "Could not read HEAD in main checkout at '{root_str}'; cannot safely detach."
                ),
            };
        }
        RestoreTarget::Detached(sha)
    };

    log(&format!(
        "Detaching main checkout HEAD at '{root_str}' to free branch '{target_branch}' (will restore on completion)."
    ));
    if !cmd_run("git", &["-C", &root_str, "checkout", "--detach"]) {
        return PrepareBranchOutcome::Aborted {
            reason: format!(
                "Failed to detach HEAD in main checkout at '{root_str}' to free branch '{target_branch}'."
            ),
        };
    }

    PrepareBranchOutcome::Ready {
        restore: Some(MainHeadRestore {
            root: repo_root.to_path_buf(),
            target,
        }),
    }
}

/// Which unresolved threads to load for fix-comments (`run_pr_review_fix` vs
/// [`run_issue_pr_review_resume`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PrReviewFixThreadScope {
    /// Bot / `[bot]` / `@caretta fix` marker (same filter as before #146-style tooling).
    #[default]
    ActionableBot,
    /// Every unresolved inline thread on the PR (for tracker `issue` pseudo-resume).
    AllInline,
}

fn review_threads_for_fix(pr_num: u32, scope: PrReviewFixThreadScope) -> Vec<ReviewThread> {
    match scope {
        PrReviewFixThreadScope::ActionableBot => {
            fetch_unresolved_review_threads(pr_num, DEFAULT_REVIEW_BOT_LOGIN)
        }
        PrReviewFixThreadScope::AllInline => fetch_all_unresolved_review_threads(pr_num),
    }
}

pub fn run_pr_review_fix(cfg: &Config, pr_num: u32) {
    run_pr_review_fix_scoped(cfg, pr_num, PrReviewFixThreadScope::ActionableBot, true);
}

/// Fix-comments path for [`crate::agent::issue::work_on_issue`] when an `agent/issue-*` PR
/// is already open: same worktree + verification flow as [`run_pr_review_fix`], but threads
/// are all unresolved inline comments (not restricted to the review bot). Never submits an
/// approving review — that remains for the bot `code-review` step.
pub(crate) fn run_issue_pr_review_resume(cfg: &Config, pr_num: u32) {
    run_pr_review_fix_scoped(cfg, pr_num, PrReviewFixThreadScope::AllInline, false);
}

fn run_pr_review_fix_scoped(
    cfg: &Config,
    pr_num: u32,
    scope: PrReviewFixThreadScope,
    try_approve_if_fully_resolved: bool,
) {
    preflight(cfg);
    log(&format!("Starting Fix Comments run for PR #{pr_num}..."));

    if cfg.dry_run {
        log(&format!(
            "[dry-run] Would run Fix Comments for PR #{pr_num}"
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let threads = review_threads_for_fix(pr_num, scope);
    if threads.is_empty() {
        let detail = match scope {
            PrReviewFixThreadScope::ActionableBot => {
                "No unresolved bot-authored review threads found"
            }
            PrReviewFixThreadScope::AllInline => "No unresolved inline review threads found",
        };
        log(&format!("{detail} for PR #{pr_num}."));
        emit_event(AgentEvent::Done);
        return;
    }

    let branch = pr_head_branch(pr_num);
    let title = list_open_prs()
        .into_iter()
        .find(|pr| pr.number == pr_num)
        .map(|pr| pr.title)
        .unwrap_or_else(|| format!("PR #{pr_num}"));
    let diff = pr_diff(pr_num);

    let Some((_guard, worktree_path)) = setup_pr_worktree(cfg, pr_num, &branch, "Fix Comments")
    else {
        emit_event(AgentEvent::Done);
        return;
    };
    let worktree_str = worktree_path.to_string_lossy().to_string();

    let prompt =
        build_pr_review_fix_prompt(&cfg.project_name, pr_num, &title, &branch, &diff, &threads);

    if !run_agent_with_env_in_dir(cfg, &prompt, &[], &worktree_path) {
        log(&format!("Fix Comments agent failed for PR #{pr_num}."));
        emit_event(AgentEvent::Done);
        return;
    }
    if stop_requested() {
        log("Stop requested. Fix Comments run cancelled.");
        emit_event(AgentEvent::Done);
        return;
    }

    let status =
        cmd_stdout("git", &["-C", &worktree_str, "status", "--porcelain"]).unwrap_or_default();
    if status.trim().is_empty() {
        log(&format!(
            "Fix Comments made no file changes for PR #{pr_num}; leaving review threads unresolved."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let message = format!(
        "fix review comments on PR #{pr_num}\n\n{}",
        cfg.agent.co_author()
    );
    if !commit_and_push_worktree_changes(
        cfg,
        pr_num,
        &branch,
        &worktree_path,
        &message,
        "Fix Comments",
    ) {
        emit_event(AgentEvent::Done);
        return;
    }

    let verified_ids = run_verification_pass(cfg, pr_num, &threads, &worktree_path);
    let resolved = threads
        .iter()
        .filter(|thread| verified_ids.contains(&thread.id) && resolve_review_thread(&thread.id))
        .count();
    log(&format!(
        "Fix Comments complete for PR #{pr_num}: pushed changes and resolved {resolved}/{} thread(s).",
        threads.len()
    ));

    if resolved == threads.len() {
        if try_approve_if_fully_resolved {
            try_approve_pr(cfg, pr_num);
        } else {
            log(&format!(
                "PR #{pr_num}: all targeted threads resolved — leaving approval to the code-review flow (issue runner does not approve its own PR)."
            ));
        }
    } else {
        log(&format!(
            "Skipping auto-approve for PR #{pr_num}: {} thread(s) still unresolved.",
            threads.len() - resolved
        ));
    }
    emit_event(AgentEvent::Done);
}

/// Run the verification agent pass and return the set of thread IDs it
/// confirmed are addressed by the new code. On any failure (agent error,
/// missing/malformed verdict file, stop requested) returns an empty set so the
/// caller leaves all threads unresolved — better to require a human than to
/// rubber-stamp a wrong fix.
fn run_verification_pass(
    cfg: &Config,
    pr_num: u32,
    threads: &[crate::agent::tracker::ReviewThread],
    worktree_path: &Path,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let empty = HashSet::new();

    let verdict_path = std::env::temp_dir().join(format!(
        "caretta-pr-{pr_num}-{}-verify.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&verdict_path);
    let verdict_path_str = verdict_path.to_string_lossy().to_string();

    let post_fix_diff = pr_diff(pr_num);
    let prompt = build_pr_review_verification_prompt(
        &cfg.project_name,
        pr_num,
        &post_fix_diff,
        threads,
        &verdict_path_str,
    );

    log(&format!(
        "Running verification pass for PR #{pr_num} (verdict file: {verdict_path_str})."
    ));
    if !run_agent_with_env_in_dir(cfg, &prompt, &[], worktree_path) {
        log(&format!(
            "Verification agent failed for PR #{pr_num}; treating all threads as unverified."
        ));
        return empty;
    }
    if stop_requested() {
        log("Stop requested during verification pass; treating all threads as unverified.");
        return empty;
    }

    let json = match std::fs::read_to_string(&verdict_path) {
        Ok(s) => s,
        Err(err) => {
            log(&format!(
                "Verification verdict file missing for PR #{pr_num} ({err}); treating all threads as unverified."
            ));
            return empty;
        }
    };
    let _ = std::fs::remove_file(&verdict_path);

    let Some(verdict) = parse_verification_verdict(&json) else {
        log(&format!(
            "Verification verdict for PR #{pr_num} was not valid JSON; treating all threads as unverified."
        ));
        return empty;
    };

    if !verdict.unverified.is_empty() {
        for unv in &verdict.unverified {
            log(&format!(
                "Thread {id} unverified: {reason}",
                id = unv.id,
                reason = unv.reason
            ));
        }
    }
    verdict.verified.into_iter().collect()
}

/// Approve a PR if all bot-authored review threads are resolved and the
/// current `reviewDecision` is `CHANGES_REQUESTED`. Returns `true` when an
/// approval review was successfully submitted.
///
/// Uses the bot identity (`DEV_BOT_TOKEN` or App-minted token) so the approval
/// counts against `CHANGES_REQUESTED` left by the same bot. Without bot
/// credentials the call will fail (GitHub rejects self-approval).
pub fn try_approve_pr(cfg: &Config, pr_num: u32) -> bool {
    let unresolved = fetch_unresolved_review_threads(pr_num, DEFAULT_REVIEW_BOT_LOGIN);
    if !unresolved.is_empty() {
        log(&format!(
            "PR #{pr_num} still has {} unresolved bot-authored thread(s); not approving.",
            unresolved.len()
        ));
        return false;
    }

    let decision = pr_review_decision(pr_num).unwrap_or_default();
    if decision != "CHANGES_REQUESTED" {
        log(&format!(
            "PR #{pr_num} reviewDecision is {decision:?}; nothing to clear (no approval submitted)."
        ));
        return false;
    }

    let bot_token = cfg
        .effective_bot_credentials()
        .as_ref()
        .and_then(resolve_bot_token);
    let env: Vec<(String, String)> = bot_token
        .as_deref()
        .map(|t| vec![("GH_TOKEN".to_string(), t.to_string())])
        .unwrap_or_default();
    if env.is_empty() {
        log(
            "WARNING: No bot credentials configured; approval will run under your identity \
             and GitHub rejects self-approval. Set DEV_BOT_TOKEN or configure a GitHub App.",
        );
    }

    let body =
        "All requested changes have been addressed. Approving via caretta code-review follow-up.";
    let ok = Gh::submit_pr_review_with_env(pr_num, "--approve", body, &env);
    if ok {
        log(&format!("Approved PR #{pr_num}."));
    } else {
        log(&format!(
            "WARNING: failed to submit approve review on PR #{pr_num}."
        ));
    }
    ok
}

/// Fetch `origin/{branch}` and lay down a throwaway worktree on that branch
/// at `<tmp>/caretta-pr-<pr_num>-<pid>`. Returns the [`WorktreeGuard`] (which
/// cleans up on drop) and the worktree path on success.
///
/// `flow_label` is used in log messages so the operator can tell which
/// caller (Fix Comments, Fix Failing Checks, …) failed when setup aborts.
///
/// Returns `None` on any failure (already logged). Shared by [`run_pr_review_fix_scoped`]
/// and [`crate::agent::fix_pr::run_pr_failing_checks_fix`].
pub(crate) fn setup_pr_worktree(
    cfg: &Config,
    pr_num: u32,
    branch: &str,
    flow_label: &str,
) -> Option<(WorktreeGuard, PathBuf)> {
    let worktree_path =
        std::env::temp_dir().join(format!("caretta-pr-{pr_num}-{}", std::process::id()));
    let worktree_str = worktree_path.to_string_lossy().to_string();
    let remote_ref = format!("origin/{branch}");

    let fetch_refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    if !cmd_run("git", &["fetch", "origin", &fetch_refspec]) {
        log(&format!("Failed to fetch branch '{branch}' from origin."));
        return None;
    }

    let restore_after_add = match prepare_branch_for_worktree_add(Path::new(&cfg.root), branch) {
        PrepareBranchOutcome::Aborted { reason } => {
            log(&reason);
            log(&format!("Aborting {flow_label} run for PR #{pr_num}."));
            return None;
        }
        PrepareBranchOutcome::Ready { restore } => restore,
    };

    if !cmd_run(
        "git",
        &[
            "-C",
            &cfg.root,
            "worktree",
            "add",
            "--force",
            "-B",
            branch,
            &worktree_str,
            &remote_ref,
        ],
    ) {
        if let Some(restore) = &restore_after_add {
            restore_main_head(restore);
        }
        log(&format!(
            "Failed to create worktree for PR #{pr_num} from {remote_ref}."
        ));
        return None;
    }

    let guard = WorktreeGuard {
        path: worktree_path.clone(),
        root: PathBuf::from(&cfg.root),
        restore: restore_after_add,
    };
    Some((guard, worktree_path))
}

/// Stage everything in `worktree_path`, commit with `message`, then push
/// `branch` to origin. Retries the commit up to [`MAX_COMMIT_ATTEMPTS`] times,
/// invoking the agent between attempts to address pre-commit hook failures.
///
/// Returns `false` (and logs) on any failure. Stop-requested mid-retry also
/// returns `false`.
pub(crate) fn commit_and_push_worktree_changes(
    cfg: &Config,
    pr_num: u32,
    branch: &str,
    worktree_path: &Path,
    message: &str,
    flow_label: &str,
) -> bool {
    let worktree_str = worktree_path.to_string_lossy().to_string();
    let mut committed = false;
    for attempt in 1..=MAX_COMMIT_ATTEMPTS {
        if !cmd_run("git", &["-C", &worktree_str, "add", "."]) {
            log(&format!(
                "{flow_label} commit attempt {attempt} failed at `git add` for PR #{pr_num}, retrying..."
            ));
            std::thread::sleep(std::time::Duration::from_secs(2));
            continue;
        }
        let (commit_ok, commit_out) =
            cmd_capture("git", &["-C", &worktree_str, "commit", "-m", message]);
        if !commit_out.is_empty() {
            eprint!("{commit_out}");
        }
        if commit_ok {
            committed = true;
            break;
        }
        log(&format!(
            "{flow_label} commit attempt {attempt} failed for PR #{pr_num}, retrying..."
        ));
        if attempt < MAX_COMMIT_ATTEMPTS {
            log(
                "Invoking agent to address pre-commit hook failures before the next commit attempt.",
            );
            let fix_prompt = build_commit_hook_fix_prompt(&commit_out);
            run_agent_with_env_in_dir(cfg, &fix_prompt, &[], worktree_path);
            if stop_requested() {
                log(&format!(
                    "Stop requested during hook-fix pass; aborting {flow_label} commit retries."
                ));
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    if !committed {
        log(&format!(
            "Failed to commit {flow_label} changes for PR #{pr_num}."
        ));
        return false;
    }

    if !cmd_run("git", &["-C", &worktree_str, "push", "origin", branch]) {
        log(&format!(
            "Failed to push {flow_label} changes for PR #{pr_num}."
        ));
        return false;
    }
    true
}

#[cfg(test)]
mod worktree_prep_tests {
    use super::{
        MainHeadRestore, PrepareBranchOutcome, RestoreTarget, WorktreeEntry, WorktreeGuard,
        is_caretta_temp_worktree_path, parse_worktree_list, prepare_branch_for_worktree_add,
        restore_main_head,
    };
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    /// Minimal `git init -b main` repo with one empty commit on `main` so HEAD
    /// is a valid symbolic ref.
    fn init_repo() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        run(&root, &["init", "-q", "-b", "main"]);
        run(&root, &["config", "user.email", "t@example.com"]);
        run(&root, &["config", "user.name", "tester"]);
        run(&root, &["config", "commit.gpgsign", "false"]);
        run(&root, &["commit", "--allow-empty", "-qm", "initial"]);
        (dir, root)
    }

    fn run(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed in {root:?}");
    }

    fn current_branch(root: &Path) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn head_sha(root: &Path) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("rev-parse");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn parses_single_branch_entry() {
        let input = "\
worktree /path/to/main
HEAD abc123
branch refs/heads/main
";
        assert_eq!(
            parse_worktree_list(input),
            vec![WorktreeEntry {
                path: "/path/to/main".into(),
                branch: Some("refs/heads/main".into()),
            }]
        );
    }

    #[test]
    fn parses_multiple_entries_including_detached() {
        let input = "\
worktree /a
HEAD abc
branch refs/heads/main

worktree /b
HEAD def
detached

worktree /c
HEAD ghi
branch refs/heads/agent/issue-67
";
        let got = parse_worktree_list(input);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].branch.as_deref(), Some("refs/heads/main"));
        assert_eq!(got[1].branch, None);
        assert_eq!(got[2].branch.as_deref(), Some("refs/heads/agent/issue-67"));
    }

    #[test]
    fn caretta_temp_path_classifier_matches_both_caretta_prefixes() {
        let temp = std::env::temp_dir();
        let fix_pr = temp.join("caretta-pr-67-12345");
        let conflicts = temp.join("caretta-conflicts-pr-67-12345");
        let unrelated = temp.join("not-caretta-67-12345");
        let nested = temp.join("nested").join("caretta-pr-67-12345");
        assert!(is_caretta_temp_worktree_path(
            &fix_pr.to_string_lossy(),
            &temp
        ));
        assert!(is_caretta_temp_worktree_path(
            &conflicts.to_string_lossy(),
            &temp
        ));
        assert!(!is_caretta_temp_worktree_path(
            &unrelated.to_string_lossy(),
            &temp
        ));
        assert!(!is_caretta_temp_worktree_path(
            &nested.to_string_lossy(),
            &temp
        ));
    }

    #[test]
    fn prepare_returns_ready_with_no_restore_when_branch_unused() {
        let (_dir, root) = init_repo();
        // Branch exists but main checkout is on `main`, not on `agent/issue-99`.
        run(&root, &["branch", "agent/issue-99"]);
        match prepare_branch_for_worktree_add(&root, "agent/issue-99") {
            PrepareBranchOutcome::Ready { restore } => assert!(restore.is_none()),
            PrepareBranchOutcome::Aborted { reason } => panic!("unexpected abort: {reason}"),
        }
        // Main checkout is untouched.
        assert_eq!(current_branch(&root).as_deref(), Some("main"));
    }

    #[test]
    fn prepare_detaches_clean_main_and_restore_puts_branch_back() {
        let (_dir, root) = init_repo();
        // Put the main checkout ON `agent/issue-101` — this is the Bug A
        // scenario verbatim.
        run(&root, &["checkout", "-b", "agent/issue-101"]);
        assert_eq!(current_branch(&root).as_deref(), Some("agent/issue-101"));

        let outcome = prepare_branch_for_worktree_add(&root, "agent/issue-101");
        let restore = match outcome {
            PrepareBranchOutcome::Ready { restore } => restore.expect("restore should be set"),
            PrepareBranchOutcome::Aborted { reason } => panic!("unexpected abort: {reason}"),
        };
        // HEAD should now be detached on the same commit.
        assert_eq!(current_branch(&root), None, "HEAD should be detached");
        assert!(matches!(
            &restore.target,
            RestoreTarget::Branch(b) if b == "agent/issue-101"
        ));

        restore_main_head(&restore);
        assert_eq!(current_branch(&root).as_deref(), Some("agent/issue-101"));
    }

    #[test]
    fn prepare_aborts_when_main_checkout_is_dirty() {
        let (_dir, root) = init_repo();
        run(&root, &["checkout", "-b", "agent/issue-102"]);
        // Stage a dirty change so detach would risk operator work.
        std::fs::write(root.join("scratch.txt"), b"wip").unwrap();
        run(&root, &["add", "scratch.txt"]);

        match prepare_branch_for_worktree_add(&root, "agent/issue-102") {
            PrepareBranchOutcome::Aborted { reason } => {
                assert!(
                    reason.contains("dirty working tree"),
                    "abort reason should call out the dirty tree: {reason}"
                );
            }
            PrepareBranchOutcome::Ready { .. } => {
                panic!("prepare should refuse to detach a dirty main checkout")
            }
        }
        // Main checkout unchanged: still on the branch, change still staged.
        assert_eq!(current_branch(&root).as_deref(), Some("agent/issue-102"));
    }

    #[test]
    fn prepare_prunes_stale_caretta_pr_orphan_worktree() {
        let (_dir, root) = init_repo();
        // The branch exists but main checkout is on `main`. An orphan from a
        // prior killed fix-pr run lives at <temp>/caretta-pr-N-pid and holds
        // `agent/issue-103`.
        run(&root, &["branch", "agent/issue-103"]);
        let orphan_dir =
            std::env::temp_dir().join(format!("caretta-pr-103-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&orphan_dir);
        run(
            &root,
            &[
                "worktree",
                "add",
                "-B",
                "agent/issue-103",
                orphan_dir.to_string_lossy().as_ref(),
                "agent/issue-103",
            ],
        );

        // Sanity: the orphan is registered and holds the branch.
        let listed = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .expect("worktree list");
        let listed_s = String::from_utf8_lossy(&listed.stdout);
        assert!(
            listed_s.contains("refs/heads/agent/issue-103"),
            "precondition: orphan should hold the branch:\n{listed_s}"
        );

        match prepare_branch_for_worktree_add(&root, "agent/issue-103") {
            PrepareBranchOutcome::Ready { restore } => assert!(restore.is_none()),
            PrepareBranchOutcome::Aborted { reason } => panic!("unexpected abort: {reason}"),
        }

        // The orphan is no longer registered.
        let listed = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .expect("worktree list");
        let listed_s = String::from_utf8_lossy(&listed.stdout);
        assert!(
            !listed_s.contains(orphan_dir.to_string_lossy().as_ref()),
            "orphan should be pruned:\n{listed_s}"
        );
        let _ = std::fs::remove_dir_all(&orphan_dir);
    }

    #[test]
    fn worktree_guard_drop_runs_restore_after_remove() {
        // End-to-end: detach main, "use" a throwaway worktree, then drop the
        // guard and confirm the main checkout is back on the original branch.
        // This exercises the Drop order requirement (remove first → restore).
        let (_dir, root) = init_repo();
        run(&root, &["checkout", "-b", "agent/issue-104"]);
        let sha_before = head_sha(&root);

        let throwaway = std::env::temp_dir().join(format!("caretta-pr-104-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&throwaway);

        let outcome = prepare_branch_for_worktree_add(&root, "agent/issue-104");
        let restore = match outcome {
            PrepareBranchOutcome::Ready { restore } => restore.expect("restore set"),
            PrepareBranchOutcome::Aborted { reason } => panic!("unexpected abort: {reason}"),
        };
        run(
            &root,
            &[
                "worktree",
                "add",
                "--force",
                "-B",
                "agent/issue-104",
                throwaway.to_string_lossy().as_ref(),
                "agent/issue-104",
            ],
        );
        {
            let _guard = WorktreeGuard {
                path: throwaway.clone(),
                root: root.clone(),
                restore: Some(MainHeadRestore {
                    root: restore.root.clone(),
                    target: restore.target.clone(),
                }),
            };
            // Guard is alive — main checkout is still detached.
            assert_eq!(current_branch(&root), None);
        }
        // After drop: throwaway is gone, main is back on the branch.
        assert_eq!(current_branch(&root).as_deref(), Some("agent/issue-104"));
        assert_eq!(head_sha(&root), sha_before);
        assert!(
            !throwaway.exists()
                || std::fs::read_dir(&throwaway)
                    .map(|d| d.count() == 0)
                    .unwrap_or(true)
        );
        let _ = std::fs::remove_dir_all(&throwaway);
    }
}
