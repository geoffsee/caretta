use crate::agent::adapter_dispatch;
use crate::agent::cmd::{count_tokens, log};
use crate::agent::launch::{auto_mode_overrides, merged_agent_env, model_selection_overrides};
use crate::agent::process::{emit_event, set_active_child_pid, stop_requested};
use crate::agent::types::{AgentEvent, AssistantMessage, Config, ContentBlock, RichAction};
use agent_common::{AgentCliAdapter, PromptTransport};
use agent_runtime::AgentRuntime;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use tempfile::NamedTempFile;

pub(crate) fn native_command(binary: &str, args: &[String]) -> Command {
    let mut cmd = if binary == "cursor" {
        // Cursor remains external for now. If bundled runtime cannot resolve it,
        // keep using the system CLI.
        match AgentRuntime::prepare() {
            Ok(runtime) => {
                if runtime.binary_path(binary).is_some() {
                    runtime.command_for_binary(binary)
                } else {
                    Command::new("cursor")
                }
            }
            Err(_) => Command::new("cursor"),
        }
    } else {
        match AgentRuntime::prepare() {
            Ok(runtime) => {
                let cli_cmd = agent_common::AgentCliCommand {
                    binary: binary.to_string(),
                    args: args.to_vec(),
                };
                return runtime.command_for_cli_command(&cli_cmd);
            }
            Err(_) => Command::new(binary),
        }
    };

    cmd.args(args);
    cmd
}

pub(crate) fn spawn_sanitized_stderr_logger(
    child: &mut Child,
    label: impl Into<String>,
    adapter: Option<&'static dyn AgentCliAdapter>,
) -> Option<std::thread::JoinHandle<()>> {
    let stderr = child.stderr.take()?;
    let label = label.into();
    Some(std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Scrub giant noise: HTML blocks, long blobs, Cloudflare challenges.
            if trimmed.len() > 4096
                || trimmed.contains("<html")
                || trimmed.contains("<!DOCTYPE")
                || trimmed.contains("<body")
                || trimmed.contains("<div")
                || trimmed.contains("<svg")
            {
                continue;
            }

            if let Some(adapter) = adapter
                && let Some(values) = adapter.parse_stderr_line(trimmed)
            {
                for v in values {
                    if let Ok(ev) = serde_json::from_value::<AgentEvent>(v) {
                        emit_event(ev);
                    }
                }
            }
            log(&format!("{label}: {trimmed}"));
        }
    }))
}

#[allow(clippy::too_many_arguments)]
fn run_native_agent(
    adapter: &'static dyn AgentCliAdapter,
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
    prompt: &str,
    stdin_prompt: Option<&str>,
    append_system_prompt: Option<&str>,
) -> bool {
    let started_at = Instant::now();
    let (launch_args, _system_prompt_file) =
        match args_with_append_system_prompt_file(args, append_system_prompt) {
            Ok(prepared) => prepared,
            Err(err) => {
                return handle_agent_launch_failure(
                    format!("Failed to prepare system prompt file for {binary}: {err}"),
                    started_at,
                    prompt,
                );
            }
        };
    if append_system_prompt.is_some() {
        log(&format!(
            "Appending caretta system prompt for {binary} via --append-system-prompt-file"
        ));
    }

    let mut cmd = native_command(binary, &launch_args);

    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let program = cmd.get_program().to_string_lossy().to_string();

    let mut child = match cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return handle_agent_spawn_error(binary, &program, err, started_at, prompt);
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Some(p) = stdin_prompt {
            log(&format!(
                "Sending {} byte prompt to {binary} stdin...",
                p.len()
            ));
            let _ = stdin.write_all(p.as_bytes());
            if !p.ends_with('\n') {
                let _ = stdin.write_all(b"\n");
            }
        }
        drop(stdin); // Explicitly send EOF
    }

    set_active_child_pid(Some(child.id()));
    let stderr_log =
        spawn_sanitized_stderr_logger(&mut child, format!("{binary} stderr"), Some(adapter));

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = BufReader::new(stdout);
    let mut saw_result = false;
    let mut output_text = String::new();

    for line in reader.lines().map_while(Result::ok) {
        if stop_requested() {
            let _ = child.kill();
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(values) = adapter.parse_output_line(trimmed) {
            for v in values {
                let ev = if let Ok(ev) = serde_json::from_value::<AgentEvent>(v.clone()) {
                    ev
                } else if let Ok(rev) = serde_json::from_value::<RichAction>(v) {
                    AgentEvent::Rich(rev)
                } else {
                    continue;
                };

                if ev.is_result() {
                    saw_result = true;
                }
                append_event_output(&ev, &mut output_text);
                if ev.is_assistant() {
                    log(&format!("{binary} assistant: {ev:?}"));
                }
                emit_event(ev);
            }
        } else {
            output_text.push_str(trimmed);
            output_text.push('\n');
            log(&format!("{binary}: {trimmed}"));
        }
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    if let Some(handle) = stderr_log {
        let _ = handle.join();
    }
    set_active_child_pid(None);
    if !saw_result {
        emit_event(estimated_result_event(
            ok,
            started_at.elapsed().as_millis(),
            prompt,
            output_text.trim(),
        ));
    }
    ok
}

pub fn u64_to_u32(value: Option<u64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

pub fn assistant_text_event(text: String) -> AgentEvent {
    AgentEvent::Rich(RichAction::Assistant {
        message: AssistantMessage {
            content: vec![ContentBlock::Text { text }],
        },
    })
}

pub fn assistant_block_event(block: ContentBlock) -> AgentEvent {
    AgentEvent::Rich(RichAction::Assistant {
        message: AssistantMessage {
            content: vec![block],
        },
    })
}

fn estimated_result_event(ok: bool, elapsed_ms: u128, prompt: &str, output: &str) -> AgentEvent {
    AgentEvent::Rich(RichAction::Result {
        status: if ok { "completed" } else { "failed" }.to_string(),
        summary: Some(
            "Usage estimated by caretta; provider token accounting was unavailable.".to_string(),
        ),
        duration_ms: u64::try_from(elapsed_ms).ok(),
        input_tokens: u64_to_u32(Some(count_tokens(prompt) as u64)),
        output_tokens: (!output.trim().is_empty())
            .then(|| count_tokens(output) as u64)
            .and_then(|tokens| u64_to_u32(Some(tokens))),
    })
}

fn handle_agent_spawn_error(
    binary: &str,
    program: &str,
    err: std::io::Error,
    started_at: Instant,
    prompt: &str,
) -> bool {
    handle_agent_launch_failure(
        format!("Failed to spawn {binary} at {program}: {err}"),
        started_at,
        prompt,
    )
}

fn handle_agent_launch_failure(message: String, started_at: Instant, prompt: &str) -> bool {
    log(&message);
    set_active_child_pid(None);
    emit_event(estimated_result_event(
        false,
        started_at.elapsed().as_millis(),
        prompt,
        &message,
    ));
    false
}

fn args_with_append_system_prompt_file(
    args: &[String],
    append_system_prompt: Option<&str>,
) -> std::io::Result<(Vec<String>, Option<NamedTempFile>)> {
    let Some(system_prompt) = append_system_prompt else {
        return Ok((args.to_vec(), None));
    };

    let file = system_prompt_tempfile(system_prompt)?;
    let mut args = args.to_vec();
    args.push("--append-system-prompt-file".to_string());
    args.push(file.path().to_string_lossy().to_string());
    Ok((args, Some(file)))
}

fn system_prompt_tempfile(prompt: &str) -> std::io::Result<NamedTempFile> {
    text_tempfile("caretta-system-prompt-", prompt)
}

fn text_tempfile(prefix: &str, contents: &str) -> std::io::Result<NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".txt")
        .tempfile()?;
    file.write_all(contents.as_bytes())?;
    if !contents.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.flush()?;
    Ok(file)
}

fn append_event_output(ev: &AgentEvent, output: &mut String) {
    match ev {
        AgentEvent::Rich(RichAction::Assistant { message }) => {
            for block in &message.content {
                if let ContentBlock::Text { text } = block {
                    output.push_str(text);
                    output.push('\n');
                }
            }
        }
        AgentEvent::Rich(RichAction::ContentBlockDelta { delta, .. }) => {
            if let Some(text) = &delta.text {
                output.push_str(text);
            }
        }
        _ => {}
    }
}

pub fn run_agent(cfg: &Config, prompt: &str) -> bool {
    run_agent_with_env(cfg, prompt, &[])
}

pub fn run_agent_with_env(cfg: &Config, prompt: &str, extra_env: &[(String, String)]) -> bool {
    run_agent_with_env_with_cwd(cfg, prompt, extra_env, None)
}

pub fn run_agent_with_env_in_dir(
    cfg: &Config,
    prompt: &str,
    extra_env: &[(String, String)],
    cwd: &Path,
) -> bool {
    run_agent_with_env_with_cwd(cfg, prompt, extra_env, Some(cwd))
}

fn run_agent_with_env_with_cwd(
    cfg: &Config,
    prompt: &str,
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
) -> bool {
    let env = merged_agent_env(cfg, extra_env);
    let mut overrides = local_inference_overrides(cfg);
    let model_ov = model_selection_overrides(cfg);
    overrides.args.extend(model_ov.args);
    let auto_ov = auto_mode_overrides(cfg);
    overrides.args.extend(auto_ov.args);

    let cmd = adapter_dispatch::caretta_native_command_with_prompt_transport(
        cfg.agent,
        prompt,
        &overrides.args,
    );
    let stdin_prompt = (cmd.prompt_transport == PromptTransport::Stdin).then_some(prompt);
    let adapter = adapter_dispatch::adapter_for_agent(cfg.agent);
    let append_system_prompt = adapter.system_prompt();

    run_native_agent(
        adapter,
        &cmd.command.binary,
        &cmd.command.args,
        &env,
        cwd,
        prompt,
        stdin_prompt,
        append_system_prompt,
    )
}

pub fn local_inference_overrides(cfg: &Config) -> crate::agent::types::AgentLaunchOverrides {
    crate::agent::launch::local_inference_overrides(cfg)
}

#[cfg(test)]
mod tests {
    use super::{args_with_append_system_prompt_file, native_command};
    use crate::agent::adapter_dispatch;
    use crate::agent::types::{Agent, AgentEvent, RichAction};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn native_command_uses_bundled_runtime_for_codex_when_available() {
        let cmd = native_command("codex", &["exec".to_string(), "--json".to_string()]);
        let program = PathBuf::from(cmd.get_program());
        let display = program.to_string_lossy();
        assert!(
            display.contains("caretta") || display == "codex",
            "unexpected codex program path: {display}"
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["exec".to_string(), "--json".to_string()]);
    }

    #[test]
    fn native_command_falls_back_to_system_cursor() {
        let cmd = native_command("cursor", &["-p".to_string(), "hi".to_string()]);
        assert_eq!(cmd.get_program().to_string_lossy(), "cursor");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["-p".to_string(), "hi".to_string()]);
    }

    #[test]
    fn only_claude_gets_appended_system_prompt() {
        let adapter = adapter_dispatch::adapter_for_agent(Agent::Claude);
        let prompt = adapter
            .system_prompt()
            .expect("claude should receive appended caretta guidance");

        assert!(prompt.contains("caretta's autonomous repository agent"));
        assert!(prompt.contains("preserve unrelated worktree changes"));

        let codex_adapter = adapter_dispatch::adapter_for_agent(Agent::Codex);
        assert_eq!(codex_adapter.system_prompt(), None);
    }

    #[test]
    fn append_system_prompt_arg_uses_temp_file_without_inlining_prompt() {
        let base_args = vec!["-p".to_string(), "hello".to_string()];
        let (args, system_prompt_file) =
            args_with_append_system_prompt_file(&base_args, Some("stable guidance"))
                .expect("system prompt temp file should be created");
        let system_prompt_file = system_prompt_file.expect("system prompt file should be retained");

        assert_eq!(&args[..base_args.len()], base_args.as_slice());
        let flag_index = args
            .iter()
            .position(|arg| arg == "--append-system-prompt-file")
            .expect("append system prompt flag should be present");
        let path_arg = args
            .get(flag_index + 1)
            .expect("append system prompt flag should include a file path");
        let expected_path = system_prompt_file.path().to_string_lossy().to_string();

        assert_eq!(path_arg, &expected_path);
        assert_eq!(
            fs::read_to_string(system_prompt_file.path())
                .expect("system prompt should be readable"),
            "stable guidance\n"
        );
        assert!(!args.iter().any(|arg| arg == "stable guidance"));
    }

    #[test]
    fn codex_turn_completed_maps_usage_to_result() {
        let adapter = adapter_dispatch::adapter_for_agent(Agent::Codex);
        let events = adapter
            .parse_output_line(
                r#"{"type":"turn.completed","duration_seconds":1.25,"usage":{"input_tokens":1000,"output_tokens":250}}"#,
            )
            .expect("valid codex event")
            .into_iter()
            .map(|v| {
                if let Ok(ev) = serde_json::from_value::<AgentEvent>(v.clone()) {
                    ev
                } else {
                    AgentEvent::Rich(serde_json::from_value::<RichAction>(v).expect("valid rich action"))
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Rich(RichAction::Result {
                duration_ms,
                input_tokens,
                output_tokens,
                ..
            }) => {
                assert_eq!(*duration_ms, Some(1250));
                assert_eq!(*input_tokens, Some(1000));
                assert_eq!(*output_tokens, Some(250));
            }
            other => panic!("expected result event, got {other:?}"),
        }
    }
}
