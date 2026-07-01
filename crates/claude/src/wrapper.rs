// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::{AgentCliAdapter, claude_family_native_argv};
use cli_common::RichAction;

const CARETTA_CLAUDE_SYSTEM_PROMPT: &str = r#"You are caretta's autonomous repository agent.

Follow repository instructions from AGENTS.md, workflow prompts, tracker issues, and local status files when present.
Treat the user or workflow prompt as the source of task-specific scope.
When the task implies implementation, carry it through edits, verification, and a concise outcome report.
Make the smallest coherent code changes that complete the task, and preserve unrelated worktree changes.
Prefer existing project patterns and tools over new abstractions.
Run the most relevant verification commands available in the repository, and report failures or blockers plainly.
If required context is missing or instructions conflict, surface the blocker instead of guessing.
"#;

fn local_inference_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        "local".to_string()
    } else {
        trimmed.to_string()
    }
}

fn claude_like_launch_model_selection(model: &str) -> (Vec<String>, Vec<(String, String)>) {
    (vec!["--model".to_string(), model.to_string()], Vec::new())
}

fn claude_like_launch_local_inference(
    base_url: &str,
    api_key: &str,
    local_model: &str,
) -> (Vec<String>, Vec<(String, String)>) {
    let env = vec![
        ("ANTHROPIC_BASE_URL".to_string(), base_url.to_string()),
        (
            "ANTHROPIC_API_KEY".to_string(),
            local_inference_api_key(api_key),
        ),
    ];
    let mut args = Vec::new();
    if !local_model.trim().is_empty() {
        args.extend(["--model".to_string(), local_model.trim().to_string()]);
    }
    (args, env)
}

fn claude_family_parse_output_line(line: &str) -> Option<Vec<serde_json::Value>> {
    if let Ok(ev) = serde_json::from_str::<RichAction>(line) {
        Some(vec![serde_json::to_value(ev).ok()?])
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeWrapper;

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorWrapper;

impl AgentCliAdapter for ClaudeWrapper {
    fn binary(&self) -> &'static str {
        "claude"
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        vec!["--version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec!["--model".to_string(), model.to_string()])
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        let mut args = vec!["--resume".to_string()];
        if let Some(id) = session_id {
            args.push("--session-id".to_string());
            args.push(id.to_string());
        }
        Some(args)
    }

    fn system_prompt(&self) -> Option<&'static str> {
        Some(CARETTA_CLAUDE_SYSTEM_PROMPT)
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        claude_family_native_argv(prompt)
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_model_selection(model)
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--dangerously-skip-permissions".to_string()]
    }

    fn launch_local_inference(
        &self,
        base_url: &str,
        api_key: &str,
        local_model: &str,
    ) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_local_inference(base_url, api_key, local_model)
    }

    fn parse_output_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        claude_family_parse_output_line(line)
    }
}

impl AgentCliAdapter for CursorWrapper {
    fn binary(&self) -> &'static str {
        "cursor"
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        vec!["--version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec!["--model".to_string(), model.to_string()])
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        let mut args = vec!["--resume".to_string()];
        if let Some(id) = session_id {
            args.push("--session-id".to_string());
            args.push(id.to_string());
        }
        Some(args)
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        claude_family_native_argv(prompt)
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_model_selection(model)
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }

    fn parse_output_line(&self, line: &str) -> Option<Vec<serde_json::Value>> {
        claude_family_parse_output_line(line)
    }
}

#[cfg(test)]
mod tests {
    use super::{ClaudeWrapper, CursorWrapper};
    use agent_common::AgentCliAdapter;
    use agent_common::claude_family_native_argv;

    #[test]
    fn builds_model_and_native_argv() {
        let wrapper = ClaudeWrapper;
        assert_eq!(
            wrapper.model_args("opus"),
            Some(vec!["--model".to_string(), "opus".to_string()])
        );
        assert_eq!(
            wrapper.caretta_native_run_argv("hello"),
            claude_family_native_argv("hello")
        );
    }

    #[test]
    fn cursor_matches_claude_argv_and_binary_differs() {
        let c = ClaudeWrapper;
        let u = CursorWrapper;
        assert_eq!(
            c.caretta_native_run_argv("x"),
            u.caretta_native_run_argv("x")
        );
        assert_eq!(c.binary(), "claude");
        assert_eq!(u.binary(), "cursor");
        assert_eq!(
            c.launch_auto_mode(),
            vec!["--dangerously-skip-permissions".to_string()]
        );
        assert_eq!(u.launch_auto_mode(), vec!["--yolo".to_string()]);
    }

    #[test]
    fn builds_resume_with_and_without_session_id() {
        let wrapper = ClaudeWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["--resume".to_string()])
        );
        assert_eq!(
            wrapper.resume_args(Some("abc123")),
            Some(vec![
                "--resume".to_string(),
                "--session-id".to_string(),
                "abc123".to_string(),
            ])
        );
    }
}
