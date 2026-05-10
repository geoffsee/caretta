use crate::agent::bot::resolve_bot_token;
use crate::agent::cmd::{cmd_run, cmd_run_env, cmd_stdout, log};
use crate::agent::issue::preflight;
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::{emit_event, stop_requested};
use crate::agent::run::{run_agent_with_env, run_agent_with_env_in_dir};
use crate::agent::tracker::{
    DEFAULT_REVIEW_BOT_LOGIN, build_code_review_prompt, build_pr_review_fix_prompt,
    build_pr_review_verification_prompt, build_security_review_prompt,
    fetch_unresolved_review_threads, list_open_prs, parse_verification_verdict, pr_body, pr_diff,
    pr_head_branch, pr_review_decision, resolve_review_thread,
};
use crate::agent::types::{AgentEvent, Config};
use std::path::{Path, PathBuf};

pub fn run_code_review(cfg: &Config) {
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

    let prs = list_open_prs();
    if prs.is_empty() {
        log("No open PRs to review.");
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
        let prompt =
            build_code_review_prompt(&cfg.project_name, pr.number, &pr.title, &body, &diff);
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
pub struct WorktreeGuard {
    pub path: PathBuf,
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        let path_str = self.path.to_string_lossy().to_string();
        if !cmd_run("git", &["worktree", "remove", "--force", &path_str]) {
            log(&format!(
                "WARNING: `git worktree remove` failed for {path_str}; falling back to fs cleanup"
            ));
            let _ = std::fs::remove_dir_all(&self.path);
            let _ = cmd_run("git", &["worktree", "prune"]);
        }
    }
}

pub fn run_pr_review_fix(cfg: &Config, pr_num: u32) {
    preflight(cfg);
    log(&format!("Starting Fix Comments run for PR #{pr_num}..."));

    if cfg.dry_run {
        log(&format!(
            "[dry-run] Would run Fix Comments for PR #{pr_num}"
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let threads = fetch_unresolved_review_threads(pr_num, DEFAULT_REVIEW_BOT_LOGIN);
    if threads.is_empty() {
        log(&format!(
            "No unresolved bot-authored review threads found for PR #{pr_num}."
        ));
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

    let worktree_path =
        std::env::temp_dir().join(format!("freq-ai-pr-{pr_num}-{}", std::process::id()));
    let worktree_str = worktree_path.to_string_lossy().to_string();
    let remote_ref = format!("origin/{branch}");

    let fetch_refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    if !cmd_run("git", &["fetch", "origin", &fetch_refspec]) {
        log(&format!("Failed to fetch branch '{branch}' from origin."));
        emit_event(AgentEvent::Done);
        return;
    }

    if !cmd_run(
        "git",
        &[
            "worktree",
            "add",
            "--force",
            "-B",
            &branch,
            &worktree_str,
            &remote_ref,
        ],
    ) {
        log(&format!(
            "Failed to create worktree for PR #{pr_num} from {remote_ref}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let _guard = WorktreeGuard {
        path: worktree_path.clone(),
    };
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
    let committed = cmd_run("git", &["-C", &worktree_str, "add", "."])
        && cmd_run("git", &["-C", &worktree_str, "commit", "-m", &message]);
    if !committed {
        log(&format!(
            "Failed to commit Fix Comments changes for PR #{pr_num}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    if !cmd_run("git", &["-C", &worktree_str, "push", "origin", &branch]) {
        log(&format!(
            "Failed to push Fix Comments changes for PR #{pr_num}."
        ));
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
        try_approve_pr(cfg, pr_num);
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
        "freq-ai-pr-{pr_num}-{}-verify.json",
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

    let pr_num_s = pr_num.to_string();
    let body =
        "All requested changes have been addressed. Approving via freq-ai code-review follow-up.";
    let ok = cmd_run_env(
        "gh",
        &["pr", "review", &pr_num_s, "--approve", "--body", body],
        &env,
    );
    if ok {
        log(&format!("Approved PR #{pr_num}."));
    } else {
        log(&format!(
            "WARNING: failed to submit approve review on PR #{pr_num}."
        ));
    }
    ok
}
