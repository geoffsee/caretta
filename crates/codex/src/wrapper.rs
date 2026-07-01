// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::{AgentCliAdapter, PromptTransport};
use cli_common::{AgentEvent, AssistantMessage, ContentBlock, ContentBlockDelta, RichAction};

fn local_inference_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        "local".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CodexWrapper;

impl AgentCliAdapter for CodexWrapper {
    fn binary(&self) -> &'static str {
        "codex"
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        vec!["--version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec!["-c".to_string(), format!("model={model:?}")])
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        let mut args = vec!["resume".to_string()];
        if let Some(id) = session_id {
            args.push(id.to_string());
        }
        Some(args)
    }

    fn project_args(&self, project: &str) -> Option<Vec<String>> {
        Some(vec!["--cd".to_string(), project.to_string()])
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        Some(vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
        ])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec!["exec".to_string(), "--json".to_string(), prompt.to_string()]
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (
            vec!["-c".to_string(), format!("model={model:?}")],
            Vec::new(),
        )
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--dangerously-bypass-approvals-and-sandbox".to_string()]
    }

    fn launch_local_inference(
        &self,
        base_url: &str,
        api_key: &str,
        local_model: &str,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let env = vec![
            ("OPENAI_BASE_URL".to_string(), base_url.to_string()),
            (
                "OPENAI_API_KEY".to_string(),
                local_inference_api_key(api_key),
            ),
        ];
        let mut args = vec!["-c".to_string(), format!("openai_base_url={base_url:?}")];
        if !local_model.trim().is_empty() {
            args.extend(["--model".to_string(), local_model.trim().to_string()]);
        }
        (args, env)
    }

    fn prompt_transport(&self) -> PromptTransport {
        PromptTransport::Stdin
    }

    fn system_prompt(&self) -> Option<&'static str> {
        None
    }

    fn launch_env(&self) -> Vec<(String, String)> {
        // CI sets RUST_LOG=info for caretta; inherited env makes codex emit verbose OTEL spans.
        vec![("RUST_LOG".to_string(), "error".to_string())]
    }

    fn should_log_stderr_line(&self, _line: &str) -> bool {
        // Codex stderr is internal tracing/HTML noise; structured stdout (--json) carries signal.
        false
    }

    fn parse_stderr_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        // Detect session expiration or Cloudflare blocks without forwarding raw stderr.
        if line.contains("403 Forbidden")
            || line.contains("Enable JavaScript and cookies to continue")
            || line.contains("http-equiv=\"refresh\"")
        {
            return Some(vec![serde_json::to_value(AgentEvent::Log {
                message: "Codex session expired or blocked by Cloudflare (403 Forbidden). Please re-authenticate by running 'codex login'.".to_string(),
            })
            .ok()?]);
        }
        None
    }

    fn parse_output_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let event_type = v.get("type")?.as_str()?;

        let mut out = Vec::new();
        match event_type {
            "thread.started" => {
                let description = v
                    .get("thread_id")
                    .and_then(serde_json::Value::as_str)
                    .map(|id| format!("Thread {id}"));
                out.push(
                    serde_json::to_value(RichAction::System {
                        subtype: "thread_started".to_string(),
                        model: Some("codex".to_string()),
                        description,
                        session_id: None,
                        claude_code_version: None,
                        tools: None,
                    })
                    .ok()?,
                );
            }
            "turn.started" => {
                out.push(
                    serde_json::to_value(RichAction::System {
                        subtype: "turn_started".to_string(),
                        model: Some("codex".to_string()),
                        description: None,
                        session_id: None,
                        claude_code_version: None,
                        tools: None,
                    })
                    .ok()?,
                );
            }
            "item.started" | "item.completed" => {
                let is_completed = event_type == "item.completed";
                let Some(item) = v.get("item").and_then(serde_json::Value::as_object) else {
                    return Some(out);
                };
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
                                if let Some(text) =
                                    c.get("text").and_then(serde_json::Value::as_str)
                                    && !is_completed
                                    && let AgentEvent::Rich(ev) =
                                        assistant_text_event(text.to_string())
                                {
                                    out.push(serde_json::to_value(ev).ok()?);
                                }
                            }
                        }
                    }
                    "tool_call" => {
                        if let Some(call) = item.get("call").and_then(serde_json::Value::as_object)
                        {
                            let name = call
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            let args = call
                                .get("arguments")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            if !is_completed
                                && let AgentEvent::Rich(ev) =
                                    assistant_block_event(ContentBlock::ToolUse {
                                        id: "codex_tool".to_string(),
                                        name: name.to_string(),
                                        input: serde_json::from_str(args)
                                            .unwrap_or(serde_json::json!({})),
                                    })
                            {
                                out.push(serde_json::to_value(ev).ok()?);
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
                    out.push(
                        serde_json::to_value(RichAction::ContentBlockDelta {
                            index: 0,
                            delta: ContentBlockDelta {
                                delta_type: "text_delta".to_string(),
                                text: Some(text.to_string()),
                            },
                        })
                        .ok()?,
                    );
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
            _ => {}
        }
        Some(out)
    }
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

fn assistant_text_event(text: String) -> AgentEvent {
    AgentEvent::Rich(RichAction::Assistant {
        message: AssistantMessage {
            content: vec![ContentBlock::Text { text }],
        },
    })
}

fn assistant_block_event(block: ContentBlock) -> AgentEvent {
    AgentEvent::Rich(RichAction::Assistant {
        message: AssistantMessage {
            content: vec![block],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::CodexWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn builds_prompt_model_and_project_args() {
        let wrapper = CodexWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("ship it"),
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "ship it".to_string()
            ]
        );
        assert_eq!(
            wrapper.model_args("gpt-5.4"),
            Some(vec!["-c".to_string(), format!("model={:?}", "gpt-5.4")])
        );
        assert_eq!(
            wrapper.project_args("/tmp/work"),
            Some(vec!["--cd".to_string(), "/tmp/work".to_string()])
        );
    }

    #[test]
    fn builds_resume_with_and_without_id() {
        let wrapper = CodexWrapper;
        assert_eq!(wrapper.resume_args(None), Some(vec!["resume".to_string()]));
        assert_eq!(
            wrapper.resume_args(Some("thread_123")),
            Some(vec!["resume".to_string(), "thread_123".to_string()])
        );
    }

    #[test]
    fn launch_env_caps_codex_rust_log_at_error() {
        let wrapper = CodexWrapper;
        assert_eq!(
            wrapper.launch_env(),
            vec![("RUST_LOG".to_string(), "error".to_string())]
        );
    }

    #[test]
    fn never_forwards_codex_stderr_to_caretta_logs() {
        let wrapper = CodexWrapper;
        assert!(!wrapper.should_log_stderr_line("ERROR codex: auth token expired"));
        assert!(!wrapper.should_log_stderr_line("<head>"));
        assert!(!wrapper.should_log_stderr_line(
            r#"2026-06-13T20:23:32.740217Z  INFO session_loop: codex_otel.trace_safe"#
        ));
    }
}
