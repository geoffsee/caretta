use crate::agent::cmd::log;
use crate::agent::issue::preflight;
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::stop_requested;
use crate::agent::run::run_agent;
use crate::agent::tracker::{
    build_interview_draft_prompt, build_interview_followup_prompt, build_interview_summary_prompt,
};
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER, Workflow};
use crate::agent::workflows::gather_strategic_context_base;
use std::sync::Mutex;

const INTERVIEW_MAX_FOLLOWUP_ROUNDS: usize = 1;
static INTERVIEW_ANSWERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn run_interview_draft(cfg: &Config) {
    preflight(cfg);
    log("Starting interview — analyzing project state...");

    // Reset interview state.
    if let Ok(mut answers) = INTERVIEW_ANSWERS.lock() {
        answers.clear();
    }

    let (open_issues, open_prs, recent_commits, crate_tree) = gather_strategic_context_base(cfg);

    let prompt =
        build_interview_draft_prompt(&open_issues, &open_prs, &recent_commits, &crate_tree);

    if cfg.dry_run {
        log_resolved_agent_launch(cfg, &[]);
        log("[dry-run] Would run interview draft");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    run_agent(cfg, &prompt);
    if stop_requested() {
        log("Stop requested. Interview cancelled.");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    log("Review the questions above and provide your answers.");
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::AwaitingFeedback(Workflow::Interview));
    }
}

pub fn run_interview_respond(cfg: &Config, answer: &str) {
    preflight(cfg);

    // Accumulate the answer.
    let answers = {
        let mut guard = INTERVIEW_ANSWERS.lock().unwrap();
        guard.push(answer.to_string());
        guard.clone()
    };

    let round = answers.len(); // 1-indexed (1 = first follow-up, etc.)

    let (open_issues, open_prs, recent_commits, crate_tree) = gather_strategic_context_base(cfg);

    if round <= INTERVIEW_MAX_FOLLOWUP_ROUNDS {
        // Follow-up round.
        log(&format!(
            "Processing answer (round {round}) — generating follow-up questions..."
        ));

        let prompt = build_interview_followup_prompt(
            &open_issues,
            &open_prs,
            &recent_commits,
            &crate_tree,
            &answers,
        );

        if cfg.dry_run {
            log_resolved_agent_launch(cfg, &[]);
            log(&format!("[dry-run] Would run interview round {round}"));
            if let Some(tx) = EVENT_SENDER.get() {
                let _ = tx.send(AgentEvent::Done);
            }
            return;
        }

        run_agent(cfg, &prompt);
        if stop_requested() {
            log("Stop requested. Interview cancelled.");
            if let Some(tx) = EVENT_SENDER.get() {
                let _ = tx.send(AgentEvent::Done);
            }
            return;
        }

        log("Review the follow-up questions and provide your answers.");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::AwaitingFeedback(Workflow::Interview));
        }
    } else {
        // Summary round.
        log("Generating interview summary...");

        let prompt = build_interview_summary_prompt(
            &open_issues,
            &open_prs,
            &recent_commits,
            &crate_tree,
            &answers,
        );

        if cfg.dry_run {
            log_resolved_agent_launch(cfg, &[]);
            log("[dry-run] Would run interview summary");
            if let Some(tx) = EVENT_SENDER.get() {
                let _ = tx.send(AgentEvent::Done);
            }
            return;
        }

        run_agent(cfg, &prompt);
        if stop_requested() {
            log("Stop requested. Interview cancelled.");
            if let Some(tx) = EVENT_SENDER.get() {
                let _ = tx.send(AgentEvent::Done);
            }
            return;
        }

        log("Strategic interview complete.");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interview_answers_accumulate() {
        if let Ok(mut answers) = INTERVIEW_ANSWERS.lock() {
            answers.clear();
        }
        {
            let mut guard = INTERVIEW_ANSWERS.lock().unwrap();
            guard.push("first".to_string());
        }
        let current = INTERVIEW_ANSWERS.lock().unwrap().clone();
        assert_eq!(current, vec!["first".to_string()]);
    }
}
