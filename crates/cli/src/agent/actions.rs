use std::collections::HashMap;
use std::sync::OnceLock;

use crate::agent::types::Config;

/// Mutable context bag passed to registered action functions.
pub struct ActionContext {
    pub workflow_id: String,
    pub vars: serde_json::Value,
}

impl ActionContext {
    pub fn new(workflow_id: &str) -> Self {
        Self {
            workflow_id: workflow_id.to_string(),
            vars: serde_json::json!({}),
        }
    }
}

/// Signature for registered workflow action functions.
pub type ActionFn = fn(&Config, &mut ActionContext) -> Result<(), String>;

static REGISTRY: OnceLock<HashMap<&'static str, ActionFn>> = OnceLock::new();

fn action_registry() -> &'static HashMap<&'static str, ActionFn> {
    REGISTRY.get_or_init(|| {
        let mut m: HashMap<&'static str, ActionFn> = HashMap::new();
        m.insert("code_review", super::shell::action_code_review);
        m.insert(
            "security_code_review",
            super::shell::action_security_code_review,
        );
        m.insert("refresh_agents", super::shell::action_refresh_agents);
        m.insert("refresh_docs", super::shell::action_refresh_docs);
        m.insert("auto_merge", super::shell::action_auto_merge);
        m.insert("visual_regression", super::shell::action_visual_regression);
        m.insert("deploy", super::shell::action_deploy);
        m
    })
}

/// Look up a runner by name. Returns `None` for workflows that should use
/// the generic `run_workflow_draft` / `run_workflow_finalize` path.
pub fn lookup_action(name: &str) -> Option<&'static ActionFn> {
    action_registry().get(name)
}
