use crate::agent::cmd::{die, log};
use crate::agent::event_log::{
    AgentRunRecord, append_run, extract_run_data, iso8601_now, resolve_db_path,
};
use crate::agent::issue::preflight;
use crate::agent::launch::log_resolved_agent_launch;
// start_run_capture / drain_run_capture are provided by #70 (event-capture infrastructure).
use crate::agent::process::{drain_run_capture, start_run_capture, stop_requested};
use crate::agent::run::run_agent;
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER, Workflow};
use cli_common::PathConstraints;
use std::time::Instant;

/// Inject standard variables that all workflows may need.
fn inject_common_vars(cfg: &Config, vars: &mut serde_json::Value) {
    vars["project_name"] = serde_json::Value::String(cfg.project_name.clone());
    vars["dry_run"] = serde_json::Value::Bool(cfg.dry_run);
    vars["user_personas_skill_path"] =
        serde_json::Value::String(cfg.skill_paths.user_personas.clone());
    vars["issue_tracking_skill_path"] =
        serde_json::Value::String(cfg.skill_paths.issue_tracking.clone());
}

/// Apply per-workflow path constraints when declared in workflow.yaml, returning
/// either the modified config (stored in `storage`) or the original `cfg`.
fn apply_workflow_path_constraints<'a>(
    cfg: &'a Config,
    storage: &'a mut Option<Config>,
    wf_constraints: Option<&PathConstraints>,
) -> &'a Config {
    if let Some(c) = wf_constraints {
        *storage = Some(Config {
            path_constraints: c.clone(),
            ..cfg.clone()
        });
        storage.as_ref().unwrap()
    } else {
        cfg
    }
}

fn record_workflow_run(
    cfg: &Config,
    workflow_phase: &str,
    captured: Vec<AgentEvent>,
    started_at: String,
    finished_at: String,
    duration_ms: u64,
) {
    let (tool_calls, input_tokens, output_tokens, run_status, event_model) =
        extract_run_data(&captured);
    let effective_model = event_model.unwrap_or_else(|| cfg.model.clone());

    #[cfg(not(target_arch = "wasm32"))]
    let policy_violations =
        crate::agent::path_constraint::check_run(&tool_calls, &cfg.path_constraints);
    #[cfg(target_arch = "wasm32")]
    let policy_violations: Vec<crate::agent::event_log::PolicyViolation> = vec![];

    if !policy_violations.is_empty() {
        log(&format!(
            "Path-constraint policy: {} violation(s) detected for workflow phase '{workflow_phase}'",
            policy_violations.len()
        ));
        for v in &policy_violations {
            log(&format!(
                "  POLICY VIOLATION: tool={} path={} reason={}",
                v.tool, v.path, v.reason
            ));
        }
    }
    let db_path = resolve_db_path(cfg.event_log_path.as_deref());
    append_run(
        &AgentRunRecord {
            agent_id: cfg.agent.to_string(),
            model: effective_model,
            workflow_phase: workflow_phase.to_string(),
            issue_number: None,
            tracker_number: None,
            tool_calls,
            input_tokens,
            output_tokens,
            status: run_status,
            started_at,
            finished_at,
            duration_ms,
            path_constraints: cfg.path_constraints.clone(),
            policy_violations,
            preset_name: None,
            preset_version: None,
        },
        &db_path,
    );
}

/// Run the draft phase of any two-phase workflow loaded from YAML.
pub fn run_workflow_draft(cfg: &Config, workflow_id: &str) {
    use crate::agent::workflow::{
        fetch_extra_context, gather_context_as_json, load_and_render, load_workflows,
    };

    let workflows = load_workflows(&cfg.root, &cfg.workflow_preset);
    let wf = workflows.get(workflow_id).unwrap_or_else(|| {
        die(&format!("Unknown workflow: {workflow_id}"));
    });
    let phase_cfg = wf.phases.get("draft").unwrap_or_else(|| {
        die(&format!("No draft phase in workflow '{workflow_id}'"));
    });

    preflight(cfg);
    log(&phase_cfg.log_start);

    let mut effective_cfg_storage: Option<Config> = None;
    let cfg = apply_workflow_path_constraints(cfg, &mut effective_cfg_storage, wf.path_constraints.as_ref());

    let mut vars = gather_context_as_json(cfg, &wf.context);
    inject_common_vars(cfg, &mut vars);
    fetch_extra_context(wf, &mut vars);

    let prompt = load_and_render(&cfg.root, &cfg.workflow_preset, wf, "draft", &vars)
        .unwrap_or_else(|e| die(&format!("Prompt render failed: {e}")));

    if cfg.dry_run {
        log_resolved_agent_launch(cfg, &[]);
        log(&format!("[dry-run] Would run {} draft", wf.name));
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    let run_started_at = iso8601_now();
    let run_wall_clock = Instant::now();
    start_run_capture();
    run_agent(cfg, &prompt);
    let run_duration_ms = run_wall_clock.elapsed().as_millis() as u64;
    let run_finished_at = iso8601_now();
    let captured = drain_run_capture();
    record_workflow_run(
        cfg,
        &format!("{workflow_id}/draft"),
        captured,
        run_started_at,
        run_finished_at,
        run_duration_ms,
    );

    if stop_requested() {
        log(&format!("Stop requested. {} draft cancelled.", wf.name));
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    log(&phase_cfg.log_complete);

    // With `--auto`, a CLI run with no human in the loop synthesizes a stand-in
    // reviewer message and chains straight into finalize. Without `--auto`, the
    // CLI stops at the draft so the user can inspect it before any side effects
    // fire (finalize phases routinely create/close GitHub issues). The GUI path
    // keeps its existing two-step flow because EVENT_SENDER is set there.
    let has_finalize = wf.phases.contains_key("finalize");
    if cfg.auto_mode && EVENT_SENDER.get().is_none() && has_finalize {
        let feedback = synthesized_cli_feedback();
        log("--auto: synthesizing feedback and continuing to finalize.");
        run_workflow_finalize(cfg, workflow_id, &feedback);
        return;
    }

    if let Some(wf_enum) = Workflow::from_id(workflow_id) {
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::AwaitingFeedback(wf_enum));
        }
    } else if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::Done);
    }
}

/// Stand-in reviewer note used when a two-phase workflow is run from the CLI
/// without an interactive human. Kept intentionally short and prescriptive so
/// finalize prompts behave deterministically.
fn synthesized_cli_feedback() -> String {
    "(Autogenerated — no human reviewer available for this run.)\n\
     \n\
     Treat the draft as fully endorsed. Carry every proposal forward as-is. \
     Do not invent new constraints, do not solicit further input, and do not \
     drop items for being uncertain. When the draft leaves a choice open, pick \
     the simplest interpretation and continue."
        .to_string()
}

/// Run the finalize phase of any two-phase workflow loaded from YAML.
pub fn run_workflow_finalize(cfg: &Config, workflow_id: &str, feedback: &str) {
    use crate::agent::workflow::{
        fetch_extra_context, gather_context_as_json, load_and_render, load_workflows,
    };

    let workflows = load_workflows(&cfg.root, &cfg.workflow_preset);
    let wf = workflows.get(workflow_id).unwrap_or_else(|| {
        die(&format!("Unknown workflow: {workflow_id}"));
    });
    let phase_cfg = wf.phases.get("finalize").unwrap_or_else(|| {
        die(&format!("No finalize phase in workflow '{workflow_id}'"));
    });

    preflight(cfg);
    log(&phase_cfg.log_start);

    let mut effective_cfg_storage: Option<Config> = None;
    let cfg = apply_workflow_path_constraints(cfg, &mut effective_cfg_storage, wf.path_constraints.as_ref());

    let mut vars = gather_context_as_json(cfg, &wf.context);
    inject_common_vars(cfg, &mut vars);
    fetch_extra_context(wf, &mut vars);
    vars["feedback"] = serde_json::Value::String(feedback.to_string());

    let prompt = load_and_render(&cfg.root, &cfg.workflow_preset, wf, "finalize", &vars)
        .unwrap_or_else(|e| die(&format!("Prompt render failed: {e}")));

    let run_started_at = iso8601_now();
    let run_wall_clock = Instant::now();
    start_run_capture();
    run_agent(cfg, &prompt);
    let run_duration_ms = run_wall_clock.elapsed().as_millis() as u64;
    let run_finished_at = iso8601_now();
    let captured = drain_run_capture();
    record_workflow_run(
        cfg,
        &format!("{workflow_id}/finalize"),
        captured,
        run_started_at,
        run_finished_at,
        run_duration_ms,
    );

    if stop_requested() {
        log(&format!(
            "Stop requested. {} finalization cancelled.",
            wf.name
        ));
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    log(&phase_cfg.log_complete);
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::Done);
    }
}

pub fn run_sprint_planning_draft(cfg: &Config) {
    run_workflow_draft(cfg, "sprint_planning");
}

pub fn run_sprint_planning_finalize(cfg: &Config, feedback: &str) {
    run_workflow_finalize(cfg, "sprint_planning", feedback);
}

pub fn run_retrospective_draft(cfg: &Config) {
    run_workflow_draft(cfg, "retrospective");
}

pub fn run_retrospective_finalize(cfg: &Config, feedback: &str) {
    run_workflow_finalize(cfg, "retrospective", feedback);
}

pub fn gather_strategic_context_base(
    cfg: &Config,
) -> (String, String, String, String, String, String) {
    let ctx = crate::agent::workflow::gather_context_as_json(cfg, "strategic");
    (
        ctx["open_issues"].as_str().unwrap_or("[]").to_string(),
        ctx["open_prs"].as_str().unwrap_or("[]").to_string(),
        ctx["recent_commits"].as_str().unwrap_or("[]").to_string(),
        ctx["active_review_threads"]
            .as_str()
            .unwrap_or("[]")
            .to_string(),
        ctx["snapshot"].as_str().unwrap_or("").to_string(),
        ctx["project_status"].as_str().unwrap_or("").to_string(),
    )
}
