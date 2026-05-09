use crate::agent::bot::load_bot_settings;
use crate::agent::cmd::{cmd_stdout, configure_log_redaction, die};
use crate::agent::config_store::load_local_inference_api_key;
use crate::agent::types::{Agent, Config};
use std::env;
use std::path::Path;

pub fn infer_project_name(root: &str) -> String {
    Path::new(root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into())
}

#[cfg(target_arch = "wasm32")]
pub fn parse_args() -> Config {
    Config {
        agent: Agent::Claude,
        model: String::new(),
        auto_mode: false,
        dry_run: true,
        local_inference: Default::default(),
        root: "/".into(),
        project_name: "freq-ai-web".into(),
        scan_targets: Default::default(),
        skill_paths: Default::default(),
        bootstrap_agent_files: false,
        bootstrap_snapshot: false,
        workflow_preset: "default".to_string(),
        use_subscription: false,
        pricing: Default::default(),
        bot_settings: Default::default(),
        bot_credentials: None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_args() -> Config {
    let root = cmd_stdout("git", &["rev-parse", "--show-toplevel"])
        .unwrap_or_else(|| die("not inside a git repository"));

    let dev_cfg = crate::agent::types::load_dev_config(&root);
    configure_log_redaction(&dev_cfg.log_redaction);
    let bot_settings = load_bot_settings(&root, &dev_cfg);
    let bot_credentials = bot_settings.to_credentials();
    let project_name = env::var("DEV_PROJECT_NAME")
        .ok()
        .or(dev_cfg.project_name)
        .unwrap_or_else(|| infer_project_name(&root));
    let mut local_inference = dev_cfg.local_inference.into_local_inference_config();
    if let Some(api_key) = load_local_inference_api_key(&root) {
        local_inference.api_key = api_key;
    }
    let scan_targets = dev_cfg.security_scan.into_scan_targets();
    let skill_paths = dev_cfg.skills.into_skill_paths();
    let bootstrap_agent_files = dev_cfg.bootstrap_agent_files.unwrap_or(true);
    let bootstrap_snapshot = dev_cfg.bootstrap_snapshot.unwrap_or(false);
    let use_subscription = dev_cfg.use_subscription.unwrap_or(false);
    let pricing = dev_cfg.pricing;

    Config {
        agent: Agent::Claude, // Default, will be overridden by CLI
        model: String::new(), // Default, will be overridden after agent is set
        auto_mode: false,     // Default, will be overridden by CLI
        dry_run: false,       // Default, will be overridden by CLI
        local_inference,
        root,
        project_name,
        scan_targets,
        skill_paths,
        bootstrap_agent_files,
        bootstrap_snapshot,
        use_subscription,
        pricing,
        workflow_preset: dev_cfg
            .workflow_preset
            .unwrap_or_else(|| "default".to_string()),
        bot_settings,
        bot_credentials,
    }
}
