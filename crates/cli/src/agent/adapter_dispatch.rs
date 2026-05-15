//! Dispatches [`cli_common::Agent`] to provider [`agent_common::AgentCliAdapter`] implementations.
//! All binary names and flag spellings for subprocess construction live in the provider crates.
//!
//! Also owns the capability registry: each adapter crate embeds a `capabilities.json` manifest
//! declaring its supported [`agent_common::AgentInvocation`] variants. Call
//! [`validate_capability_manifests`] at startup to catch missing or malformed manifests early.

use agent_common::{AgentCliAdapter, AgentCliCommand};
use claude::{ClaudeWrapper, CursorWrapper};
use cli_common::Agent;
use cline::ClineWrapper;
use codex::CodexWrapper;
use copilot::CopilotWrapper;
use gemini::GeminiWrapper;
use grok::GrokWrapper;
use junie::JunieWrapper;
use xai::XaiWrapper;

// ── Capability registry ──────────────────────────────────────────────────────

const KNOWN_INVOCATIONS: &[&str] = &[
    "Help",
    "Version",
    "Model",
    "Prompt",
    "Resume",
    "Project",
    "OutputFormat",
    "Yolo",
];

/// Returns the raw `capabilities.json` JSON for the given agent.
pub fn capability_manifest_for(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => ClaudeWrapper.capability_manifest_json(),
        Agent::Cursor => CursorWrapper.capability_manifest_json(),
        Agent::Cline => ClineWrapper.capability_manifest_json(),
        Agent::Codex => CodexWrapper.capability_manifest_json(),
        Agent::Copilot => CopilotWrapper.capability_manifest_json(),
        Agent::Gemini => GeminiWrapper.capability_manifest_json(),
        Agent::Grok => GrokWrapper.capability_manifest_json(),
        Agent::Junie => JunieWrapper.capability_manifest_json(),
        Agent::Xai => XaiWrapper.capability_manifest_json(),
    }
}

/// Returns `true` if the agent's capability manifest lists the given
/// [`AgentInvocation`] variant name (e.g. `"Prompt"`, `"Resume"`).
pub fn agent_supports_invocation(agent: Agent, invocation: &str) -> bool {
    let json = capability_manifest_for(agent);
    if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(json)
        && let Some(arr) = manifest["supported_invocations"].as_array()
    {
        return arr.iter().any(|v| v.as_str() == Some(invocation));
    }
    false
}

/// Validates every adapter's `capabilities.json` manifest at startup.
///
/// Returns an error string identifying the crate and the problem when a
/// manifest is missing (compile-time error) or malformed / contains an
/// unknown [`AgentInvocation`] variant name (runtime error).  Callers
/// should treat a non-`Ok` result as a fatal startup error.
pub fn validate_capability_manifests() -> Result<(), String> {
    let entries: &[(&str, &str)] = &[
        ("claude", ClaudeWrapper.capability_manifest_json()),
        ("claude (cursor)", CursorWrapper.capability_manifest_json()),
        ("cline", ClineWrapper.capability_manifest_json()),
        ("codex", CodexWrapper.capability_manifest_json()),
        ("copilot", CopilotWrapper.capability_manifest_json()),
        ("gemini", GeminiWrapper.capability_manifest_json()),
        ("grok", GrokWrapper.capability_manifest_json()),
        ("junie", JunieWrapper.capability_manifest_json()),
        ("xai", XaiWrapper.capability_manifest_json()),
    ];

    for (adapter_name, json) in entries {
        let manifest = serde_json::from_str::<serde_json::Value>(json).map_err(|e| {
            format!(
                "crates/{adapter_name}/capabilities.json is malformed: {e}\n\
                     Fix: ensure the file contains valid JSON with an \
                     \"supported_invocations\" array."
            )
        })?;

        let invocations = manifest["supported_invocations"]
            .as_array()
            .ok_or_else(|| {
                format!(
                    "crates/{adapter_name}/capabilities.json: missing required \
                 \"supported_invocations\" array"
                )
            })?;

        for inv in invocations {
            let name = inv.as_str().ok_or_else(|| {
                format!(
                    "crates/{adapter_name}/capabilities.json: \
                     all entries in \"supported_invocations\" must be strings"
                )
            })?;
            if !KNOWN_INVOCATIONS.contains(&name) {
                return Err(format!(
                    "crates/{adapter_name}/capabilities.json: unknown \
                     AgentInvocation variant \"{name}\"\n\
                     Known variants: {}",
                    KNOWN_INVOCATIONS.join(", ")
                ));
            }
        }
    }

    Ok(())
}

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
    fn all_capability_manifests_are_valid() {
        validate_capability_manifests()
            .expect("all capability manifests must be valid at test time");
    }

    #[test]
    fn capability_manifest_lists_expected_invocations() {
        assert!(agent_supports_invocation(Agent::Claude, "Prompt"));
        assert!(agent_supports_invocation(Agent::Claude, "Resume"));
        assert!(!agent_supports_invocation(Agent::Claude, "Project"));
        assert!(!agent_supports_invocation(Agent::Claude, "Yolo"));

        assert!(agent_supports_invocation(Agent::Grok, "Prompt"));
        assert!(!agent_supports_invocation(Agent::Grok, "Resume"));
        assert!(agent_supports_invocation(Agent::Grok, "Project"));

        assert!(agent_supports_invocation(Agent::Cline, "Yolo"));
        assert!(agent_supports_invocation(Agent::Codex, "Yolo"));
        assert!(!agent_supports_invocation(Agent::Codex, "OutputFormat"));

        assert!(agent_supports_invocation(Agent::Junie, "Yolo"));
        assert!(agent_supports_invocation(Agent::Junie, "Project"));
    }

    #[test]
    fn validate_rejects_malformed_json() {
        let result = serde_json::from_str::<serde_json::Value>("not-json");
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_unknown_invocation_names() {
        let bad_json = r#"{"agent":"x","binary":"x","supported_invocations":["Typo"]}"#;
        let manifest = serde_json::from_str::<serde_json::Value>(bad_json).unwrap();
        let invocations = manifest["supported_invocations"].as_array().unwrap();
        for inv in invocations {
            let name = inv.as_str().unwrap();
            assert!(
                !KNOWN_INVOCATIONS.contains(&name),
                "expected 'Typo' to not be a known invocation"
            );
        }
    }

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
}
