use crate::agent::cmd::{cmd_capture, cmd_run, log, origin_default_branch};
use crate::agent::issue::preflight;
use crate::agent::process::{emit_event, stop_requested};
use crate::agent::run::run_agent;
use crate::agent::tracker::{build_refresh_agents_prompt, build_refresh_docs_prompt};
use crate::agent::types::{AgentEvent, BRANCH_PREFIX, Config};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn enumerate_agent_files(cfg: &Config) -> Vec<String> {
    let root_path = Path::new(&cfg.root);
    let assets = crate::agent::assets::assets_dir();
    let mut files = BTreeSet::new();

    let agents_md = assets.join("AGENTS.md");
    if agents_md.exists() {
        files.insert(agents_md.to_string_lossy().to_string());
    }

    let skills_dir = assets.join("skills");
    if skills_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&skills_dir)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists() {
                    files.insert(skill_md.to_string_lossy().to_string());
                }
            }
        }
    }

    for preset_skill_dir in
        crate::agent::workflow::preset_skill_dirs(&cfg.root, &cfg.workflow_preset)
    {
        if preset_skill_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&preset_skill_dir)
        {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let skill_md = entry.path().join("SKILL.md");
                    if skill_md.exists() {
                        files.insert(skill_md.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    for name in &[
        "CLAUDE.md",
        "CLINE.md",
        "GEMINI.md",
        "COPILOT.md",
        "GROK.md",
        "JUNIE.md",
        "XAI.md",
    ] {
        let p = root_path.join(name);
        if p.exists() {
            files.insert(name.to_string());
        }
    }

    files.into_iter().collect()
}

pub fn run_refresh_agents(cfg: &Config) {
    preflight(cfg);
    log("Starting Refresh Agents...");

    let agent_files = enumerate_agent_files(cfg);
    if agent_files.is_empty() {
        log("No agent-facing files found — nothing to refresh.");
        emit_event(AgentEvent::Done);
        return;
    }

    log(&format!("Found {} agent-facing file(s)", agent_files.len()));

    let prompt = build_refresh_agents_prompt(&cfg.project_name, &agent_files);

    if cfg.dry_run {
        log("[dry-run] Would run Refresh Agents to review agent-facing docs");
        log(&format!(
            "[dry-run] Files in scope: {}",
            agent_files.join(", ")
        ));
        log(&format!("[dry-run] Prompt length: {} chars", prompt.len()));
        emit_event(AgentEvent::Done);
        return;
    }

    // Create a working branch.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let branch = format!("{BRANCH_PREFIX}refresh-agents-{ts}");
    let trunk = origin_default_branch();
    cmd_run("git", &["checkout", &trunk]);
    cmd_run("git", &["branch", "-D", &branch]);
    cmd_run("git", &["checkout", "-b", &branch]);

    run_agent(cfg, &prompt);
    if stop_requested() {
        log("Stop requested. Refresh Agents cancelled.");
        cmd_run("git", &["checkout", &trunk]);
        emit_event(AgentEvent::Done);
        return;
    }

    // Check if the agent made any changes.
    let (_, status_out) = cmd_capture("git", &["status", "--porcelain"]);
    if status_out.trim().is_empty() {
        log("No drift detected — agent-facing docs are up to date.");
        cmd_run("git", &["checkout", &trunk]);
        cmd_run("git", &["branch", "-D", &branch]);
        emit_event(AgentEvent::Done);
        return;
    }

    // PR logic omitted for brevity, should use `gh pr create`.
    log("Drift detected. PR creation logic should be implemented.");
    emit_event(AgentEvent::Done);
}

pub fn git_status_porcelain_scoped(cwd: Option<&Path>, paths: &[String]) -> String {
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(paths.iter().map(|s| s.as_str()));
    let mut cmd = std::process::Command::new("git");
    cmd.args(&args);
    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    let output = cmd.output().expect("failed to execute git status");
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn git_staged_files(cwd: Option<&Path>) -> Vec<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["diff", "--name-only", "--cached"]);
    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    let output = cmd.output().expect("failed to execute git diff");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub fn git_commit_paths(cwd: Option<&Path>, message: &str, paths: &[String]) -> bool {
    let mut args = vec!["commit", "-m", message, "--"];
    args.extend(paths.iter().map(|s| s.as_str()));
    let mut cmd = std::process::Command::new("git");
    cmd.args(&args);
    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

pub fn enumerate_project_doc_files(cfg: &Config) -> Vec<String> {
    let root = Path::new(&cfg.root);
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") && !name.starts_with(".") {
                files.push(name);
            }
        }
    }
    files
}

pub fn run_refresh_docs(cfg: &Config) {
    preflight(cfg);
    log("Starting Refresh Docs...");

    let doc_files = enumerate_project_doc_files(cfg);
    if doc_files.is_empty() {
        log("No project docs found — nothing to refresh.");
        emit_event(AgentEvent::Done);
        return;
    }

    log(&format!("Found {} project doc file(s)", doc_files.len()));

    let prompt = build_refresh_docs_prompt(&cfg.project_name, &doc_files);

    if cfg.dry_run {
        log("[dry-run] Would run Refresh Docs to review project docs");
        log(&format!(
            "[dry-run] Files in scope: {}",
            doc_files.join(", ")
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    run_agent(cfg, &prompt);
    emit_event(AgentEvent::Done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn init_temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(root)
            .status()
            .unwrap();
        fs::write(root.join("README.md"), "# Initial\n").unwrap();
        fs::write(root.join("STATUS.md"), "# Status\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "README.md", "STATUS.md"])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .status()
            .unwrap();
        dir
    }

    #[test]
    fn git_status_porcelain_scoped_filters_correctly() {
        let dir = init_temp_repo();
        let root = dir.path();

        fs::write(root.join("README.md"), "edit 1\n").unwrap();
        fs::write(root.join("STATUS.md"), "edit 2\n").unwrap();

        let doc_files = ["README.md".to_string()];
        let scoped = git_status_porcelain_scoped(Some(root), &doc_files);
        assert!(scoped.contains("README.md"), "got: {scoped:?}");
        assert!(!scoped.contains("STATUS.md"), "got: {scoped:?}");
    }

    #[test]
    fn refresh_docs_commit_paths_excludes_preexisting_staged_files() {
        let dir = init_temp_repo();
        let root = dir.path();

        // Pre-existing staged out-of-scope file.
        fs::write(root.join("secret.env"), "TOKEN=abc\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "secret.env"])
            .current_dir(root)
            .status()
            .unwrap();

        // Simulate the agent producing a doc change.
        fs::write(root.join("README.md"), "agent edit\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "--", "README.md"])
            .current_dir(root)
            .status()
            .unwrap();

        let doc_files = ["README.md".to_string(), "STATUS.md".to_string()];
        assert!(git_commit_paths(
            Some(root),
            "refresh project docs",
            &doc_files
        ));

        let committed = std::process::Command::new("git")
            .args(["show", "--name-only", "--pretty=format:", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        let committed_out = String::from_utf8_lossy(&committed.stdout);
        assert!(committed_out.lines().any(|line| line == "README.md"));
        assert!(!committed_out.lines().any(|line| line == "secret.env"));
        assert_eq!(git_staged_files(Some(root)), vec!["secret.env".to_string()]);
    }
}
