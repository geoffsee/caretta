// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;

#[derive(Debug, Clone, Copy, Default)]
pub struct CopilotWrapper;

impl AgentCliAdapter for CopilotWrapper {
    fn binary(&self) -> &'static str {
        "copilot"
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

    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        vec!["--prompt".to_string(), prompt.to_string()]
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        match session_id {
            Some(id) => Some(vec![format!("--resume={id}")]),
            None => Some(vec!["--resume".to_string()]),
        }
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        Some(vec!["--yolo".to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec!["-p".to_string(), prompt.to_string()]
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (vec!["--model".to_string(), model.to_string()], Vec::new())
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::CopilotWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn builds_model_prompt_and_resume_args() {
        let wrapper = CopilotWrapper;
        assert_eq!(
            wrapper.model_args("gpt-5"),
            Some(vec!["--model".to_string(), "gpt-5".to_string()])
        );
        assert_eq!(
            wrapper.prompt_args("hello"),
            vec!["--prompt".to_string(), "hello".to_string()]
        );
        assert_eq!(
            wrapper.resume_args(Some("session-9")),
            Some(vec!["--resume=session-9".to_string()])
        );
    }

    #[test]
    fn native_run_uses_dash_p() {
        let wrapper = CopilotWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("go"),
            vec!["-p".to_string(), "go".to_string()]
        );
    }

    #[test]
    fn builds_resume_without_id() {
        let wrapper = CopilotWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["--resume".to_string()])
        );
    }
}
