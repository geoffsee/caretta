use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_stdout, count_tokens, die, has_command, log, origin_default_branch,
};
use crate::agent::event_log::{
    AgentRunRecord, append_run, extract_run_data, iso8601_now, preview_entry, resolve_db_path,
};
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::{drain_run_capture, start_run_capture, stop_requested};
use crate::agent::review::run_issue_pr_review_resume;
use crate::agent::run::run_agent;
use crate::agent::snapshot::generate_codebase_snapshot;
use crate::agent::tracker::{
    ReviewThread, build_prompt, build_test_fix_prompt, fetch_all_unresolved_review_threads,
    fetch_issue, find_upstream_branch, get_tracker_body, open_pr_number_for_head_branch,
    parse_pending, pending_issues_execution_order, pr_review_decision,
};
use crate::agent::types::{BRANCH_PREFIX, Config, MAX_COMMIT_ATTEMPTS, MAX_PUSH_ATTEMPTS};
use crate::timed;
use cli_common::PendingIssue;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::Instant;

/// Fetch `origin/{branch}` first; if it exists, check it out and fast-forward pull.
/// Otherwise create a new local branch (removing a stale local branch if needed).
fn checkout_issue_working_branch(branch: &str) {
    if cmd_run("git", &["fetch", "origin", branch]) {
        let origin_ref = format!("origin/{branch}");
        if !cmd_run("git", &["checkout", "-B", branch, &origin_ref]) {
            die(&format!(
                "Fetched {origin_ref} but could not check out local branch '{branch}'."
            ));
        }
        if !cmd_run("git", &["pull", "--ff-only", "origin", branch]) {
            log(&format!(
                "Note: could not fast-forward '{branch}' from origin (continuing at checkout)."
            ));
        }
    } else {
        if cmd_stdout(
            "git",
            &[
                "rev-parse",
                "--quiet",
                "--verify",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_some()
        {
            cmd_run("git", &["branch", "-D", branch]);
        }
        if !cmd_run("git", &["checkout", "-b", branch]) {
            die(&format!("Could not create working branch '{branch}'."));
        }
    }
}

pub fn work_on_issue(cfg: &Config, tracker_num: u32, issue_num: u32, blockers: &[u32]) {
    if stop_requested() {
        return;
    }
    let (title, body) = fetch_issue(issue_num);
    log(&format!("Issue #{issue_num}: {title}"));

    let tracker_body = if tracker_num != 0 {
        get_tracker_body(tracker_num)
    } else {
        String::new()
    };

    let (resolved_preset_name, resolved_preset_version) = {
        use crate::agent::workflow::resolve_preset;
        match resolve_preset(&cfg.root, &cfg.workflow_preset) {
            Ok((name, ver)) => (Some(name), Some(ver)),
            Err(e) if e.contains(crate::agent::workflow::VERSION_MISMATCH_TAG) => {
                die(&format!("Preset version constraint not satisfied: {e}"));
            }
            Err(e) => {
                log(&format!("WARNING: preset resolution failed: {e}"));
                let (name, _) = crate::agent::workflow::parse_preset_ref(&cfg.workflow_preset)
                    .unwrap_or_else(|_| (cfg.workflow_preset.clone(), None));
                (Some(name), None)
            }
        }
    };

    if cfg.dry_run {
        let codebase =
            if !cfg.bootstrap_snapshot || env::var("DISABLE_TOAK").is_ok_and(|v| v == "1") {
                if !cfg.bootstrap_snapshot {
                    log("Skipping bootstrap snapshot (disabled in config)");
                } else {
                    log("Skipping bootstrap snapshot (DISABLE_TOAK=1)");
                }
                String::new()
            } else {
                generate_codebase_snapshot(&cfg.root)
            };
        let prompt = build_prompt(
            &cfg.project_name,
            issue_num,
            &title,
            &body,
            &codebase,
            tracker_num,
            &tracker_body,
        );
        log_resolved_agent_launch(cfg, &[]);
        let prompt_tokens = count_tokens(&prompt) as u32;
        let now = iso8601_now();
        let dry_record = AgentRunRecord {
            agent_id: cfg.agent.to_string(),
            model: cfg.model.clone(),
            workflow_phase: "issue".to_string(),
            issue_number: Some(issue_num),
            tracker_number: (tracker_num != 0).then_some(tracker_num),
            tool_calls: vec![],
            input_tokens: Some(prompt_tokens),
            output_tokens: None,
            status: "dry-run".to_string(),
            started_at: now.clone(),
            finished_at: now,
            duration_ms: 0,
            path_constraints: cfg.path_constraints.clone(),
            policy_violations: vec![],
            preset_name: resolved_preset_name.clone(),
            preset_version: resolved_preset_version.clone(),
        };
        log(&format!(
            "[dry-run] Prompt ({prompt_tokens} tokens). Would work on #{issue_num}, then open PR.\n\n---\n{}",
            prompt
        ));
        log(&format!(
            "[dry-run] Preview event log entry:\n{}",
            preview_entry(&dry_record)
        ));
        return;
    }

    let branch = format!("{BRANCH_PREFIX}{issue_num}");
    if let Some(pr_num) = open_pr_number_for_head_branch(&branch) {
        let decision = pr_review_decision(pr_num).unwrap_or_default();
        let threads: Vec<ReviewThread> = fetch_all_unresolved_review_threads(pr_num);
        let thread_count = threads.len();
        match pr_open_action(&decision, thread_count) {
            PrOpenAction::SkipApproved => {
                log(&format!(
                    "Open PR #{pr_num} for branch '{branch}' is already approved — skipping implementation run for issue #{issue_num}."
                ));
                return;
            }
            PrOpenAction::FixComments => {
                log(&format!(
                    "Open PR #{pr_num} has {thread_count} unresolved inline review thread(s) — pseudo-resuming fix-comments on that branch (skipping a full implementation pass)."
                ));
                let review_started_at = iso8601_now();
                let review_wall_clock = Instant::now();
                start_run_capture();
                run_issue_pr_review_resume(cfg, pr_num, threads);
                let review_duration_ms = review_wall_clock.elapsed().as_millis() as u64;
                let review_finished_at = iso8601_now();
                let captured = drain_run_capture();
                let (tool_calls, input_tokens, output_tokens, review_status, event_model) =
                    extract_run_data(&captured);
                let effective_model = event_model.unwrap_or_else(|| cfg.model.clone());
                let db_path = resolve_db_path(cfg.event_log_path.as_deref());
                #[cfg(not(target_arch = "wasm32"))]
                let review_violations =
                    crate::agent::path_constraint::check_run(&tool_calls, &cfg.path_constraints);
                #[cfg(target_arch = "wasm32")]
                let review_violations: Vec<
                    crate::agent::event_log::PolicyViolation,
                > = vec![];
                append_run(
                    &AgentRunRecord {
                        agent_id: cfg.agent.to_string(),
                        model: effective_model,
                        workflow_phase: "review-fix".to_string(),
                        issue_number: Some(issue_num),
                        tracker_number: (tracker_num != 0).then_some(tracker_num),
                        tool_calls,
                        input_tokens,
                        output_tokens,
                        status: review_status,
                        started_at: review_started_at,
                        finished_at: review_finished_at,
                        duration_ms: review_duration_ms,
                        path_constraints: cfg.path_constraints.clone(),
                        policy_violations: review_violations,
                        preset_name: resolved_preset_name.clone(),
                        preset_version: resolved_preset_version.clone(),
                    },
                    &db_path,
                );
                return;
            }
            PrOpenAction::SkipDeferToReview => {
                let decision_label = if decision.is_empty() {
                    "none"
                } else {
                    decision.as_str()
                };
                log(&format!(
                    "Open PR #{pr_num} for branch '{branch}' (review decision: {decision_label}) has no unresolved inline review threads — skipping redundant implementation pass for issue #{issue_num}; deferring to code-review and fix-review-comments follow-up."
                ));
                return;
            }
        }
    }

    let trunk = origin_default_branch();
    let base = find_upstream_branch(blockers);

    // Start from the upstream dependency branch (or default trunk when unblocked).
    if base != trunk {
        log(&format!("Chaining off upstream branch '{base}'"));
        cmd_run("git", &["fetch", "origin", &base]);
        cmd_run("git", &["checkout", &base]);
        cmd_run("git", &["pull", "origin", &base]);
    } else {
        cmd_run("git", &["fetch", "origin", &trunk]);
        cmd_run("git", &["checkout", &trunk]);
        cmd_run("git", &["pull", "--ff-only", "origin", &trunk]);
    }
    checkout_issue_working_branch(&branch);

    let codebase = timed!("snapshot", {
        if !cfg.bootstrap_snapshot || env::var("DISABLE_TOAK").is_ok_and(|v| v == "1") {
            if !cfg.bootstrap_snapshot {
                log("Skipping bootstrap snapshot (disabled in config)");
            } else {
                log("Skipping bootstrap snapshot (DISABLE_TOAK=1)");
            }
            String::new()
        } else {
            generate_codebase_snapshot(&cfg.root)
        }
    });
    log(&format!(
        "Launching agent for issue #{issue_num} on branch '{branch}'..."
    ));
    let db_path = resolve_db_path(cfg.event_log_path.as_deref());
    let run_started_at = iso8601_now();
    let run_wall_clock = Instant::now();
    start_run_capture();
    let agent_ok = run_agent(
        cfg,
        &build_prompt(
            &cfg.project_name,
            issue_num,
            &title,
            &body,
            &codebase,
            tracker_num,
            &tracker_body,
        ),
    );
    let run_duration_ms = run_wall_clock.elapsed().as_millis() as u64;
    let run_finished_at = iso8601_now();
    let captured = drain_run_capture();
    let (tool_calls, input_tokens, output_tokens, run_status, event_model) =
        extract_run_data(&captured);
    let final_status = if run_status != "unknown" {
        run_status
    } else if agent_ok {
        "completed".to_string()
    } else {
        "failed".to_string()
    };
    let effective_model = event_model.unwrap_or_else(|| cfg.model.clone());

    // Check path constraints and log any violations.
    #[cfg(not(target_arch = "wasm32"))]
    let policy_violations =
        crate::agent::path_constraint::check_run(&tool_calls, &cfg.path_constraints);
    #[cfg(target_arch = "wasm32")]
    let policy_violations: Vec<crate::agent::event_log::PolicyViolation> = vec![];
    if !policy_violations.is_empty() {
        log(&format!(
            "Path-constraint policy: {} violation(s) detected for issue #{issue_num}",
            policy_violations.len()
        ));
        for v in &policy_violations {
            log(&format!(
                "  POLICY VIOLATION: tool={} path={} reason={}",
                v.tool, v.path, v.reason
            ));
        }
    }
    append_run(
        &AgentRunRecord {
            agent_id: cfg.agent.to_string(),
            model: effective_model,
            workflow_phase: "issue".to_string(),
            issue_number: Some(issue_num),
            tracker_number: (tracker_num != 0).then_some(tracker_num),
            tool_calls,
            input_tokens,
            output_tokens,
            status: final_status,
            started_at: run_started_at,
            finished_at: run_finished_at,
            duration_ms: run_duration_ms,
            path_constraints: cfg.path_constraints.clone(),
            policy_violations,
            preset_name: resolved_preset_name.clone(),
            preset_version: resolved_preset_version.clone(),
        },
        &db_path,
    );
    if agent_ok {
        log(&format!("Agent run completed for issue #{issue_num}."));
    } else {
        log(&format!(
            "Agent run exited unsuccessfully for issue #{issue_num}; continuing to checks."
        ));
    }
    if stop_requested() {
        log("Stop requested; halting issue workflow before tests/commit.");
        return;
    }

    if cfg.test.command.is_empty() {
        log("Skipping test step (no `[test] command` configured in caretta.toml).");
    } else {
        timed!("tests", {
            log(&format!("Running tests: {}", cfg.test.command.join(" ")));
            let (program, args) = cfg.test.command.split_first().expect("non-empty");
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let (ok, out) = cmd_capture(program, &arg_refs);
            if !ok {
                log(&format!(
                    "Tests failed for #{issue_num} — invoking agent to fix..."
                ));
                let fix_prompt = build_test_fix_prompt(issue_num, &out);
                let fix_started_at = iso8601_now();
                let fix_wall_clock = Instant::now();
                start_run_capture();
                let fix_ok = run_agent(cfg, &fix_prompt);
                let fix_duration_ms = fix_wall_clock.elapsed().as_millis() as u64;
                let fix_finished_at = iso8601_now();
                let fix_captured = drain_run_capture();
                let (
                    fix_tool_calls,
                    fix_input_tokens,
                    fix_output_tokens,
                    fix_run_status,
                    fix_event_model,
                ) = extract_run_data(&fix_captured);
                let fix_final_status = if fix_run_status != "unknown" {
                    fix_run_status
                } else if fix_ok {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                let fix_effective_model = fix_event_model.unwrap_or_else(|| cfg.model.clone());
                #[cfg(not(target_arch = "wasm32"))]
                let fix_violations = crate::agent::path_constraint::check_run(
                    &fix_tool_calls,
                    &cfg.path_constraints,
                );
                #[cfg(target_arch = "wasm32")]
                let fix_violations: Vec<crate::agent::event_log::PolicyViolation> = vec![];
                append_run(
                    &AgentRunRecord {
                        agent_id: cfg.agent.to_string(),
                        model: fix_effective_model,
                        workflow_phase: "test-fix".to_string(),
                        issue_number: Some(issue_num),
                        tracker_number: (tracker_num != 0).then_some(tracker_num),
                        tool_calls: fix_tool_calls,
                        input_tokens: fix_input_tokens,
                        output_tokens: fix_output_tokens,
                        status: fix_final_status,
                        started_at: fix_started_at,
                        finished_at: fix_finished_at,
                        duration_ms: fix_duration_ms,
                        path_constraints: cfg.path_constraints.clone(),
                        policy_violations: fix_violations,
                        preset_name: resolved_preset_name.clone(),
                        preset_version: resolved_preset_version.clone(),
                    },
                    &db_path,
                );
                if let Some((fmt_program, fmt_args)) = cfg.test.format_command.split_first() {
                    let fmt_arg_refs: Vec<&str> = fmt_args.iter().map(String::as_str).collect();
                    cmd_run(fmt_program, &fmt_arg_refs);
                }
            }
        });
    }

    let commit_msg = format!(
        "implement #{issue_num}: {title}\n\nCloses #{issue_num}\n\n{}",
        cfg.agent.co_author()
    );
    let push_ok = timed!(
        "commit",
        commit_with_retries(cfg, issue_num, &branch, &commit_msg)
    );
    if push_ok {
        let pr_title = format!("implement #{issue_num}: {title}");
        let pr_body =
            format!("Closes #{issue_num}\n\nAutomated PR opened by caretta issue runner.");
        create_pr_if_missing(&branch, &base, &pr_title, &pr_body);
    }
    log(&format!("Issue #{issue_num} loop iteration complete."));
}

/// Open a PR for `branch` against `base` if no open PR already exists for that
/// head branch. Idempotent: re-runs of the same issue won't fail just because
/// the PR is already open.
pub fn create_pr_if_missing(branch: &str, base: &str, title: &str, body: &str) -> bool {
    let (ok, existing) = cmd_capture(
        "gh",
        &[
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
        ],
    );
    if ok && !existing.trim().is_empty() {
        log(&format!(
            "PR already open for branch '{branch}': {}",
            existing.trim()
        ));
        return true;
    }
    if cmd_run(
        "gh",
        &[
            "pr", "create", "--head", branch, "--base", base, "--title", title, "--body", body,
        ],
    ) {
        log(&format!(
            "Opened PR for branch '{branch}' against '{base}'."
        ));
        return true;
    }
    log(&format!(
        "Failed to open PR for branch '{branch}' against '{base}'."
    ));
    false
}

pub fn commit_with_retries(_cfg: &Config, _issue_num: u32, branch: &str, message: &str) -> bool {
    let mut ok = false;
    for attempt in 1..=MAX_COMMIT_ATTEMPTS {
        if !cmd_run("git", &["add", "."]) {
            log(&format!("Commit attempt {attempt} failed, retrying..."));
            std::thread::sleep(std::time::Duration::from_secs(2));
            continue;
        }
        let (_, status_out) = cmd_capture("git", &["status", "--porcelain"]);
        if status_out.trim().is_empty() {
            log("Nothing to commit — working tree clean. Skipping commit, proceeding to push.");
            ok = true;
            break;
        }
        if cmd_run("git", &["commit", "-m", message]) {
            ok = true;
            break;
        }
        log(&format!("Commit attempt {attempt} failed, retrying..."));
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    if !ok {
        log("Failed to commit after multiple attempts.");
        return false;
    }

    for attempt in 1..=MAX_PUSH_ATTEMPTS {
        if cmd_run("git", &["push", "origin", branch, "--force"]) {
            return true;
        }
        log(&format!("Push attempt {attempt} failed, retrying..."));
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    log("Failed to push after multiple attempts.");
    false
}

pub fn preflight(cfg: &Config) {
    if !has_command("gh") {
        die("`gh` CLI not found. Please install GitHub CLI.");
    }
    if !has_command("git") {
        die("`git` not found.");
    }
    if !has_command("cargo") {
        die("`cargo` not found.");
    }

    if !cfg.dry_run {
        let root = Path::new(&cfg.root);
        if !root.join(".git").exists() {
            die(&format!(
                "Configured root ({}) is not a git repository.",
                cfg.root
            ));
        }
    }
}

/// Emit pending issue numbers for `tracker` as JSON (`json_fmt`) or one per line.
/// Uses [`pending_issues_execution_order`] so dependents follow pending blockers.
pub fn run_tracker_matrix(cfg: &Config, tracker_num: u32, json_fmt: bool) {
    let nums = if cfg.dry_run {
        if !json_fmt {
            log("[dry-run] tracker-matrix: skipping tracker fetch; emitting empty list");
        }
        Vec::new()
    } else {
        if !has_command("gh") {
            die("`gh` CLI not found. Please install GitHub CLI.");
        }
        let body = get_tracker_body(tracker_num);
        pending_issues_execution_order(&body)
    };

    if json_fmt {
        println!(
            "{}",
            serde_json::to_string(&nums).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        for n in nums {
            println!("{n}");
        }
    }
}

pub fn run_loop(cfg: &Config, tracker_num: u32) {
    preflight(cfg);
    log(&format!(
        "Agent started in loop mode on {} (tracker #{tracker_num})",
        cfg.project_name
    ));

    let mut cycle = 0u64;
    loop {
        if stop_requested() {
            break;
        }

        cycle += 1;
        log(&format!(
            "Loop heartbeat: cycle {cycle} reading tracker #{tracker_num}..."
        ));
        if cfg.dry_run {
            log("[dry-run] loop: skipping tracker fetch and exiting after one cycle");
            break;
        }
        let body = get_tracker_body(tracker_num);
        let order = pending_issues_execution_order(&body);
        let pending_by_num: HashMap<u32, PendingIssue> = parse_pending(&body)
            .into_iter()
            .map(|p| (p.number, p))
            .collect();
        if order.is_empty() {
            log(&format!(
                "Loop heartbeat: cycle {cycle} found no pending issues; sleeping 30s."
            ));
        } else {
            log(&format!(
                "Loop heartbeat: cycle {cycle} found {} pending issue(s).",
                order.len()
            ));
        }
        for issue_num in order {
            if stop_requested() {
                break;
            }
            let Some(issue) = pending_by_num.get(&issue_num) else {
                continue;
            };
            log(&format!(
                "Loop heartbeat: cycle {cycle} starting issue #{issue_num}."
            ));
            work_on_issue(cfg, tracker_num, issue.number, &issue.blockers);
        }

        if cfg.dry_run {
            break;
        }

        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

pub fn run_single_issue(cfg: &Config, tracker_num: u32, issue_num: u32, blockers: &[u32]) {
    preflight(cfg);
    work_on_issue(cfg, tracker_num, issue_num, blockers);
}

/// Action to take when an open PR already exists for an issue's working branch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PrOpenAction {
    /// PR is approved; nothing to do.
    SkipApproved,
    /// Unresolved inline review threads (any author); run the fix-comments flow.
    FixComments,
    /// PR is open but neither approved nor blocked on bot threads; skip the
    /// implementation pass and let the downstream code-review /
    /// fix-review-comments jobs drive the next iteration.
    SkipDeferToReview,
}

/// Decide what `work_on_issue` should do when a PR for the issue's branch is
/// already open. Pure: takes the GitHub review decision string and the count
/// of unresolved inline review threads (human and bot; see
/// [`crate::agent::tracker::fetch_all_unresolved_review_threads`]).
pub(crate) fn pr_open_action(decision: &str, unresolved_thread_count: usize) -> PrOpenAction {
    if decision.eq_ignore_ascii_case("APPROVED") {
        return PrOpenAction::SkipApproved;
    }
    if unresolved_thread_count > 0 {
        return PrOpenAction::FixComments;
    }
    PrOpenAction::SkipDeferToReview
}

#[cfg(test)]
mod tests {
    use super::{PrOpenAction, pr_open_action};

    #[test]
    fn approved_skips_regardless_of_threads() {
        assert_eq!(pr_open_action("APPROVED", 0), PrOpenAction::SkipApproved);
        assert_eq!(pr_open_action("APPROVED", 3), PrOpenAction::SkipApproved);
        assert_eq!(pr_open_action("approved", 0), PrOpenAction::SkipApproved);
    }

    #[test]
    fn unresolved_threads_trigger_fix_comments() {
        assert_eq!(
            pr_open_action("CHANGES_REQUESTED", 1),
            PrOpenAction::FixComments
        );
        assert_eq!(
            pr_open_action("REVIEW_REQUIRED", 5),
            PrOpenAction::FixComments
        );
        assert_eq!(pr_open_action("", 2), PrOpenAction::FixComments);
    }

    #[test]
    fn changes_requested_with_no_threads_defers_to_review() {
        assert_eq!(
            pr_open_action("CHANGES_REQUESTED", 0),
            PrOpenAction::SkipDeferToReview
        );
    }

    #[test]
    fn review_required_with_no_threads_defers_to_review() {
        assert_eq!(
            pr_open_action("REVIEW_REQUIRED", 0),
            PrOpenAction::SkipDeferToReview
        );
    }

    #[test]
    fn empty_decision_with_no_threads_defers_to_review() {
        // `pr_review_decision` returns `None` for PRs with no review yet;
        // `work_on_issue` collapses that to an empty string before calling.
        assert_eq!(pr_open_action("", 0), PrOpenAction::SkipDeferToReview);
    }
}
