// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::cmd::log;
use crate::agent::launch::log_resolved_agent_launch;
use crate::agent::process::stop_requested;
use crate::agent::run::run_agent;
use crate::agent::types::{AgentEvent, Config, EVENT_SENDER, Workflow};
use std::sync::Mutex;

/// A single turn in the chat conversation.
#[derive(Clone, Debug)]
struct ChatTurn {
    is_user: bool,
    content: String,
}

static CHAT_HISTORY: Mutex<Vec<ChatTurn>> = Mutex::new(Vec::new());

/// Reset the chat conversation history.
pub fn reset_chat_history() {
    if let Ok(mut history) = CHAT_HISTORY.lock() {
        history.clear();
    }
}

/// Build the agent prompt from accumulated conversation history.
fn build_chat_prompt(project_name: &str, history: &[ChatTurn], new_message: &str) -> String {
    let mut prompt = format!(
        "You are a helpful assistant for the \"{project_name}\" project. \
         The user is chatting with you freely — there is no specific workflow or task. \
         Answer questions, discuss ideas, explain code, suggest improvements, \
         or help with whatever the user needs. Be concise and direct.\n\n"
    );

    for turn in history {
        if turn.is_user {
            prompt.push_str(&format!("User: {}\n\n", turn.content));
        } else {
            prompt.push_str(&format!("Assistant: {}\n\n", turn.content));
        }
    }

    prompt.push_str(&format!("User: {new_message}\n\nAssistant:"));
    prompt
}

/// Send a message in chat mode. Accumulates history and emits
/// `AwaitingFeedback(Workflow::Chat)` after each agent response so the
/// user can continue the conversation.
pub fn run_chat_send(cfg: &Config, message: &str) {
    // Record the user message.
    if let Ok(mut history) = CHAT_HISTORY.lock() {
        history.push(ChatTurn {
            is_user: true,
            content: message.to_string(),
        });
    }

    let history_snapshot = CHAT_HISTORY.lock().map(|h| h.clone()).unwrap_or_default();
    let prompt = build_chat_prompt(
        &cfg.project_name,
        &history_snapshot[..history_snapshot.len() - 1],
        message,
    );

    if cfg.dry_run {
        log_resolved_agent_launch(cfg, &[]);
        log("[dry-run] Would send chat message");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    run_agent(cfg, &prompt);

    if stop_requested() {
        log("Stop requested. Chat cancelled.");
        if let Some(tx) = EVENT_SENDER.get() {
            let _ = tx.send(AgentEvent::Done);
        }
        return;
    }

    // Ready for the next user message.
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::AwaitingFeedback(Workflow::Chat));
    }
}

/// Record the agent's response in the conversation history so subsequent
/// turns include it in the prompt.
pub fn record_agent_response(text: &str) {
    if let Ok(mut history) = CHAT_HISTORY.lock() {
        history.push(ChatTurn {
            is_user: false,
            content: text.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_history_accumulates() {
        reset_chat_history();
        {
            let mut guard = CHAT_HISTORY.lock().unwrap();
            guard.push(ChatTurn {
                is_user: true,
                content: "hello".into(),
            });
            guard.push(ChatTurn {
                is_user: false,
                content: "hi there".into(),
            });
        }
        let current = CHAT_HISTORY.lock().unwrap().clone();
        assert_eq!(current.len(), 2);
        assert!(current[0].is_user);
        assert!(!current[1].is_user);
        reset_chat_history();
    }

    #[test]
    fn build_chat_prompt_includes_history() {
        let history = vec![
            ChatTurn {
                is_user: true,
                content: "What is this project?".into(),
            },
            ChatTurn {
                is_user: false,
                content: "It is a dev agent framework.".into(),
            },
        ];
        let prompt = build_chat_prompt("my-project", &history, "Tell me more");
        assert!(prompt.contains("my-project"));
        assert!(prompt.contains("What is this project?"));
        assert!(prompt.contains("It is a dev agent framework."));
        assert!(prompt.contains("Tell me more"));
    }
}
