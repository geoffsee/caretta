use crate::agent::adapter_dispatch;
use crate::agent::adapter_dispatch::PromptTransport;
use crate::agent::cmd::{count_tokens, log};
use crate::agent::launch::{auto_mode_overrides, merged_agent_env, model_selection_overrides};
use crate::agent::process::{emit_event, set_active_child_pid, stop_requested};
use crate::agent::types::{Agent, AgentEvent, AssistantMessage, ClaudeEvent, Config, ContentBlock};
use agent_runtime::AgentRuntime;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;
use tempfile::NamedTempFile;

const CARETTA_CLAUDE_SYSTEM_PROMPT: &str = r#"You are caretta's autonomous repository agent.

Follow repository instructions from AGENTS.md, workflow prompts, tracker issues, and local status files when present.
Treat the user or workflow prompt as the source of task-specific scope.
When the task implies implementation, carry it through edits, verification, and a concise outcome report.
Make the smallest coherent code changes that complete the task, and preserve unrelated worktree changes.
Prefer existing project patterns and tools over new abstractions.
Run the most relevant verification commands available in the repository, and report failures or blockers plainly.
If required context is missing or instructions conflict, surface the blocker instead of guessing.
"#;

fn native_command(binary: &str, args: &[String]) -> Command {
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

pub fn run_claude_native_with_env(
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
) -> bool {
    run_claude_native_with_env_for_prompt(binary, args, extra_env, cwd, "")
}

fn run_claude_native_with_env_for_prompt(
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
    prompt: &str,
) -> bool {
    run_claude_native_with_env_for_prompt_and_stdin(
        binary, args, extra_env, cwd, prompt, None, None,
    )
}

fn run_claude_native_with_env_for_prompt_and_stdin(
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
    let _prompt_file =
        match attach_prompt_stdin(&mut cmd, stdin_prompt, binary, &program, started_at, prompt) {
            Ok(prompt_file) => prompt_file,
            Err(ok) => return ok,
        };
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn() {
        Ok(child) => child,
        Err(err) => {
            return handle_agent_spawn_error(binary, &program, err, started_at, prompt);
        }
    };
    set_active_child_pid(Some(child.id()));

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = BufReader::new(stdout);
    let mut saw_result = false;
    let mut raw_output = String::new();

    for line in reader.lines().map_while(Result::ok) {
        if stop_requested() {
            let _ = child.kill();
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<ClaudeEvent>(trimmed) {
            if matches!(ev, ClaudeEvent::Result { .. }) {
                saw_result = true;
            }
            emit_event(AgentEvent::Claude(ev));
        } else {
            raw_output.push_str(trimmed);
            raw_output.push('\n');
            log(&format!("claude: {trimmed}"));
        }
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    set_active_child_pid(None);
    if !saw_result {
        emit_event(estimated_result_event(
            ok,
            started_at.elapsed().as_millis(),
            prompt,
            raw_output.trim(),
        ));
    }
    ok
}

pub fn u64_to_u32(value: Option<u64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

pub fn assistant_text_event(text: String) -> AgentEvent {
    AgentEvent::Claude(ClaudeEvent::Assistant {
        message: AssistantMessage {
            content: vec![ContentBlock::Text { text }],
        },
    })
}

pub fn assistant_block_event(block: ContentBlock) -> AgentEvent {
    AgentEvent::Claude(ClaudeEvent::Assistant {
        message: AssistantMessage {
            content: vec![block],
        },
    })
}

pub fn codex_events_from_json_line(line: &str) -> Option<Vec<AgentEvent>> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    let mut out = Vec::new();
    match event_type {
        "thread.started" => {
            let description = v
                .get("thread_id")
                .and_then(serde_json::Value::as_str)
                .map(|id| format!("Thread {id}"));
            out.push(AgentEvent::Claude(ClaudeEvent::System {
                subtype: "thread_started".to_string(),
                model: Some("codex".to_string()),
                description,
                session_id: None,
                claude_code_version: None,
                tools: None,
            }));
        }
        "turn.started" => {
            out.push(AgentEvent::Claude(ClaudeEvent::System {
                subtype: "turn_started".to_string(),
                model: Some("codex".to_string()),
                description: None,
                session_id: None,
                claude_code_version: None,
                tools: None,
            }));
        }
        "item.started" | "item.completed" => {
            let is_completed = event_type == "item.completed";
            let Some(item) = v.get("item").and_then(serde_json::Value::as_object) else {
                return Some(out);
            };
            let _item_id = item
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("codex_item")
                .to_string();
            let item_type = item
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");

            match item_type {
                "message" => {
                    if let Some(content_arr) =
                        item.get("content").and_then(serde_json::Value::as_array)
                    {
                        for c in content_arr {
                            if let Some(text) = c.get("text").and_then(serde_json::Value::as_str)
                                && !is_completed
                            {
                                out.push(assistant_text_event(text.to_string()));
                            }
                        }
                    }
                }
                "tool_call" => {
                    if let Some(call) = item.get("call").and_then(serde_json::Value::as_object) {
                        let name = call
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        let args = call
                            .get("arguments")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        if !is_completed {
                            out.push(assistant_block_event(ContentBlock::ToolUse {
                                id: "codex_tool".to_string(),
                                name: name.to_string(),
                                input: serde_json::from_str(args).unwrap_or(serde_json::json!({})),
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
        "delta.started" => {
            if let Some(delta) = v.get("delta").and_then(serde_json::Value::as_object)
                && let Some(text) = delta.get("text").and_then(serde_json::Value::as_str)
            {
                out.push(AgentEvent::Claude(ClaudeEvent::ContentBlockDelta {
                    index: 0,
                    delta: crate::agent::types::ContentBlockDelta {
                        delta_type: "text_delta".to_string(),
                        text: Some(text.to_string()),
                    },
                }));
            }
        }
        "turn.completed" | "turn.failed" | "response.completed" | "response.failed" => {
            let usage = usage_value(&v);
            let input_tokens = usage
                .and_then(|u| json_u32_any(u, &["input_tokens", "prompt_tokens"]))
                .or_else(|| json_u32_any(&v, &["input_tokens", "prompt_tokens"]));
            let output_tokens = usage
                .and_then(|u| json_u32_any(u, &["output_tokens", "completion_tokens"]))
                .or_else(|| json_u32_any(&v, &["output_tokens", "completion_tokens"]));
            let duration_ms = json_duration_ms(&v);
            let status = v
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| {
                    if event_type.ends_with(".failed") {
                        "failed"
                    } else {
                        "completed"
                    }
                })
                .to_string();
            let summary = v
                .get("message")
                .or_else(|| v.get("error"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);

            out.push(AgentEvent::Claude(ClaudeEvent::Result {
                status,
                summary,
                duration_ms,
                input_tokens,
                output_tokens,
            }));
        }
        _ => {}
    }
    Some(out)
}

fn usage_value(v: &serde_json::Value) -> Option<&serde_json::Value> {
    v.get("usage")
        .or_else(|| v.pointer("/response/usage"))
        .or_else(|| v.pointer("/turn/usage"))
}

fn json_u32_any(v: &serde_json::Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| v.get(*key).and_then(serde_json::Value::as_u64))
        .and_then(|n| u32::try_from(n).ok())
}

fn json_duration_ms(v: &serde_json::Value) -> Option<u64> {
    ["duration_ms", "elapsed_ms", "wall_time_ms"]
        .iter()
        .find_map(|key| v.get(*key).and_then(serde_json::Value::as_u64))
        .or_else(|| {
            ["duration_seconds", "elapsed_seconds"]
                .iter()
                .find_map(|key| v.get(*key).and_then(serde_json::Value::as_f64))
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .map(|seconds| (seconds * 1000.0).round() as u64)
        })
}

fn estimated_result_event(ok: bool, elapsed_ms: u128, prompt: &str, output: &str) -> AgentEvent {
    AgentEvent::Claude(ClaudeEvent::Result {
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

fn attach_prompt_stdin(
    cmd: &mut Command,
    stdin_prompt: Option<&str>,
    binary: &str,
    program: &str,
    started_at: Instant,
    prompt: &str,
) -> Result<Option<NamedTempFile>, bool> {
    let Some(stdin_prompt) = stdin_prompt else {
        return Ok(None);
    };

    match prompt_tempfile(stdin_prompt) {
        Ok(file) => match file.reopen() {
            Ok(stdin_file) => {
                cmd.stdin(Stdio::from(stdin_file));
                log(&format!(
                    "Prompt is {} bytes; sending to {binary} through a temp-file stdin handle",
                    stdin_prompt.len()
                ));
                Ok(Some(file))
            }
            Err(err) => Err(handle_agent_spawn_error(
                binary, program, err, started_at, prompt,
            )),
        },
        Err(err) => Err(handle_agent_spawn_error(
            binary, program, err, started_at, prompt,
        )),
    }
}

fn prompt_tempfile(prompt: &str) -> std::io::Result<NamedTempFile> {
    text_tempfile("caretta-prompt-", prompt)
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
    file.flush()?;
    Ok(file)
}

fn append_event_output(ev: &AgentEvent, output: &mut String) {
    match ev {
        AgentEvent::Claude(ClaudeEvent::Assistant { message }) => {
            for block in &message.content {
                if let ContentBlock::Text { text } = block {
                    output.push_str(text);
                    output.push('\n');
                }
            }
        }
        AgentEvent::Claude(ClaudeEvent::ContentBlockDelta { delta, .. }) => {
            if let Some(text) = &delta.text {
                output.push_str(text);
            }
        }
        _ => {}
    }
}

pub fn run_codex_native_with_env(
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
) -> bool {
    run_codex_native_with_env_for_prompt(binary, args, extra_env, cwd, "")
}

fn run_codex_native_with_env_for_prompt(
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
    prompt: &str,
) -> bool {
    run_codex_native_with_env_for_prompt_and_stdin(binary, args, extra_env, cwd, prompt, None)
}

fn run_codex_native_with_env_for_prompt_and_stdin(
    binary: &str,
    args: &[String],
    extra_env: &[(String, String)],
    cwd: Option<&Path>,
    prompt: &str,
    stdin_prompt: Option<&str>,
) -> bool {
    let mut cmd = native_command(binary, args);

    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let program = cmd.get_program().to_string_lossy().to_string();
    let started_at = Instant::now();
    let _prompt_file =
        match attach_prompt_stdin(&mut cmd, stdin_prompt, binary, &program, started_at, prompt) {
            Ok(prompt_file) => prompt_file,
            Err(ok) => return ok,
        };
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn() {
        Ok(child) => child,
        Err(err) => {
            return handle_agent_spawn_error(binary, &program, err, started_at, prompt);
        }
    };
    set_active_child_pid(Some(child.id()));

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
        if let Some(events) = codex_events_from_json_line(trimmed) {
            for ev in events {
                if matches!(ev, AgentEvent::Claude(ClaudeEvent::Result { .. })) {
                    saw_result = true;
                }
                append_event_output(&ev, &mut output_text);
                emit_event(ev);
            }
        } else {
            output_text.push_str(trimmed);
            output_text.push('\n');
            log(&format!("codex: {trimmed}"));
        }
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
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
    let append_system_prompt_owned = build_appended_system_prompt(cfg);
    match cfg.agent {
        Agent::Codex => run_codex_native_with_env_for_prompt_and_stdin(
            &cmd.command.binary,
            &cmd.command.args,
            &env,
            cwd,
            prompt,
            stdin_prompt,
        ),
        _ => run_claude_native_with_env_for_prompt_and_stdin(
            &cmd.command.binary,
            &cmd.command.args,
            &env,
            cwd,
            prompt,
            stdin_prompt,
            append_system_prompt_owned.as_deref(),
        ),
    }
}

fn appended_system_prompt_for_agent(agent: Agent) -> Option<&'static str> {
    matches!(agent, Agent::Claude).then_some(CARETTA_CLAUDE_SYSTEM_PROMPT)
}

/// Build the full appended system prompt for a given config, including any
/// active path-constraint guidance appended after the base caretta guidance.
#[cfg(not(target_arch = "wasm32"))]
fn build_appended_system_prompt(cfg: &Config) -> Option<String> {
    if !cfg.path_constraints.is_unconstrained() && cfg.agent != Agent::Claude {
        tracing::warn!(
            agent = %cfg.agent,
            "path_constraints are configured but this agent does not support \
             system-prompt append; constraints will be audited post-hoc only \
             — the agent receives no in-prompt guidance to respect them"
        );
    }
    let base = appended_system_prompt_for_agent(cfg.agent)?;
    match crate::agent::path_constraint::build_system_prompt_fragment(&cfg.path_constraints) {
        Some(fragment) => Some(format!("{base}\n{fragment}\n")),
        None => Some(base.to_string()),
    }
}

#[cfg(target_arch = "wasm32")]
fn build_appended_system_prompt(cfg: &Config) -> Option<String> {
    appended_system_prompt_for_agent(cfg.agent).map(str::to_string)
}

pub fn local_inference_overrides(cfg: &Config) -> crate::agent::types::AgentLaunchOverrides {
    crate::agent::launch::local_inference_overrides(cfg)
}

#[cfg(test)]
mod tests {
    use super::{
        appended_system_prompt_for_agent, args_with_append_system_prompt_file,
        codex_events_from_json_line, native_command, run_claude_native_with_env,
        run_codex_native_with_env,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use super::build_appended_system_prompt;
    use crate::agent::types::{Agent, AgentEvent, ClaudeEvent, Config, PathConstraints};
    use std::fs;
    use std::path::PathBuf;

    #[cfg(not(target_arch = "wasm32"))]
    fn make_test_config(agent: Agent, path_constraints: PathConstraints) -> Config {
        Config {
            agent,
            model: String::new(),
            auto_mode: false,
            dry_run: false,
            local_inference: Default::default(),
            root: "/".into(),
            project_name: "test".into(),
            scan_targets: Default::default(),
            skill_paths: Default::default(),
            bootstrap_agent_files: false,
            bootstrap_snapshot: false,
            workflow_preset: "default".to_string(),
            use_subscription: false,
            pricing: Default::default(),
            bot_settings: Default::default(),
            bot_credentials: None,
            test: Default::default(),
            event_log_path: None,
            path_constraints,
        }
    }

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
    fn claude_spawn_error_returns_false_instead_of_panicking() {
        let result = std::panic::catch_unwind(|| {
            run_claude_native_with_env("__caretta_missing_agent__", &[], &[], None)
        });

        assert!(
            !result.expect("spawn failure should not panic"),
            "missing agent binary should report an unsuccessful run"
        );
    }

    #[test]
    fn codex_spawn_error_returns_false_instead_of_panicking() {
        let result = std::panic::catch_unwind(|| {
            run_codex_native_with_env("__caretta_missing_agent__", &[], &[], None)
        });

        assert!(
            !result.expect("spawn failure should not panic"),
            "missing agent binary should report an unsuccessful run"
        );
    }

    #[test]
    fn only_claude_gets_appended_system_prompt() {
        let prompt = appended_system_prompt_for_agent(Agent::Claude)
            .expect("claude should receive appended caretta guidance");

        assert!(prompt.contains("caretta's autonomous repository agent"));
        assert!(prompt.contains("preserve unrelated worktree changes"));
        assert_eq!(appended_system_prompt_for_agent(Agent::Codex), None);
        assert_eq!(appended_system_prompt_for_agent(Agent::Cursor), None);
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
            "stable guidance"
        );
        assert!(!args.iter().any(|arg| arg == "stable guidance"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn build_appended_prompt_claude_with_constraints_appends_fragment() {
        let cfg = make_test_config(
            Agent::Claude,
            PathConstraints {
                allow_paths: vec!["src/".to_string()],
                deny_paths: vec![],
            },
        );
        let prompt = build_appended_system_prompt(&cfg)
            .expect("Claude with constraints should produce a prompt");
        assert!(prompt.contains("caretta's autonomous repository agent"),
            "base guidance missing");
        assert!(prompt.contains("Allowed path prefixes"),
            "path-constraint fragment missing");
        assert!(prompt.contains("src/"), "allow_paths value missing from fragment");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn build_appended_prompt_non_claude_with_constraints_returns_none() {
        let cfg = make_test_config(
            Agent::Codex,
            PathConstraints {
                allow_paths: vec!["src/".to_string()],
                deny_paths: vec![],
            },
        );
        // Codex has no base system-prompt to append to, so the function returns None
        // regardless of path constraints (a tracing::warn is still emitted above).
        assert!(build_appended_system_prompt(&cfg).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn build_appended_prompt_no_triple_newline_between_base_and_fragment() {
        let cfg = make_test_config(
            Agent::Claude,
            PathConstraints {
                allow_paths: vec!["src/".to_string()],
                deny_paths: vec![],
            },
        );
        let prompt = build_appended_system_prompt(&cfg)
            .expect("Claude with constraints should produce a prompt");
        // The base prompt ends with "\n" and the format adds one separator "\n",
        // yielding a blank line between sections ("\n\n"). A triple newline would
        // indicate unintended extra whitespace in the assembly logic.
        assert!(!prompt.contains("\n\n\n"),
            "unexpected triple newline in assembled prompt");
    }

    #[test]
    fn codex_turn_completed_maps_usage_to_result() {
        let events = codex_events_from_json_line(
            r#"{"type":"turn.completed","duration_seconds":1.25,"usage":{"input_tokens":1000,"output_tokens":250}}"#,
        )
        .expect("valid codex event");

        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Claude(ClaudeEvent::Result {
                status,
                duration_ms,
                input_tokens,
                output_tokens,
                ..
            }) => {
                assert_eq!(status, "completed");
                assert_eq!(*duration_ms, Some(1250));
                assert_eq!(*input_tokens, Some(1000));
                assert_eq!(*output_tokens, Some(250));
            }
            other => panic!("expected result event, got {other:?}"),
        }
    }
}
