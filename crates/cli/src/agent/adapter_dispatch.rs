// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Dispatches [`cli_common::Agent`] to provider [`agent_common::AgentCliAdapter`] implementations.
//! All binary names and flag spellings for subprocess construction live in the provider crates.

use agent_common::{AgentCliAdapter, AgentCliCommand, PromptTransport};
use claude::{ClaudeWrapper, CursorWrapper};
use cli_common::Agent;
use cline::ClineWrapper;
use codex::CodexWrapper;
use copilot::CopilotWrapper;
use gemini::GeminiWrapper;
use junie::JunieWrapper;
use xai::XaiWrapper;

pub const PROMPT_STDIN_BYTE_THRESHOLD: usize = 64 * 1024;

pub struct NativeRunCommand {
    pub command: AgentCliCommand,
    pub prompt_transport: PromptTransport,
}

pub fn native_base_command(agent: Agent, prompt: &str) -> AgentCliCommand {
    match agent {
        Agent::Claude => AgentCliCommand {
            binary: ClaudeWrapper.binary().to_string(),
            args: ClaudeWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Cursor => AgentCliCommand {
            binary: CursorWrapper.binary().to_string(),
            args: CursorWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Junie => AgentCliCommand {
            binary: JunieWrapper.binary().to_string(),
            args: JunieWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Copilot => AgentCliCommand {
            binary: CopilotWrapper.binary().to_string(),
            args: CopilotWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Codex => AgentCliCommand {
            binary: CodexWrapper.binary().to_string(),
            args: CodexWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Gemini => AgentCliCommand {
            binary: GeminiWrapper.binary().to_string(),
            args: GeminiWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Xai => AgentCliCommand {
            binary: XaiWrapper.binary().to_string(),
            args: XaiWrapper.caretta_native_run_argv(prompt),
        },
        Agent::Cline => AgentCliCommand {
            binary: ClineWrapper.binary().to_string(),
            args: ClineWrapper.caretta_native_run_argv(prompt),
        },
    }
}

pub fn caretta_native_command(
    agent: Agent,
    prompt: &str,
    extra_args: &[String],
) -> AgentCliCommand {
    let mut cmd = native_base_command(agent, prompt);
    cmd.args.extend_from_slice(extra_args);
    cmd
}

pub fn caretta_native_command_with_prompt_transport(
    agent: Agent,
    prompt: &str,
    extra_args: &[String],
) -> NativeRunCommand {
    let adapter = adapter_for_agent(agent);
    let transport_pref = adapter.prompt_transport();
    let use_stdin =
        prompt.len() > PROMPT_STDIN_BYTE_THRESHOLD || transport_pref == PromptTransport::Stdin;

    let mut command = if use_stdin {
        native_stdin_command(agent).unwrap_or_else(|| native_base_command(agent, prompt))
    } else {
        native_base_command(agent, prompt)
    };
    command.args.extend_from_slice(extra_args);

    NativeRunCommand {
        command,
        prompt_transport: if use_stdin && supports_stdin_prompt(agent) {
            PromptTransport::Stdin
        } else {
            PromptTransport::Argv
        },
    }
}

pub fn adapter_for_agent(agent: Agent) -> &'static dyn AgentCliAdapter {
    match agent {
        Agent::Claude => &ClaudeWrapper,
        Agent::Cursor => &CursorWrapper,
        Agent::Junie => &JunieWrapper,
        Agent::Copilot => &CopilotWrapper,
        Agent::Codex => &CodexWrapper,
        Agent::Gemini => &GeminiWrapper,
        Agent::Xai => &XaiWrapper,
        Agent::Cline => &ClineWrapper,
    }
}

fn supports_stdin_prompt(agent: Agent) -> bool {
    matches!(
        agent,
        Agent::Claude | Agent::Cursor | Agent::Junie | Agent::Codex
    )
}

fn native_stdin_command(agent: Agent) -> Option<AgentCliCommand> {
    match agent {
        Agent::Claude => Some(AgentCliCommand {
            binary: ClaudeWrapper.binary().to_string(),
            args: claude_family_native_stdin_argv(),
        }),
        Agent::Cursor => Some(AgentCliCommand {
            binary: CursorWrapper.binary().to_string(),
            args: claude_family_native_stdin_argv(),
        }),
        Agent::Junie => Some(AgentCliCommand {
            binary: JunieWrapper.binary().to_string(),
            args: claude_family_native_stdin_argv(),
        }),
        Agent::Codex => Some(AgentCliCommand {
            binary: CodexWrapper.binary().to_string(),
            args: vec!["exec".to_string(), "--json".to_string(), "-".to_string()],
        }),
        _ => None,
    }
}

fn claude_family_native_stdin_argv() -> Vec<String> {
    vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ]
}

pub fn launch_model_selection(agent: Agent, model: &str) -> (Vec<String>, Vec<(String, String)>) {
    match agent {
        Agent::Claude => ClaudeWrapper.launch_model_selection(model),
        Agent::Cursor => CursorWrapper.launch_model_selection(model),
        Agent::Junie => JunieWrapper.launch_model_selection(model),
        Agent::Copilot => CopilotWrapper.launch_model_selection(model),
        Agent::Codex => CodexWrapper.launch_model_selection(model),
        Agent::Gemini => GeminiWrapper.launch_model_selection(model),
        Agent::Xai => XaiWrapper.launch_model_selection(model),
        Agent::Cline => ClineWrapper.launch_model_selection(model),
    }
}

pub fn launch_auto_mode(agent: Agent) -> Vec<String> {
    match agent {
        Agent::Claude => ClaudeWrapper.launch_auto_mode(),
        Agent::Cursor => CursorWrapper.launch_auto_mode(),
        Agent::Junie => JunieWrapper.launch_auto_mode(),
        Agent::Copilot => CopilotWrapper.launch_auto_mode(),
        Agent::Codex => CodexWrapper.launch_auto_mode(),
        Agent::Gemini => GeminiWrapper.launch_auto_mode(),
        Agent::Xai => XaiWrapper.launch_auto_mode(),
        Agent::Cline => ClineWrapper.launch_auto_mode(),
    }
}

pub fn launch_env(agent: Agent) -> Vec<(String, String)> {
    adapter_for_agent(agent).launch_env()
}

pub fn launch_local_inference(
    agent: Agent,
    base_url: &str,
    api_key: &str,
    local_model: &str,
) -> (Vec<String>, Vec<(String, String)>) {
    match agent {
        Agent::Claude => ClaudeWrapper.launch_local_inference(base_url, api_key, local_model),
        Agent::Cursor => CursorWrapper.launch_local_inference(base_url, api_key, local_model),
        Agent::Codex => CodexWrapper.launch_local_inference(base_url, api_key, local_model),
        _ => (Vec::new(), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_common::claude_family_native_argv;
    use cli_common::Agent;

    #[test]
    fn native_base_matches_claude_family_and_distinct_agents() {
        let p = "do the thing";
        assert_eq!(native_base_command(Agent::Cursor, p).binary, "cursor");
        assert_eq!(
            native_base_command(Agent::Claude, p).args,
            claude_family_native_argv(p)
        );
        assert_eq!(
            native_base_command(Agent::Junie, p).args,
            claude_family_native_argv(p)
        );
        assert_eq!(
            native_base_command(Agent::Cursor, p).args,
            claude_family_native_argv(p)
        );
        assert_eq!(
            native_base_command(Agent::Codex, p).args,
            vec!["exec".to_string(), "--json".to_string(), p.to_string()]
        );
        assert_eq!(
            native_base_command(Agent::Cline, p).args,
            vec!["chat".to_string(), p.to_string()]
        );
        assert_eq!(
            native_base_command(Agent::Copilot, p).args,
            vec!["-p".to_string(), p.to_string()]
        );
    }

    #[test]
    fn caretta_native_command_appends_overrides_after_base() {
        let extra = vec!["--model".to_string(), "m".to_string()];
        let cmd = caretta_native_command(Agent::Gemini, "hi", &extra);
        assert_eq!(cmd.args[0..2], ["-p", "hi"]);
        assert_eq!(cmd.args[2..], ["--model", "m"]);
    }

    #[test]
    fn oversized_claude_prompt_uses_stdin_transport() {
        let prompt = "x".repeat(PROMPT_STDIN_BYTE_THRESHOLD + 1);
        let cmd = caretta_native_command_with_prompt_transport(Agent::Claude, &prompt, &[]);

        assert_eq!(cmd.command.binary, "claude");
        assert_eq!(cmd.command.args, claude_family_native_stdin_argv());
        assert_eq!(cmd.prompt_transport, PromptTransport::Stdin);
        assert!(!cmd.command.args.iter().any(|arg| arg == &prompt));
    }

    #[test]
    fn oversized_codex_prompt_uses_stdin_transport() {
        let prompt = "x".repeat(PROMPT_STDIN_BYTE_THRESHOLD + 1);
        let extra = vec!["--dangerously-bypass-approvals-and-sandbox".to_string()];
        let cmd = caretta_native_command_with_prompt_transport(Agent::Codex, &prompt, &extra);

        assert_eq!(cmd.command.binary, "codex");
        assert_eq!(
            cmd.command.args,
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-".to_string(),
                "--dangerously-bypass-approvals-and-sandbox".to_string()
            ]
        );
        assert_eq!(cmd.prompt_transport, PromptTransport::Stdin);
        assert!(!cmd.command.args.iter().any(|arg| arg == &prompt));
    }

    #[test]
    fn small_prompts_keep_existing_argv_shape() {
        let cmd = caretta_native_command_with_prompt_transport(Agent::Claude, "small", &[]);

        assert_eq!(cmd.command.args, claude_family_native_argv("small"));
        assert_eq!(cmd.prompt_transport, PromptTransport::Argv);

        let cmd = caretta_native_command_with_prompt_transport(Agent::Codex, "small", &[]);
        assert_eq!(
            cmd.command.args,
            vec!["exec".to_string(), "--json".to_string(), "-".to_string()]
        );
        assert_eq!(cmd.prompt_transport, PromptTransport::Stdin);
    }

    #[test]
    fn xai_model_selection_uses_dash_m() {
        let (args, env) = launch_model_selection(Agent::Xai, "grok-3");
        assert_eq!(args, vec!["-m".to_string(), "grok-3".to_string()]);
        assert!(env.is_empty());
    }

    #[test]
    fn codex_launch_env_caps_rust_log_at_error() {
        assert_eq!(
            launch_env(Agent::Codex),
            vec![("RUST_LOG".to_string(), "error".to_string())]
        );
        assert!(launch_env(Agent::Claude).is_empty());
    }

    #[test]
    fn xai_launch_env_caps_rust_log_at_error() {
        assert_eq!(
            launch_env(Agent::Xai),
            vec![("RUST_LOG".to_string(), "error".to_string())]
        );
    }

    #[test]
    fn xai_native_run_uses_streaming_json() {
        assert_eq!(
            native_base_command(Agent::Xai, "audit").args,
            vec![
                "-p".to_string(),
                "audit".to_string(),
                "--output-format".to_string(),
                "streaming-json".to_string(),
                "--no-auto-update".to_string(),
            ]
        );
    }
}
