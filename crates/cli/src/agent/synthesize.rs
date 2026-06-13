//! Synthesize a [`DiscoveryWorkspace`] from the current working directory using
//! the configured agent.
//!
//! Powers the "Synthesize" button on the Discovery tab: the user clicks it,
//! the configured agent inspects the project root, and returns a JSON blob
//! that we parse into a [`DiscoveryWorkspace`].

use crate::agent::adapter_dispatch::{
    adapter_for_agent, caretta_native_command_with_prompt_transport,
};
use crate::agent::cmd::log;
use crate::agent::launch::{
    auto_mode_overrides, local_inference_overrides, merged_agent_env, model_selection_overrides,
};
use crate::agent::run::{native_command, spawn_sanitized_stderr_logger};
use crate::agent::types::{AgentEvent, Config, ContentBlock, RichAction};
use crate::ui::discovery::DiscoveryWorkspace;
use agent_common::{AgentCliAdapter, PromptTransport};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Stdio;

/// Build the prompt that asks the configured agent to inspect the working
/// directory and emit a Discovery workspace JSON object.
pub fn build_synthesis_prompt(root: &str) -> String {
    let schema = r#"{
  "problem": "string",
  "stakeholders": "string",
  "evidence": "string",
  "desired_outcome": "string",
  "constraints": "string",
  "dependencies": "string",
  "existing_systems": "string",
  "hypotheses": "string",
  "success_metrics": "string",
  "tradeoffs": "string",
  "risks": "string",
  "assumption_summary": "string",
  "frame_notes": "string",
  "decision": "string",
  "decision_rationale": "string",
  "risk_accepted": false,
  "risk_notes": "string",
  "assumptions": [
    {"status": "string", "confidence": "string", "evidence": "string", "owner": "string", "validation_next_step": "string"}
  ],
  "frame_comparisons": [
    {"frame": "string", "framing": "string", "evidence": "string", "tradeoffs": "string", "recommendation": "string"}
  ],
  "decision_log": [
    {"gate": "string", "rationale": "string", "rejected_alternatives": "string", "reversibility": "string"}
  ],
  "risk_dashboard": [
    {"likelihood": "string", "impact": "string", "trigger": "string", "mitigation": "string"}
  ],
  "dependency_graph": [
    {"from": "string", "to": "string", "reason": "string"}
  ]
}"#;

    format!(
        "You are inspecting the project located at `{root}` to populate a Discovery & Framing workspace for the team.\n\n\
         Investigate the working directory directly with your available tools: read top-level files such as README, AGENTS.md, CHARTER.md, COVENANT.md, Cargo.toml, package.json, manifest/config files, source layout, and any docs/ directory to understand the project's problem, stakeholders, constraints, outcomes, and risks.\n\n\
         When you have enough context, respond with **exactly one JSON object** (no markdown code fences, no prose, no commentary before or after) that matches this Discovery workspace schema:\n\
         {schema}\n\n\
         Rules:\n\
         - Output **only** the JSON object; nothing else.\n\
         - Use empty strings (\"\") or empty arrays ([]) for fields you cannot infer. Do not omit any field.\n\
         - `risk_accepted` is a boolean — use false when uncertain.\n\
         - Keep each string field concise (one short paragraph at most). Prefer concrete evidence drawn from files in the working directory."
    )
}

/// Locate and return the JSON object embedded in `text`, if any. Strips markdown
/// code fences and uses a balanced-brace scan to recover the first object that
/// is also valid JSON.
pub fn extract_workspace_json(text: &str) -> Option<String> {
    let cleaned = strip_code_fences(text);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(end) = scan_balanced_object(&bytes[i..])
        {
            let candidate = &cleaned[i..i + end];
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
        i += 1;
    }
    None
}

/// Parse the agent's output into a [`DiscoveryWorkspace`].
pub fn parse_workspace_from_text(text: &str) -> Result<DiscoveryWorkspace, String> {
    let json = extract_workspace_json(text)
        .ok_or_else(|| "No JSON object found in agent output.".to_string())?;
    serde_json::from_str::<DiscoveryWorkspace>(&json)
        .map_err(|err| format!("Failed to parse workspace JSON: {err}"))
}

/// Run the configured agent against `cfg.root` to synthesize a fresh
/// [`DiscoveryWorkspace`]. Blocks the caller; invoke from a worker thread.
pub fn synthesize_discovery_workspace(cfg: &Config) -> Result<DiscoveryWorkspace, String> {
    let root = cfg.root.trim();
    if root.is_empty() {
        return Err("Working directory is not configured.".to_string());
    }
    let prompt = build_synthesis_prompt(root);
    let (ok, output) = run_agent_capture(cfg, &prompt, Path::new(root))?;
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err(if ok {
            "Agent produced no output.".to_string()
        } else {
            "Agent invocation failed and produced no output.".to_string()
        });
    }
    match parse_workspace_from_text(&output) {
        Ok(ws) => Ok(ws),
        Err(err) if !ok => Err(format!("Agent invocation failed: {err}")),
        Err(err) => Err(err),
    }
}

fn run_agent_capture(cfg: &Config, prompt: &str, cwd: &Path) -> Result<(bool, String), String> {
    let env = merged_agent_env(cfg, &[]);
    let mut overrides = local_inference_overrides(cfg);
    overrides.args.extend(model_selection_overrides(cfg).args);
    overrides.args.extend(auto_mode_overrides(cfg).args);

    let spec = caretta_native_command_with_prompt_transport(cfg.agent, prompt, &overrides.args);
    let use_stdin = spec.prompt_transport == PromptTransport::Stdin;

    let mut cmd = native_command(&spec.command.binary, &spec.command.args);
    cmd.current_dir(cwd);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    if use_stdin {
        cmd.stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("Failed to spawn agent `{}`: {err}", spec.command.binary))?;
    let adapter = adapter_for_agent(cfg.agent);
    let stderr_log = spawn_sanitized_stderr_logger(
        &mut child,
        format!("{} stderr", spec.command.binary),
        Some(adapter),
    );

    if use_stdin
        && let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(prompt.as_bytes())
    {
        log(&format!(
            "synthesize: failed to send prompt via stdin: {err}"
        ));
    }

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = BufReader::new(stdout);
    let mut text = String::new();
    let adapter = adapter_for_agent(cfg.agent);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        append_line_text(trimmed, adapter, &mut text);
    }
    let ok = child
        .wait()
        .map(|status| status.success())
        .map_err(|err| format!("Agent process wait failed: {err}"))?;
    if let Some(handle) = stderr_log {
        let _ = handle.join();
    }
    Ok((ok, text))
}

fn append_line_text(line: &str, adapter: &dyn AgentCliAdapter, out: &mut String) {
    if let Some(values) = adapter.parse_output_line(line) {
        for v in values {
            let ev = if let Ok(ev) = serde_json::from_value::<AgentEvent>(v.clone()) {
                ev
            } else if let Ok(rev) = serde_json::from_value::<RichAction>(v) {
                AgentEvent::Rich(rev)
            } else {
                continue;
            };

            if let AgentEvent::Rich(r) = ev {
                accumulate_rich(&r, out);
            }
        }
        return;
    }

    // Fallback: agents that print raw text get appended verbatim so the JSON
    // extractor can still find their output.
    out.push_str(line);
    out.push('\n');
}

fn accumulate_rich(ev: &RichAction, out: &mut String) {
    match ev {
        RichAction::Assistant { message } => {
            for block in &message.content {
                if let ContentBlock::Text { text } = block {
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
        RichAction::ContentBlockDelta { delta, .. } => {
            if let Some(text) = &delta.text {
                out.push_str(text);
            }
        }
        _ => {}
    }
}

fn strip_code_fences(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_fence = false;
    for line in s.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn scan_balanced_object(bytes: &[u8]) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
                continue;
            }
            match b {
                b'\\' => esc = true,
                b'"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_mentions_root_and_required_fields() {
        let prompt = build_synthesis_prompt("/tmp/some/project");
        assert!(prompt.contains("/tmp/some/project"));
        for field in [
            "problem",
            "stakeholders",
            "evidence",
            "desired_outcome",
            "assumptions",
            "frame_comparisons",
            "decision_log",
            "risk_dashboard",
            "dependency_graph",
            "risk_accepted",
        ] {
            assert!(
                prompt.contains(field),
                "synthesis prompt should mention field `{field}`"
            );
        }
        // The prompt must instruct the agent to emit JSON only.
        assert!(prompt.to_lowercase().contains("json"));
    }

    #[test]
    fn extracts_bare_json_object() {
        let raw = r#"{"problem":"x","stakeholders":"y"}"#;
        let extracted = extract_workspace_json(raw).expect("should find JSON");
        assert_eq!(extracted, raw);
    }

    #[test]
    fn extracts_json_inside_code_fence() {
        let raw = "Here is the answer:\n```json\n{\"problem\":\"x\"}\n```\nthanks!";
        let extracted = extract_workspace_json(raw).expect("should find JSON");
        assert_eq!(extracted, "{\"problem\":\"x\"}");
    }

    #[test]
    fn extracts_json_with_nested_objects_and_strings() {
        let raw = r#"prefix {"a":{"b":"c}{"}} suffix {"x":1}"#;
        let extracted = extract_workspace_json(raw).expect("should find JSON");
        assert_eq!(extracted, r#"{"a":{"b":"c}{"}}"#);
    }

    #[test]
    fn returns_none_when_no_object_present() {
        assert!(extract_workspace_json("no json here").is_none());
        assert!(extract_workspace_json("{ unterminated").is_none());
    }

    #[test]
    fn parses_full_workspace_from_text_with_chatter() {
        let raw = r#"
            Sure, here is what I found:
            ```json
            {
                "problem": "Caretta automates discovery",
                "stakeholders": "engineering leads",
                "evidence": "",
                "desired_outcome": "",
                "constraints": "",
                "dependencies": "",
                "existing_systems": "",
                "hypotheses": "",
                "success_metrics": "",
                "tradeoffs": "",
                "risks": "",
                "assumption_summary": "",
                "frame_notes": "",
                "decision": "",
                "decision_rationale": "",
                "risk_accepted": false,
                "risk_notes": "",
                "assumptions": [],
                "frame_comparisons": [],
                "decision_log": [],
                "risk_dashboard": [],
                "dependency_graph": []
            }
            ```
        "#;
        let ws = parse_workspace_from_text(raw).expect("parse workspace");
        assert_eq!(ws.problem, "Caretta automates discovery");
        assert_eq!(ws.stakeholders, "engineering leads");
        assert!(!ws.risk_accepted);
        assert!(ws.assumptions.is_empty());
    }

    #[test]
    fn parse_workspace_falls_back_to_defaults_for_missing_fields() {
        // Schema uses `#[serde(default)]`, so partial objects still parse.
        let raw = r#"{"problem": "only this field"}"#;
        let ws = parse_workspace_from_text(raw).expect("partial parse");
        assert_eq!(ws.problem, "only this field");
        assert!(ws.stakeholders.is_empty());
        assert!(ws.assumptions.is_empty());
    }

    #[test]
    fn parse_workspace_fails_on_invalid_json() {
        let err = parse_workspace_from_text("totally not json").unwrap_err();
        assert!(err.contains("No JSON object"), "unexpected error: {err}");
    }
}
