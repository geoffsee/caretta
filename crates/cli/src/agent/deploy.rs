use crate::agent::cmd::log;
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER};
use std::path::Path;
use std::process::{Command, Stdio};

const DEFAULT_DEPLOY_COMMAND: &[&str] = &["./scripts/deploy.sh"];

pub fn run_deploy(cfg: &Config) -> Result<(), String> {
    let deploy = &cfg.deploy;
    let Some((program, args)) = deploy.command.split_first() else {
        let guidance = missing_config_guidance();
        log(&guidance);
        notify_done();
        return Err(guidance);
    };

    log("Starting deploy workflow...");
    log(&format!(
        "Deploy environment context: {}",
        context_value(&deploy.environment)
    ));
    log(&format!(
        "Deploy url context: {}",
        context_value(&deploy.url)
    ));
    log(&format!(
        "Deploy command: {}",
        display_command(&deploy.command)
    ));

    if cfg.dry_run {
        log("[dry-run] Would run configured deploy command.");
        notify_done();
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, output) = capture_command_in(program, &arg_refs, Path::new(&cfg.root));
    if !output.trim().is_empty() {
        log(&format!("Deploy output:\n{}", output.trim_end()));
    }

    if ok {
        log("Deploy workflow complete.");
        notify_done();
        Ok(())
    } else {
        log("Deploy command failed.");
        notify_done();
        Err("Deploy command failed.".to_string())
    }
}

fn missing_config_guidance() -> String {
    format!(
        "No `[deploy].command` configured in caretta.toml. Add:\n\n\
         [deploy]\n\
         command = [{}]\n\
         environment = \"staging\"\n\
         url = \"https://staging.example.com\"",
        DEFAULT_DEPLOY_COMMAND
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

    fn test_config(deploy: DeployConfig) -> Config {
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
            visual_regression: VisualRegressionConfig::default(),
            deploy,
            workspace: None,
        }
    }

    #[test]
    fn deploy_requires_configured_command() {
        let cfg = test_config(DeployConfig::default());

        let err = run_deploy(&cfg).expect_err("missing command should fail");

        assert!(err.contains("[deploy]"));
        assert!(err.contains("deploy.sh"));
    }

    #[test]
    fn deploy_dry_run_does_not_spawn_command() {
        let mut cfg = test_config(DeployConfig {
            command: vec!["__caretta_missing_deploy_command__".to_string()],
            environment: "staging".to_string(),
            url: "https://staging.example.com".to_string(),
        });
        cfg.dry_run = true;

        run_deploy(&cfg).expect("dry run should not spawn command");
    }

    #[test]
    fn deploy_runs_command_from_repo_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "ok").expect("write marker");
        let mut cfg = test_config(DeployConfig {
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "test -f marker.txt && printf deploy-ok".to_string(),
            ],
            environment: String::new(),
            url: String::new(),
        });
        cfg.root = dir.path().to_string_lossy().into_owned();

        run_deploy(&cfg).expect("command should run in configured root");
    }
}
