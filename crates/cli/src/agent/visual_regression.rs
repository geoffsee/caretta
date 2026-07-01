// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::cmd::log;
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER};
use std::path::Path;
use std::process::{Command, Stdio};

const DEFAULT_VISUAL_REGRESSION_COMMAND: &[&str] =
    &["bun", "x", "playwright", "test", "tests/visual"];

pub fn run_visual_regression(cfg: &Config) -> Result<(), String> {
    let visual = &cfg.visual_regression;
    let Some((program, args)) = visual.command.split_first() else {
        let guidance = missing_config_guidance();
        log(&guidance);
        notify_done();
        return Err(guidance);
    };

    log("Starting visual regression workflow...");
    log(&format!(
        "Visual regression base_url context: {}",
        context_value(&visual.base_url)
    ));
    log(&format!(
        "Visual regression screenshots_dir context: {}",
        context_value(&visual.screenshots_dir)
    ));
    log(&format!(
        "Visual regression command: {}",
        display_command(&visual.command)
    ));

    if cfg.dry_run {
        log("[dry-run] Would run configured visual regression command.");
        notify_done();
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, output) = capture_command_in(program, &arg_refs, Path::new(&cfg.root));
    if !output.trim().is_empty() {
        log(&format!("Visual regression output:\n{}", output.trim_end()));
    }

    if ok {
        log("Visual regression workflow complete.");
        notify_done();
        Ok(())
    } else {
        log("Visual regression command failed.");
        notify_done();
        Err("Visual regression command failed.".to_string())
    }
}

fn missing_config_guidance() -> String {
    format!(
        "No `[visual_regression].command` configured in caretta.toml. Add:\n\n\
         [visual_regression]\n\
         command = [{}]\n\
         base_url = \"http://localhost:5173\"\n\
         screenshots_dir = \"tests/visual/screenshots\"",
        DEFAULT_VISUAL_REGRESSION_COMMAND
            .iter()
            .map(|part| format!("\"{part}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn context_value(value: &str) -> &str {
    if value.trim().is_empty() {
        "(not configured)"
    } else {
        value
    }
}

fn display_command(command: &[String]) -> String {
    command.join(" ")
}

fn capture_command_in(program: &str, args: &[&str], dir: &Path) -> (bool, String) {
    match Command::new(program)
        .args(args)
        .current_dir(dir)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
    {
        Ok(output) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            (output.status.success(), combined)
        }
        Err(err) => (false, err.to_string()),
    }
}

fn notify_done() {
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::Done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{
        Agent, BotSettings, Config, DeployConfig, LocalInferenceConfig, PricingConfig, ScanTargets,
        SkillPaths, TestCommands, VisualRegressionConfig,
    };

    fn test_config(visual_regression: VisualRegressionConfig) -> Config {
        Config {
            agent: Agent::Claude,
            model: String::new(),
            auto_mode: false,
            dry_run: false,
            geodynamo_url: None,
            local_inference: LocalInferenceConfig::default(),
            root: ".".to_string(),
            project_name: "caretta-test".to_string(),
            scan_targets: ScanTargets::default(),
            skill_paths: SkillPaths::default(),
            bootstrap_agent_files: false,
            bootstrap_snapshot: false,
            workflow_preset: "default".to_string(),
            use_subscription: false,
            pricing: PricingConfig::default(),
            bot_settings: BotSettings::default(),
            bot_credentials: None,
            test: TestCommands::default(),
            visual_regression,
            deploy: DeployConfig::default(),
            workspace: None,
            telemetry: cli_common::TelemetryConfig::default(),
        }
    }

    #[test]
    fn visual_regression_requires_configured_command() {
        let cfg = test_config(VisualRegressionConfig::default());

        let err = run_visual_regression(&cfg).expect_err("missing command should fail");

        assert!(err.contains("[visual_regression]"));
        assert!(err.contains("bun"));
    }

    #[test]
    fn visual_regression_dry_run_does_not_spawn_command() {
        let mut cfg = test_config(VisualRegressionConfig {
            command: vec!["__caretta_missing_visual_command__".to_string()],
            base_url: "http://localhost:5173".to_string(),
            screenshots_dir: "tests/visual/screenshots".to_string(),
        });
        cfg.dry_run = true;

        run_visual_regression(&cfg).expect("dry run should not spawn command");
    }

    #[test]
    fn visual_regression_runs_command_from_repo_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "ok").expect("write marker");
        let mut cfg = test_config(VisualRegressionConfig {
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "test -f marker.txt && printf visual-ok".to_string(),
            ],
            base_url: String::new(),
            screenshots_dir: String::new(),
        });
        cfg.root = dir.path().to_string_lossy().into_owned();

        run_visual_regression(&cfg).expect("command should run in configured root");
    }
}
