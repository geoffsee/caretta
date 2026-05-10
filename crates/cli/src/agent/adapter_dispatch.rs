//! Dispatches [`cli_common::Agent`] to provider [`agent_common::AgentCliAdapter`] implementations.
//! All binary names and flag spellings for subprocess construction live in the provider crates.

use agent_common::{
    AdapterCapabilities, AgentCliAdapter, AgentCliCommand, WorkflowCapabilityRequirements,
};
use claude::{ClaudeWrapper, CursorWrapper};
use cli_common::Agent;
use cline::ClineWrapper;
use codex::CodexWrapper;
use copilot::CopilotWrapper;
use gemini::GeminiWrapper;
use grok::GrokWrapper;
use junie::JunieWrapper;
use xai::XaiWrapper;

pub const PROMPT_STDIN_BYTE_THRESHOLD: usize = 64 * 1024;

pub struct NativeRunCommand {
    pub command: AgentCliCommand,
    pub prompt_transport: PromptTransport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptTransport {
    Argv,
    Stdin,
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
        Agent::Grok => AgentCliCommand {
            binary: GrokWrapper.binary().to_string(),
            args: GrokWrapper.caretta_native_run_argv(prompt),
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
    let use_stdin = prompt.len() > PROMPT_STDIN_BYTE_THRESHOLD;
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
        Agent::Grok => GrokWrapper.launch_model_selection(model),
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
        Agent::Grok => GrokWrapper.launch_auto_mode(),
        Agent::Xai => XaiWrapper.launch_auto_mode(),
        Agent::Cline => ClineWrapper.launch_auto_mode(),
    }
}

/// Return the declared capabilities for `agent`.
pub fn adapter_capabilities(agent: Agent) -> AdapterCapabilities {
    match agent {
        Agent::Claude => ClaudeWrapper.capabilities(),
        Agent::Cursor => CursorWrapper.capabilities(),
        Agent::Junie => JunieWrapper.capabilities(),
        Agent::Copilot => CopilotWrapper.capabilities(),
        Agent::Codex => CodexWrapper.capabilities(),
        Agent::Gemini => GeminiWrapper.capabilities(),
        Agent::Grok => GrokWrapper.capabilities(),
        Agent::Xai => XaiWrapper.capabilities(),
        Agent::Cline => ClineWrapper.capabilities(),
    }
}

/// Check whether `agent` satisfies `requirements`. Returns `Err` with a human-readable
/// message listing missing capabilities; `Ok(())` otherwise.
pub fn check_capabilities(
    agent: Agent,
    requirements: &WorkflowCapabilityRequirements,
) -> Result<(), String> {
    let caps = adapter_capabilities(agent);
    requirements.check(&caps, &agent.to_string())
}

/// Print a table of all adapters and their declared capabilities to stdout.
pub fn run_list_adapters() {
    let adapters: &[(Agent, &str)] = &[
        (Agent::Claude, "claude"),
        (Agent::Cursor, "cursor"),
        (Agent::Cline, "cline"),
        (Agent::Codex, "codex"),
        (Agent::Copilot, "copilot"),
        (Agent::Gemini, "gemini"),
        (Agent::Grok, "grok"),
        (Agent::Junie, "junie"),
        (Agent::Xai, "xai"),
    ];

    println!(
        "{:<10}  {:<9}  {:<7}  {:<9}  {}",
        "ADAPTER", "TOOL_USE", "VISION", "STREAMING", "CONTEXT_WINDOW"
    );
    println!("{}", "-".repeat(58));
    for (agent, name) in adapters {
        let caps = adapter_capabilities(*agent);
        let ctx = caps
            .context_window
            .map(|w| format!("{w:>11}"))
            .unwrap_or_else(|| "    unknown".to_string());
        println!(
            "{:<10}  {:<9}  {:<7}  {:<9}  {}",
            name,
            if caps.tool_use { "yes" } else { "no" },
            if caps.vision { "yes" } else { "no" },
            if caps.streaming { "yes" } else { "no" },
            ctx,
        );
    }
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
    use clap::ValueEnum;
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
    }

    #[test]
    fn xai_model_selection_uses_env_not_args() {
        let (args, env) = launch_model_selection(Agent::Xai, "grok-3");
        assert!(args.is_empty());
        assert_eq!(
            env,
            vec![("COPILOT_MODEL".to_string(), "grok-3".to_string())]
        );
    }

    #[test]
    fn claude_capabilities_include_vision_and_large_context() {
        let caps = adapter_capabilities(Agent::Claude);
        assert!(caps.tool_use);
        assert!(caps.vision);
        assert!(caps.streaming);
        assert_eq!(caps.context_window, Some(200_000));
    }

    #[test]
    fn codex_capabilities_lack_tool_use_and_vision() {
        let caps = adapter_capabilities(Agent::Codex);
        assert!(!caps.tool_use);
        assert!(!caps.vision);
        assert!(caps.streaming);
    }

    #[test]
    fn gemini_capabilities_report_one_million_context() {
        let caps = adapter_capabilities(Agent::Gemini);
        assert!(caps.tool_use);
        assert!(caps.vision);
        assert_eq!(caps.context_window, Some(1_000_000));
    }

    #[test]
    fn capability_check_returns_error_for_missing_capability() {
        let reqs = agent_common::WorkflowCapabilityRequirements {
            tool_use: true,
            vision: false,
            streaming: false,
            min_context_window: None,
        };
        assert!(check_capabilities(Agent::Claude, &reqs).is_ok());
        assert!(check_capabilities(Agent::Codex, &reqs).is_err());
    }

    #[test]
    fn capability_check_passes_for_empty_requirements() {
        let reqs = agent_common::WorkflowCapabilityRequirements::default();
        for &agent in Agent::value_variants() {
            assert!(
                check_capabilities(agent, &reqs).is_ok(),
                "empty requirements should pass for every adapter"
            );
        }
    }
}
