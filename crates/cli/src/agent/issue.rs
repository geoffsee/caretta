use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_stdout, count_tokens, die, has_command, log, origin_default_branch,
};
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::stop_requested;
use crate::agent::run::run_agent;
use crate::agent::snapshot::generate_codebase_snapshot;
use crate::agent::tracker::{
    build_prompt, build_test_fix_prompt, fetch_issue, find_upstream_branch, get_tracker_body,
    parse_pending, pending_issues_execution_order,
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
        log(&format!(
            "[dry-run] Prompt ({} tokens). Would work on #{issue_num}, then open PR.\n\n---\n{}",
            count_tokens(&prompt),
            prompt
        ));
        return;
    }

    let branch = format!("{BRANCH_PREFIX}{issue_num}");
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
        log("Skipping test step (no `[test] command` configured in freq-ai.toml).");
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
                run_agent(cfg, &fix_prompt);
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
            format!("Closes #{issue_num}\n\nAutomated PR opened by freq-ai issue runner.");
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
