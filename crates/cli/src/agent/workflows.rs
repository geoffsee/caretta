use crate::agent::adapter_dispatch::agent_supports_invocation;
use crate::agent::cmd::{die, log};
use crate::agent::issue::preflight;
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::stop_requested;
use crate::agent::run::run_agent;
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER, Workflow};
use crate::agent::workflow::PhaseConfig;

/// Apply per-phase agent and model overrides declared in the workflow YAML.
///
/// Clones `cfg` only when at least one override is present, so the common
/// path (no overrides) pays no allocation.
fn apply_phase_overrides(cfg: &Config, phase: &PhaseConfig) -> Config {
    let mut c = cfg.clone();
    if let Some(agent_str) = &phase.agent {
        match agent_str.parse::<cli_common::Agent>() {
            Ok(agent) => {
                if agent_supports_invocation(agent, "Prompt") {
                    log(&format!(
                        "Phase agent override: switching from {} to {agent_str}",
                        cfg.agent
                    ));
                    c.agent = agent;
                } else {
                    log(&format!(
                        "WARNING: phase agent '{agent_str}' does not support Prompt; \
                         falling back to global agent {}",
                        cfg.agent
                    ));
                }
            }
            Err(_) => {
                log(&format!(
                    "WARNING: unknown agent '{agent_str}' in phase config; \
                     ignoring and using global agent {}",
                    cfg.agent
                ));
            }
        }
    }
    if let Some(model) = &phase.model {
        log(&format!("Phase model override: using '{model}'"));
        c.model = model.clone();
    }
    c
}

/// Inject standard variables that all workflows may need.
fn inject_common_vars(cfg: &Config, vars: &mut serde_json::Value) {
    vars["project_name"] = serde_json::Value::String(cfg.project_name.clone());
    vars["dry_run"] = serde_json::Value::Bool(cfg.dry_run);
    vars["user_personas_skill_path"] =
        serde_json::Value::String(cfg.skill_paths.user_personas.clone());
    vars["issue_tracking_skill_path"] =
        serde_json::Value::String(cfg.skill_paths.issue_tracking.clone());
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

    let mut vars = gather_context_as_json(cfg, &wf.context);
    inject_common_vars(cfg, &mut vars);
    fetch_extra_context(wf, &mut vars);

    let prompt = load_and_render(&cfg.root, &cfg.workflow_preset, wf, "draft", &vars)
        .unwrap_or_else(|e| die(&format!("Prompt render failed: {e}")));

    let effective = apply_phase_overrides(cfg, phase_cfg);

    if effective.dry_run {
        log_resolved_agent_launch(&effective, &[]);
        log(&format!("[dry-run] Would run {} draft", wf.name));
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    run_agent(&effective, &prompt);
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
    if effective.auto_mode && EVENT_SENDER.get().is_none() && has_finalize {
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

    let mut vars = gather_context_as_json(cfg, &wf.context);
    inject_common_vars(cfg, &mut vars);
    fetch_extra_context(wf, &mut vars);
    vars["feedback"] = serde_json::Value::String(feedback.to_string());

    let prompt = load_and_render(&cfg.root, &cfg.workflow_preset, wf, "finalize", &vars)
        .unwrap_or_else(|e| die(&format!("Prompt render failed: {e}")));

    let effective = apply_phase_overrides(cfg, phase_cfg);
    run_agent(&effective, &prompt);
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
