// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;
use cli_common::{AgentEvent, AssistantMessage, ContentBlock, ContentBlockDelta, RichAction};

#[derive(Debug, Clone, Copy, Default)]
pub struct XaiWrapper;

fn grok_stderr_user_message(line: &str) -> Option<String> {
    if line.contains("tool_output_error") || line.contains("tool_error:") {
        return Some(
            "Grok reported a tool execution error. Verify files exist and tool arguments match the Grok CLI schema (not Cursor-style names like target_file)."
                .to_string(),
        );
    }
    if line.contains("403 Forbidden") {
        return Some(
            "Grok/xAI authentication failed (403). Check XAI_API_KEY or re-authenticate with the Grok CLI."
                .to_string(),
        );
    }
    if line.contains("does not exist")
        && line.contains("model")
        && line.contains("grok-build")
        && !line.contains("grok-build-0.1")
    {
        // Grok CLI session-title generation hard-codes `grok-build`; harmless when the
        // session model (e.g. grok-build-0.1) is valid.
        return None;
    }
    if line.contains("does not exist") && line.contains("model") {
        return Some(
            "Grok/xAI model not found or not accessible for your team. Check the model name and API access."
                .to_string(),
        );
    }
    None
}

fn grok_text_delta(text: &str) -> serde_json::Value {
    serde_json::to_value(RichAction::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDelta {
            delta_type: "text_delta".to_string(),
            text: Some(text.to_string()),
        },
    })
    .expect("content block delta serializes")
}

fn grok_tool_use(name: &str, input: serde_json::Value) -> serde_json::Value {
    serde_json::to_value(RichAction::Assistant {
        message: AssistantMessage {
            content: vec![ContentBlock::ToolUse {
                id: "grok_tool".to_string(),
                name: name.to_string(),
                input,
            }],
        },
    })
    .expect("tool use serializes")
}

fn json_u32_any(v: &serde_json::Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| v.get(*key).and_then(serde_json::Value::as_u64))
        .and_then(|n| u32::try_from(n).ok())
}

fn grok_event_text(v: &serde_json::Value) -> Option<&str> {
    v.get("data")
        .or_else(|| v.get("text"))
        .or_else(|| v.get("content"))
        .and_then(serde_json::Value::as_str)
}

fn grok_parse_output_line(line: &str) -> Option<Vec<serde_json::Value>> {
    if let Ok(ev) = serde_json::from_str::<RichAction>(line) {
        return Some(vec![serde_json::to_value(ev).ok()?]);
    }

    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type").and_then(serde_json::Value::as_str)?;
    let mut out = Vec::new();

    match event_type {
        "text" => {
            if let Some(text) = grok_event_text(&v) {
                out.push(grok_text_delta(text));
            }
        }
        "thought" | "step_start" => {
            // Consumed; omit reasoning / step markers from CLI transcript.
            return Some(Vec::new());
        }
        "tool_use" => {
            let name = v
                .get("name")
                .or_else(|| v.get("tool"))
                .or_else(|| v.get("tool_name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let input = v
                .get("input")
                .or_else(|| v.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::json!({}));
            if !name.is_empty() {
                out.push(grok_tool_use(name, input));
            }
        }
        "step_finish" | "result" | "end" => {
            let usage = v.get("usage").unwrap_or(&v);
            let input_tokens = json_u32_any(usage, &["input_tokens", "prompt_tokens"]);
            let output_tokens = json_u32_any(usage, &["output_tokens", "completion_tokens"]);
            let duration_ms = json_u32_any(&v, &["duration_ms", "elapsed_ms"]).map(u64::from);
            let status = v
                .get("status")
                .or_else(|| v.get("stopReason"))
                .and_then(serde_json::Value::as_str)
                .map(|s| {
                    if s.eq_ignore_ascii_case("endturn") || s.eq_ignore_ascii_case("completed") {
                        "completed".to_string()
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or_else(|| "completed".to_string());
            let summary = v
                .get("summary")
                .or_else(|| v.get("message"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            out.push(
                serde_json::to_value(RichAction::Result {
                    status,
                    summary,
                    duration_ms,
                    input_tokens,
                    output_tokens,
                })
                .ok()?,
            );
        }
        "error" => {
            let message = v
                .get("message")
                .or_else(|| v.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Grok reported an error");
            out.push(
                serde_json::to_value(AgentEvent::Log {
                    message: message.to_string(),
                })
                .ok()?,
            );
        }
        _ => {}
    }

    if out.is_empty() { None } else { Some(out) }
}

impl AgentCliAdapter for XaiWrapper {
    fn binary(&self) -> &'static str {
        "grok"
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        // `grok version` can require auth in newer releases; `--version` is non-auth.
        vec!["--version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec!["-m".to_string(), model.to_string()])
    }

    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        vec!["--prompt".to_string(), prompt.to_string()]
    }

    fn resume_args(&self, _session_id: Option<&str>) -> Option<Vec<String>> {
        None
    }

    fn project_args(&self, project: &str) -> Option<Vec<String>> {
        Some(vec!["--directory".to_string(), project.to_string()])
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec![
            "-p".to_string(),
            prompt.to_string(),
            "--output-format".to_string(),
            "streaming-json".to_string(),
            "--no-auto-update".to_string(),
        ]
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (vec!["-m".to_string(), model.to_string()], Vec::new())
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        // Headless housekeeping needs host `gh`/`git` and auto-approved tools.
        // `--sandbox` requires a profile in grok 0.2.60+ and fails without one.
        vec!["--always-approve".to_string()]
    }

    fn launch_env(&self) -> Vec<(String, String)> {
        // Inherited RUST_LOG=info from caretta makes grok emit per-token SSE tracing on stderr.
        vec![("RUST_LOG".to_string(), "error".to_string())]
    }

    fn should_log_stderr_line(&self, _line: &str) -> bool {
        // Grok stderr is internal tracing; surface actionable errors via parse_stderr_line.
        false
    }

    fn parse_stderr_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        let message = grok_stderr_user_message(line)?;
        serde_json::to_value(AgentEvent::Log { message })
            .ok()
            .map(|value| vec![value])
    }

    fn parse_output_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        grok_parse_output_line(line)
    }
}

#[cfg(test)]
mod tests {
    use super::XaiWrapper;
    use agent_common::AgentCliAdapter;
    use cli_common::{AgentEvent, RichAction};

    #[test]
    fn uses_grok_binary_with_xai_adapter() {
        let wrapper = XaiWrapper;
        assert_eq!(wrapper.binary(), "grok");
        assert_eq!(
            wrapper.model_args("grok-4"),
            Some(vec!["-m".to_string(), "grok-4".to_string()])
        );
        assert_eq!(
            wrapper.prompt_args("hello"),
            vec!["--prompt".to_string(), "hello".to_string()]
        );
        assert_eq!(wrapper.resume_args(Some("x1")), None);
    }

    #[test]
    fn native_run_uses_streaming_json() {
        let wrapper = XaiWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("go"),
            vec![
                "-p".to_string(),
                "go".to_string(),
                "--output-format".to_string(),
                "streaming-json".to_string(),
                "--no-auto-update".to_string(),
            ]
        );
    }

    #[test]
    fn version_uses_flag() {
        let wrapper = XaiWrapper;
        assert_eq!(wrapper.version_args(), vec!["--version".to_string()]);
    }

    #[test]
    fn launch_auto_mode_uses_always_approve() {
        let wrapper = XaiWrapper;
        assert_eq!(
            wrapper.launch_auto_mode(),
            vec!["--always-approve".to_string()]
        );
    }

    #[test]
    fn launch_env_caps_grok_rust_log_at_error() {
        let wrapper = XaiWrapper;
        assert_eq!(
            wrapper.launch_env(),
            vec![("RUST_LOG".to_string(), "error".to_string())]
        );
    }

    #[test]
    fn stderr_logging_suppressed_but_tool_errors_surface() {
        let wrapper = XaiWrapper;
        assert!(!wrapper.should_log_stderr_line("ERROR tool_output_error"));
        assert!(
            wrapper
                .parse_stderr_line("tool_error: tool_output_error tool_name=read_file")
                .is_some()
        );
    }

    #[test]
    fn ignores_benign_grok_build_title_model_error() {
        let wrapper = XaiWrapper;
        assert!(
            wrapper
                .parse_stderr_line("not-found: The model grok-build does not exist")
                .is_none()
        );
    }

    #[test]
    fn parses_claude_compatible_assistant_events() {
        let wrapper = XaiWrapper;
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"audit"}]}}"#;
        let values = wrapper.parse_output_line(line).expect("assistant event");
        assert_eq!(values.len(), 1);
        let ev = serde_json::from_value::<RichAction>(values[0].clone()).expect("rich action");
        assert!(matches!(ev, RichAction::Assistant { .. }));
    }

    #[test]
    fn parses_grok_streaming_json_text_and_end_events() {
        let wrapper = XaiWrapper;
        let text = wrapper
            .parse_output_line(r#"{"type":"text","data":"ok"}"#)
            .expect("text event");
        assert_eq!(text.len(), 1);
        let delta = serde_json::from_value::<RichAction>(text[0].clone()).expect("delta");
        assert!(matches!(delta, RichAction::ContentBlockDelta { .. }));

        let finish = wrapper
            .parse_output_line(
                r#"{"type":"end","stopReason":"EndTurn","sessionId":"s1","requestId":"r1"}"#,
            )
            .expect("end event");
        assert_eq!(finish.len(), 1);
        match serde_json::from_value::<RichAction>(finish[0].clone()).expect("result") {
            RichAction::Result { status, .. } => assert_eq!(status, "completed"),
            other => panic!("expected result, got {other:?}"),
        }
    }

    #[test]
    fn parses_grok_text_and_step_finish_events() {
        let wrapper = XaiWrapper;
        let text = wrapper
            .parse_output_line(r#"{"type":"text","text":"Finding 1"}"#)
            .expect("text event");
        assert_eq!(text.len(), 1);
        let delta = serde_json::from_value::<RichAction>(text[0].clone()).expect("delta");
        assert!(matches!(delta, RichAction::ContentBlockDelta { .. }));

        let finish = wrapper
            .parse_output_line(
                r#"{"type":"step_finish","usage":{"input_tokens":10,"output_tokens":5}}"#,
            )
            .expect("step_finish");
        assert_eq!(finish.len(), 1);
        match serde_json::from_value::<RichAction>(finish[0].clone()).expect("result") {
            RichAction::Result {
                input_tokens,
                output_tokens,
                ..
            } => {
                assert_eq!(input_tokens, Some(10));
                assert_eq!(output_tokens, Some(5));
            }
            other => panic!("expected result, got {other:?}"),
        }
    }

    #[test]
    fn parses_grok_error_event_as_log() {
        let wrapper = XaiWrapper;
        let values = wrapper
            .parse_output_line(r#"{"type":"error","message":"rate limited"}"#)
            .expect("error event");
        let ev = serde_json::from_value::<AgentEvent>(values[0].clone()).expect("log event");
        match ev {
            AgentEvent::Log { message } => assert_eq!(message, "rate limited"),
            other => panic!("expected log event, got {other:?}"),
        }
    }
}
