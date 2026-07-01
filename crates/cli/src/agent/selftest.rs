// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Operational self-test: confirms the local environment can actually run the
//! agent workflows for the current configuration.
//!
//! The report is produced by [`run_self_test`] (desktop only) and surfaced in
//! the UI title bar via the "Self-test" button. The same shape is also served
//! from the `caretta serve` API at `/api/selftest` so the web build (running
//! through `dx serve`) can render the same report.
//!
//! Checks are intentionally cheap and read-only (except verification of optional
//! **GitHub review bot** credentials, which performs a live `github.com` REST
//! call when a token or GitHub App is configured):
//! - the configured agent CLI is on `PATH`,
//! - host tools `git` and `gh` are on `PATH`,
//! - the workspace root exists and is a git repository,
//! - bundled / overridden skill files are reachable on disk,
//! - review bot PAT / GitHub App credentials authenticate when present.
//!
//! Each check is a [`SelfTestCheck`] with a [`CheckStatus`]; the report's
//! [`SelfTestReport::overall`] collapses them into a single status that the
//! UI uses to colour the badge.

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crate::agent::adapter_dispatch::native_base_command;
#[cfg(not(target_arch = "wasm32"))]
use crate::agent::cmd::has_command;
#[cfg(not(target_arch = "wasm32"))]
use crate::agent::types::{BotAuthMode, BotCredentials, Config};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// Outcome of a single self-test check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The check completed and the environment is in the expected state.
    Pass,
    /// The check completed but the environment is in a degraded — but still
    /// usable — state (e.g. an optional CLI is missing).
    Warn,
    /// The check failed; agent workflows depending on this resource will
    /// likely not work.
    Fail,
    /// The check could not run in the current build (e.g. process spawning is
    /// not available in the browser).
    Skipped,
}

impl CheckStatus {
    /// Short human-readable label, also used by the UI's status badge.
    pub fn label(self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
            CheckStatus::Skipped => "skipped",
        }
    }
}

/// A single named check in a [`SelfTestReport`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfTestCheck {
    /// Stable display name (e.g. `"Agent CLI on PATH"`).
    pub name: String,
    /// Outcome bucket.
    pub status: CheckStatus,
    /// One-line detail shown under the row — file path, version string, error
    /// message, etc.
    pub detail: String,
}

impl SelfTestCheck {
    fn new(name: impl Into<String>, status: CheckStatus, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}

/// Full self-test result for the active [`Config`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfTestReport {
    /// Stringified agent (`claude`, `codex`, …) at the moment the report was
    /// generated.
    pub agent: String,
    /// Workspace root the report was generated against.
    pub root: String,
    /// Individual checks, displayed in order.
    pub checks: Vec<SelfTestCheck>,
}

impl SelfTestReport {
    /// Collapse the per-check statuses into one summary status.
    ///
    /// Rules: any `Fail` → `Fail`; otherwise any `Warn` → `Warn`; otherwise
    /// `Pass`. `Skipped` rows are ignored for the summary so a no-op report
    /// (e.g. on web) doesn't pretend to be green.
    pub fn overall(&self) -> CheckStatus {
        let mut has_warn = false;
        let mut has_pass = false;
        for c in &self.checks {
            match c.status {
                CheckStatus::Fail => return CheckStatus::Fail,
                CheckStatus::Warn => has_warn = true,
                CheckStatus::Pass => has_pass = true,
                CheckStatus::Skipped => {}
            }
        }
        if has_warn {
            CheckStatus::Warn
        } else if has_pass {
            CheckStatus::Pass
        } else {
            CheckStatus::Skipped
        }
    }

    /// Short human-readable summary, e.g. `"3 pass, 1 warn, 0 fail"`.
    pub fn summary(&self) -> String {
        let mut pass = 0;
        let mut warn = 0;
        let mut fail = 0;
        let mut skipped = 0;
        for c in &self.checks {
            match c.status {
                CheckStatus::Pass => pass += 1,
                CheckStatus::Warn => warn += 1,
                CheckStatus::Fail => fail += 1,
                CheckStatus::Skipped => skipped += 1,
            }
        }
        let mut parts = vec![
            format!("{pass} pass"),
            format!("{warn} warn"),
            format!("{fail} fail"),
        ];
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }
        parts.join(", ")
    }
}

/// Build a [`SelfTestReport`] indicating the runtime cannot self-introspect.
/// Used in the web (wasm32) build before the `/api/selftest` response lands,
/// or when the API is unreachable.
pub fn unsupported_report(
    agent: impl Into<String>,
    root: impl Into<String>,
    detail: impl Into<String>,
) -> SelfTestReport {
    SelfTestReport {
        agent: agent.into(),
        root: root.into(),
        checks: vec![SelfTestCheck::new(
            "Debug runtime",
            CheckStatus::Skipped,
            detail,
        )],
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Run the full set of operational checks against `cfg`.
///
/// All checks are synchronous and may shell out to `which`. Callers should run
/// this inside [`tokio::task::spawn_blocking`] (or similar) so the Dioxus UI
/// stays responsive.
pub fn run_self_test(cfg: &Config) -> SelfTestReport {
    let mut checks: Vec<SelfTestCheck> = Vec::new();

    // 1. Agent CLI: resolve the binary the dispatcher would actually exec.
    let agent_binary = native_base_command(cfg.agent, "").binary;
    let detail = format!("`{}` (selected agent: {})", agent_binary, cfg.agent);
    if has_command(&agent_binary) {
        checks.push(SelfTestCheck::new(
            "Agent CLI on PATH",
            CheckStatus::Pass,
            detail,
        ));
    } else {
        checks.push(SelfTestCheck::new(
            "Agent CLI on PATH",
            CheckStatus::Fail,
            format!("{detail} — not found. Install it or pick a different agent in caretta.toml."),
        ));
    }

    // 2. Required host tools.
    for (name, required) in [("git", true), ("gh", false), ("cargo", false)] {
        let present = has_command(name);
        let status = match (present, required) {
            (true, _) => CheckStatus::Pass,
            (false, true) => CheckStatus::Fail,
            (false, false) => CheckStatus::Warn,
        };
        let detail = if present {
            format!("`{name}` available")
        } else if required {
            format!("`{name}` not found on PATH (required)")
        } else {
            format!("`{name}` not found on PATH (optional)")
        };
        checks.push(SelfTestCheck::new(
            format!("Host tool: {name}"),
            status,
            detail,
        ));
    }

    // 3. Workspace root.
    let root_path = Path::new(&cfg.root);
    if root_path.is_dir() {
        checks.push(SelfTestCheck::new(
            "Workspace root",
            CheckStatus::Pass,
            cfg.root.clone(),
        ));
        if root_path.join(".git").exists() {
            checks.push(SelfTestCheck::new(
                "Git repository",
                CheckStatus::Pass,
                ".git found at workspace root",
            ));
        } else {
            checks.push(SelfTestCheck::new(
                "Git repository",
                CheckStatus::Warn,
                "Workspace root is not a git repository — PR / commit flows will fail.",
            ));
        }
    } else {
        checks.push(SelfTestCheck::new(
            "Workspace root",
            CheckStatus::Fail,
            format!("{} is not a directory", cfg.root),
        ));
    }

    // 4. GitHub review bot (credentials from DES / DEV_BOT_*; live GitHub check when configured).
    push_review_bot_checks(&mut checks, cfg);

    // 5. Skill paths. Paths in caretta.toml may be repo-relative, so resolve
    //    them against the workspace root before checking existence.
    let resolve = |p: &str| -> std::path::PathBuf {
        let candidate = Path::new(p);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            root_path.join(candidate)
        }
    };
    for (label, path) in [
        ("Skill: issue-tracking", &cfg.skill_paths.issue_tracking),
        ("Skill: user-personas", &cfg.skill_paths.user_personas),
    ] {
        let resolved = resolve(path);
        if resolved.exists() {
            checks.push(SelfTestCheck::new(
                label,
                CheckStatus::Pass,
                resolved.display().to_string(),
            ));
        } else {
            checks.push(SelfTestCheck::new(
                label,
                CheckStatus::Warn,
                format!(
                    "{} not found (will fall back to bundled assets)",
                    resolved.display()
                ),
            ));
        }
    }

    // 6. Model selection: warn if the selected agent supports models but none
    //    is configured. This is a frequent cause of "the run starts but the
    //    agent picks the wrong model".
    use crate::agent::types::AgentExt;
    let models = cfg.agent.available_models();
    if !models.is_empty() {
        let configured = cfg.model.trim();
        if configured.is_empty() {
            checks.push(SelfTestCheck::new(
                "Model selection",
                CheckStatus::Warn,
                format!(
                    "No model selected for `{}` — the adapter will pick its default.",
                    cfg.agent
                ),
            ));
        } else if models.iter().any(|(id, _)| *id == configured) {
            checks.push(SelfTestCheck::new(
                "Model selection",
                CheckStatus::Pass,
                format!("`{configured}` is a known model for {}", cfg.agent),
            ));
        } else {
            checks.push(SelfTestCheck::new(
                "Model selection",
                CheckStatus::Warn,
                format!(
                    "`{configured}` is not in the known model list for {} — typo?",
                    cfg.agent
                ),
            ));
        }
    }

    SelfTestReport {
        agent: cfg.agent.to_string(),
        root: cfg.root.clone(),
        checks,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_review_bot_checks(checks: &mut Vec<SelfTestCheck>, cfg: &Config) {
    use crate::agent::{bot, cmd::has_command};

    if cfg.bot_settings.mode != BotAuthMode::Disabled && !cfg.has_bot_credentials() {
        checks.push(SelfTestCheck::new(
            "GitHub review bot",
            CheckStatus::Warn,
            format!(
                "{} credentials are incomplete — approve/review-bot flows cannot authenticate.",
                cfg.bot_settings.mode
            ),
        ));
        return;
    }

    match cfg.effective_bot_credentials() {
        None => checks.push(SelfTestCheck::new(
            "GitHub review bot",
            CheckStatus::Warn,
            "Not configured — GitHub does not allow a user to approve their own pull request. Set up reviewer bot credentials (token or GitHub App) when you need approvals from a separate bot identity.",
        )),
        Some(BotCredentials::Token(tok)) => {
            if !has_command("curl") {
                checks.push(SelfTestCheck::new(
                    "GitHub review bot",
                    CheckStatus::Warn,
                    "`curl` not found on PATH — skipped token verification.",
                ));
                return;
            }
            match bot::verify_github_bot_token_rest(&tok) {
                Ok(()) => checks.push(SelfTestCheck::new(
                    "GitHub review bot",
                    CheckStatus::Pass,
                    "Token accepted by the GitHub REST API.",
                )),
                Err(e) => checks.push(SelfTestCheck::new(
                    "GitHub review bot",
                    CheckStatus::Fail,
                    e,
                )),
            }
        }
        Some(BotCredentials::GitHubApp {
            app_id,
            installation_id,
            private_key_pem,
        }) => {
            if !has_command("curl") {
                checks.push(SelfTestCheck::new(
                    "GitHub review bot",
                    CheckStatus::Fail,
                    "`curl` must be on PATH to authenticate a GitHub App.",
                ));
                return;
            }
            match bot::mint_installation_access_token(
                app_id.as_str(),
                installation_id.as_str(),
                private_key_pem.as_str(),
            ) {
                Ok(_) => checks.push(SelfTestCheck::new(
                    "GitHub review bot",
                    CheckStatus::Pass,
                    format!(
                        "GitHub App authenticated (installation `{installation_id}` for app `{app_id}`)."
                    ),
                )),
                Err(e) => checks.push(SelfTestCheck::new("GitHub review bot", CheckStatus::Fail, e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(status: CheckStatus) -> SelfTestCheck {
        SelfTestCheck::new("test", status, "")
    }

    #[test]
    fn overall_returns_fail_when_any_check_fails() {
        let report = SelfTestReport {
            agent: "claude".into(),
            root: ".".into(),
            checks: vec![
                check(CheckStatus::Pass),
                check(CheckStatus::Fail),
                check(CheckStatus::Warn),
            ],
        };
        assert_eq!(report.overall(), CheckStatus::Fail);
    }

    #[test]
    fn overall_returns_warn_when_only_warnings_present() {
        let report = SelfTestReport {
            agent: "claude".into(),
            root: ".".into(),
            checks: vec![check(CheckStatus::Pass), check(CheckStatus::Warn)],
        };
        assert_eq!(report.overall(), CheckStatus::Warn);
    }

    #[test]
    fn overall_returns_pass_when_all_pass() {
        let report = SelfTestReport {
            agent: "claude".into(),
            root: ".".into(),
            checks: vec![check(CheckStatus::Pass), check(CheckStatus::Pass)],
        };
        assert_eq!(report.overall(), CheckStatus::Pass);
    }

    #[test]
    fn overall_is_skipped_when_only_skipped_rows() {
        let report = SelfTestReport {
            agent: "claude".into(),
            root: ".".into(),
            checks: vec![check(CheckStatus::Skipped)],
        };
        assert_eq!(report.overall(), CheckStatus::Skipped);
    }

    #[test]
    fn summary_counts_each_bucket() {
        let report = SelfTestReport {
            agent: "claude".into(),
            root: ".".into(),
            checks: vec![
                check(CheckStatus::Pass),
                check(CheckStatus::Pass),
                check(CheckStatus::Warn),
                check(CheckStatus::Fail),
                check(CheckStatus::Skipped),
            ],
        };
        assert_eq!(report.summary(), "2 pass, 1 warn, 1 fail, 1 skipped");
    }

    #[test]
    fn unsupported_report_marks_check_as_skipped() {
        let report = unsupported_report("claude", "/repo", "browser sandbox");
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].status, CheckStatus::Skipped);
        assert_eq!(report.overall(), CheckStatus::Skipped);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn run_self_test_produces_report_for_current_workspace() {
        use cli_common::{Agent, Config, PricingConfig, TestCommands};
        let cfg = Config {
            agent: Agent::Claude,
            model: String::new(),
            auto_mode: false,
            dry_run: true,
            geodynamo_url: None,
            local_inference: Default::default(),
            root: env!("CARGO_MANIFEST_DIR").to_string(),
            project_name: "caretta-test".to_string(),
            scan_targets: Default::default(),
            skill_paths: Default::default(),
            bootstrap_agent_files: false,
            bootstrap_snapshot: false,
            workflow_preset: "default".to_string(),
            use_subscription: false,
            pricing: PricingConfig::default(),
            bot_settings: Default::default(),
            bot_credentials: None,
            test: TestCommands::default(),
            visual_regression: Default::default(),
            deploy: Default::default(),
            workspace: None,
        };
        let report = run_self_test(&cfg);
        assert_eq!(report.agent, "claude");
        // The crate root is a directory containing src/, so the workspace
        // root check must at least pass.
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "Workspace root" && c.status == CheckStatus::Pass),
            "expected the workspace root check to pass; got: {:?}",
            report.checks
        );
        // Always exercise the summary string so a panic here would catch
        // future refactors that introduce divide-by-zero or bad formatting.
        let _ = report.summary();
    }
}
