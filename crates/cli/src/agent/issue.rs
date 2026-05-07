use crate::agent::cmd::{cmd_capture, cmd_run, count_tokens, die, has_command, log};
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::stop_requested;
use crate::agent::run::run_agent;
use crate::agent::snapshot::generate_codebase_snapshot;
use crate::agent::tracker::{
    build_prompt, build_test_fix_prompt, fetch_issue, find_upstream_branch, get_tracker_body,
    parse_pending,
};
use crate::agent::types::{BRANCH_PREFIX, Config, MAX_COMMIT_ATTEMPTS, MAX_PUSH_ATTEMPTS};
use crate::timed;
use std::env;
use std::path::Path;
use std::time::Instant;

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
    let base = find_upstream_branch(blockers);

    // Start from the upstream dependency branch (or master if no blockers).
    if base != "master" {
        log(&format!("Chaining off upstream branch '{base}'"));
        cmd_run("git", &["fetch", "origin", &base]);
        cmd_run("git", &["checkout", &base]);
        cmd_run("git", &["pull", "origin", &base]);
    } else {
        cmd_run("git", &["checkout", "master"]);
    }
    cmd_run("git", &["branch", "-D", &branch]); // remove stale branch if any
    cmd_run("git", &["checkout", "-b", &branch]);

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

    timed!("tests", {
        log("Running tests...");
        if !cmd_run("./scripts/test-examples.sh", &[]) {
            log(&format!(
                "Tests failed for #{issue_num} — invoking agent to fix..."
            ));
            let (_, test_out) = cmd_capture("cargo", &["test", "--workspace"]);
            let fix_prompt = build_test_fix_prompt(issue_num, &test_out);
            run_agent(cfg, &fix_prompt);
            cmd_run("cargo", &["fmt", "--all"]);
        }
    });

    let commit_msg = format!(
        "implement #{issue_num}: {title}\n\nCloses #{issue_num}\n\n{}",
        cfg.agent.co_author()
    );
    timed!(
        "commit",
        commit_with_retries(cfg, issue_num, &branch, &commit_msg)
    );
    log(&format!("Issue #{issue_num} loop iteration complete."));
}

pub fn commit_with_retries(_cfg: &Config, _issue_num: u32, branch: &str, message: &str) -> bool {
    let mut ok = false;
    for attempt in 1..=MAX_COMMIT_ATTEMPTS {
        if cmd_run("git", &["add", "."]) && cmd_run("git", &["commit", "-m", message]) {
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
        let pending = parse_pending(&get_tracker_body(tracker_num));
        if pending.is_empty() {
            log(&format!(
                "Loop heartbeat: cycle {cycle} found no pending issues; sleeping 30s."
            ));
        } else {
            log(&format!(
                "Loop heartbeat: cycle {cycle} found {} pending issue(s).",
                pending.len()
            ));
        }
        for issue in pending {
            if stop_requested() {
                break;
            }
            // Real implementation would handle dependencies/blockers.
            log(&format!(
                "Loop heartbeat: cycle {cycle} starting issue #{}.",
                issue.number
            ));
            work_on_issue(cfg, tracker_num, issue.number, &issue.blockers);
        }

        if cfg.dry_run {
            break;
        }

        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

pub fn run_single_issue(cfg: &Config, tracker_num: u32, issue_num: u32) {
    preflight(cfg);
    work_on_issue(cfg, tracker_num, issue_num, &[]);
}
