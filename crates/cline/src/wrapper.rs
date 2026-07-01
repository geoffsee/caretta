// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;

#[derive(Debug, Clone, Copy, Default)]
pub struct ClineWrapper;

impl AgentCliAdapter for ClineWrapper {
    fn binary(&self) -> &'static str {
        "cline"
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        vec!["version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec![
            "auth".to_string(),
            "-m".to_string(),
            model.to_string(),
        ])
    }

    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        vec![prompt.to_string()]
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        let mut args = vec!["task".to_string(), "open".to_string()];
        if let Some(id) = session_id {
            args.push(id.to_string());
        }
        Some(args)
    }

    fn project_args(&self, project: &str) -> Option<Vec<String>> {
        Some(vec!["--workspace".to_string(), project.to_string()])
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["-F".to_string(), format.to_string()])
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        Some(vec!["--yolo".to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec!["chat".to_string(), prompt.to_string()]
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::ClineWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn builds_prompt_model_and_version_args() {
        let wrapper = ClineWrapper;
        assert_eq!(wrapper.prompt_args("hello"), vec!["hello".to_string()]);
        assert_eq!(
            wrapper.model_args("sonnet"),
            Some(vec![
                "auth".to_string(),
                "-m".to_string(),
                "sonnet".to_string()
            ])
        );
        assert_eq!(wrapper.version_args(), vec!["version".to_string()]);
    }

    #[test]
    fn native_run_uses_chat_subcommand() {
        let wrapper = ClineWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("hi"),
            vec!["chat".to_string(), "hi".to_string()]
        );
    }

    #[test]
    fn builds_resume_with_and_without_id() {
        let wrapper = ClineWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["task".to_string(), "open".to_string()])
        );
        assert_eq!(
            wrapper.resume_args(Some("session-42")),
            Some(vec![
                "task".to_string(),
                "open".to_string(),
                "session-42".to_string(),
            ])
        );
    }
}
