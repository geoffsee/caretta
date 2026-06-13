use crate::agent::adapter_dispatch;
use crate::agent::cmd::log;
use crate::agent::types::{AgentLaunchOverrides, Config};

pub fn local_inference_overrides(cfg: &Config) -> AgentLaunchOverrides {
    let local = &cfg.local_inference;
    if !local.advanced {
        return AgentLaunchOverrides::default();
    }

    let base_url = local.base_url.trim();
    if base_url.is_empty() {
        return AgentLaunchOverrides::default();
    }

    let (args, env) = adapter_dispatch::launch_local_inference(
        cfg.agent,
        base_url,
        local.api_key.trim(),
        local.model.trim(),
    );
    AgentLaunchOverrides { args, env }
}

/// Generate CLI arguments and/or env vars to pass the user's model selection
/// to the agent subprocess. Returns empty overrides when the model is "Default"
/// (empty string) or when local_inference already provides a model override.
pub fn model_selection_overrides(cfg: &Config) -> AgentLaunchOverrides {
    // Local inference model takes priority.
    if cfg.local_inference.advanced
        && !cfg.local_inference.base_url.trim().is_empty()
        && !cfg.local_inference.model.trim().is_empty()
    {
        return AgentLaunchOverrides::default();
    }

    let model = cfg.model.trim();
    if model.is_empty() {
        return AgentLaunchOverrides::default();
    }

    let (args, env) = adapter_dispatch::launch_model_selection(cfg.agent, model);
    AgentLaunchOverrides { args, env }
}

/// Generate CLI arguments to skip permission prompts when `auto_mode` is enabled.
/// Returns empty overrides when `auto_mode` is false or the agent has no such flag.
pub fn auto_mode_overrides(cfg: &Config) -> AgentLaunchOverrides {
    if !cfg.auto_mode {
        return AgentLaunchOverrides::default();
    }

    AgentLaunchOverrides {
        args: adapter_dispatch::launch_auto_mode(cfg.agent),
        env: Vec::new(),
    }
}

pub fn merged_agent_env(cfg: &Config, extra_env: &[(String, String)]) -> Vec<(String, String)> {
    let mut env = local_inference_overrides(cfg).env;
    env.extend(model_selection_overrides(cfg).env);
    env.extend(adapter_dispatch::launch_env(cfg.agent));
    env.extend(extra_env.iter().cloned());

    // #11: Ensure `gh` never uses a pager (e.g. `less`) when the agent
    // invokes it to fetch context or submit PRs.
    env.push(("GH_PAGER".to_string(), "cat".to_string()));

    if cfg.use_subscription {
        env.push(("ANTHROPIC_API_KEY".to_string(), "".to_string()));
        env.push(("OPENAI_API_KEY".to_string(), "".to_string()));
        env.push(("GEMINI_API_KEY".to_string(), "".to_string()));
        env.push(("GROK_API_KEY".to_string(), "".to_string()));
        env.push(("JUNIE_API_KEY".to_string(), "".to_string()));
        env.push(("XAI_API_KEY".to_string(), "".to_string()));
    }

    env
}

fn redact_launch_env_value(key: &str, value: &str) -> String {
    if key.ends_with("API_KEY") && !value.is_empty() && value != "local" {
        "<redacted>".to_string()
    } else {
        value.to_string()
    }
}

pub fn log_resolved_agent_launch(cfg: &Config, extra_env: &[(String, String)]) {
    let mut overrides = local_inference_overrides(cfg);
    let model_ov = model_selection_overrides(cfg);
    overrides.args.extend(model_ov.args);
    let auto_ov = auto_mode_overrides(cfg);
    overrides.args.extend(auto_ov.args);
    let env = merged_agent_env(cfg, extra_env);
    let args = if overrides.args.is_empty() {
        "(none)".to_string()
    } else {
        overrides.args.join(" ")
    };
    let env = if env.is_empty() {
        "(none)".to_string()
    } else {
        env.iter()
            .map(|(key, value)| format!("{key}={}", redact_launch_env_value(key, value)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    log(&format!(
        "[dry-run] Agent launch overrides for {} -> args: {args}; env: {env}",
        cfg.agent
    ));
}
