use std::sync::OnceLock;
use tokio::sync::mpsc;

pub use cli_common::*;

pub static EVENT_SENDER: OnceLock<mpsc::UnboundedSender<AgentEvent>> = OnceLock::new();

pub trait AgentExt {
    fn available_models(self) -> &'static [(&'static str, &'static str)];
}

impl AgentExt for Agent {
    /// Models available for selection in the UI dropdown.
    /// Each entry is (model_id, display_label). Empty slice means the agent
    /// has no model flag and the dropdown should be hidden.
    ///
    /// Populated from `assets/available-models.json` (generated while building
    /// Rebuild that crate so its build script rescans embedded provider bundles in
    /// `crates/agent-runtime/node_modules` and refreshes this file.
    /// CLI: `caretta --agent … models` lists the same catalog.
    fn available_models(self) -> &'static [(&'static str, &'static str)] {
        use std::collections::HashMap;
        use std::sync::OnceLock;

        type ModelMap = HashMap<&'static str, &'static [(&'static str, &'static str)]>;

        static MODELS: OnceLock<ModelMap> = OnceLock::new();

        let map = MODELS.get_or_init(|| {
            let raw: HashMap<String, Vec<(String, String)>> =
                serde_json::from_str(super::assets::AVAILABLE_MODELS_JSON)
                    .expect("assets/available-models.json is invalid");

            raw.into_iter()
                .map(|(agent, pairs)| {
                    let key: &'static str = Box::leak(agent.into_boxed_str());
                    let slice: &'static [(&'static str, &'static str)] = Box::leak(
                        pairs
                            .into_iter()
                            .map(|(id, label)| -> (&'static str, &'static str) {
                                (
                                    Box::leak(id.into_boxed_str()),
                                    Box::leak(label.into_boxed_str()),
                                )
                            })
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    );
                    (key, slice)
                })
                .collect()
        });

        let key = match self {
            Agent::Claude => "claude",
            Agent::Cline => "cline",
            Agent::Codex => "codex",
            Agent::Copilot => "copilot",
            Agent::Gemini => "gemini",
            Agent::Grok => "grok",
            Agent::Junie => "junie",
            Agent::Xai => "xai",
            Agent::Cursor => "cursor",
        };

        map.get(key).copied().unwrap_or(&[])
    }
}

/// Load `caretta.toml` from the project root. Returns defaults if the file is absent or malformed.
/// Falls back to the legacy `dev.toml` filename when `caretta.toml` is missing,
/// so existing installs keep working until the next save rewrites the file.
pub fn load_dev_config(root: &str) -> DevConfig {
    let root = std::path::Path::new(root);
    for name in ["caretta.toml", "dev.toml"] {
        if let Ok(contents) = std::fs::read_to_string(root.join(name)) {
            return toml::from_str(&contents).unwrap_or_default();
        }
    }
    DevConfig::default()
}

pub fn save_dev_config(root: &str, cfg: &Config) -> Result<(), String> {
    let path = std::path::Path::new(root).join("caretta.toml");
    let existing = load_dev_config(root);

    // Merge current model selection into the persisted per-agent map.
    let mut agent_models = existing.agent_models;
    if cfg.model.trim().is_empty() {
        agent_models.remove(&cfg.agent.to_string());
    } else {
        agent_models.insert(cfg.agent.to_string(), cfg.model.clone());
    }

    let mut local_inference = LocalInferenceConfigFile {
        advanced: Some(cfg.local_inference.advanced),
        preset: Some(cfg.local_inference.preset),
        base_url: Some(cfg.local_inference.base_url.clone()),
        model: Some(cfg.local_inference.model.clone()),
        api_key: None,
    };
    if cfg.local_inference.base_url.trim().is_empty() {
        local_inference.base_url = None;
    }
    if cfg.local_inference.model.trim().is_empty() {
        local_inference.model = None;
    }

    let bot = BotSettingsFile {
        mode: Some(cfg.bot_settings.mode),
        app_id: (!cfg.bot_settings.app_id.trim().is_empty())
            .then(|| cfg.bot_settings.app_id.clone()),
        installation_id: (!cfg.bot_settings.installation_id.trim().is_empty())
            .then(|| cfg.bot_settings.installation_id.clone()),
    };

    let file_cfg = DevConfig {
        project_name: Some(cfg.project_name.clone()),
        local_inference,
        security_scan: ScanTargetsFile {
            paths: cfg.scan_targets.paths.clone(),
        },
        skills: SkillPathsFile {
            user_personas: Some(cfg.skill_paths.user_personas.clone()),
            issue_tracking: Some(cfg.skill_paths.issue_tracking.clone()),
        },
        bootstrap_agent_files: Some(cfg.bootstrap_agent_files),
        workflow_preset: if cfg.workflow_preset == "default" {
            None
        } else {
            Some(cfg.workflow_preset.clone())
        },
        bot,
        bootstrap_snapshot: Some(cfg.bootstrap_snapshot),
        use_subscription: Some(cfg.use_subscription),
        pricing: cfg.pricing.clone(),
        log_redaction: existing.log_redaction,
        agent_models,
        test: cfg.test.clone(),
        event_log_path: cfg.event_log_path.clone(),
        path_constraints: cfg.path_constraints.clone(),
    };

    let toml = toml::to_string_pretty(&file_cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, toml).map_err(|e| e.to_string())
}

pub const BRANCH_PREFIX: &str = "agent/issue-";
pub const MAX_COMMIT_ATTEMPTS: u32 = 3;
pub const MAX_PUSH_ATTEMPTS: u32 = 3;
