use crate::agent::types::Config;

// Re-exports from decomposed modules.
pub use crate::agent::bot::{load_bot_credentials_from_env, load_bot_settings, resolve_bot_token};
pub use crate::agent::chat::{record_agent_response, reset_chat_history, run_chat_send};
pub use crate::agent::cli::{infer_project_name, parse_args};
pub use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_run_in, cmd_stdout, cmd_stdout_or_die, count_tokens, die,
    has_command, list_all_files, log,
};
pub use crate::agent::interview::{run_interview_draft, run_interview_respond};
pub use crate::agent::issue::{
    commit_with_retries, preflight, run_loop, run_single_issue, run_tracker_matrix, work_on_issue,
};
pub use crate::agent::launch::log_resolved_agent_launch;
pub use crate::agent::process::{
    active_child_pid, clear_stop_request, request_stop, stop_requested,
};
pub use crate::agent::refresh::{run_refresh_agents, run_refresh_docs};
pub use crate::agent::review::{
    run_code_review, run_pr_review_fix, run_security_code_review, try_approve_pr,
};
pub use crate::agent::run::{run_agent, run_agent_with_env};
pub use crate::agent::snapshot::generate_codebase_snapshot;
pub use crate::agent::workflows::{
    run_retrospective_draft, run_retrospective_finalize, run_sprint_planning_draft,
    run_sprint_planning_finalize, run_workflow_draft, run_workflow_finalize,
};

// ── Action wrappers for the registry ─────────────────────────────────────

pub fn action_code_review(
    cfg: &Config,
    _ctx: &mut crate::agent::actions::ActionContext,
) -> Result<(), String> {
    run_code_review(cfg, None);
    Ok(())
}

pub fn action_security_code_review(
    cfg: &Config,
    _ctx: &mut crate::agent::actions::ActionContext,
) -> Result<(), String> {
    run_security_code_review(cfg);
    Ok(())
}

pub fn action_refresh_agents(
    cfg: &Config,
    _ctx: &mut crate::agent::actions::ActionContext,
) -> Result<(), String> {
    run_refresh_agents(cfg);
    Ok(())
}

pub fn action_refresh_docs(
    cfg: &Config,
    _ctx: &mut crate::agent::actions::ActionContext,
) -> Result<(), String> {
    run_refresh_docs(cfg);
    Ok(())
}

pub fn action_auto_merge(
    cfg: &Config,
    _ctx: &mut crate::agent::actions::ActionContext,
) -> Result<(), String> {
    crate::agent::auto_merge::run_auto_merge_stack(cfg, None);
    Ok(())
}
