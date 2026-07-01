// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;

/// Stable argv shapes for integration tests; pairs with the `caretta-dummy-agent` binary.
#[derive(Debug, Clone, Copy, Default)]
pub struct DummyAgentWrapper;

impl AgentCliAdapter for DummyAgentWrapper {
    fn binary(&self) -> &'static str {
        "caretta-dummy-agent"
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
        let mut args = vec!["resume".to_string()];
        if let Some(id) = session_id {
            args.push(id.to_string());
        }
        Some(args)
    }

    fn project_args(&self, project: &str) -> Option<Vec<String>> {
        Some(vec!["--project".to_string(), project.to_string()])
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        Some(vec!["--yolo".to_string()])
    }

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec!["exec".to_string(), "--json".to_string(), prompt.to_string()]
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
    use super::DummyAgentWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn argv_shapes_match_binary_contract() {
        let w = DummyAgentWrapper;
        assert_eq!(w.binary(), "caretta-dummy-agent");
        assert_eq!(w.help_args(), vec!["--help".to_string()]);
        assert_eq!(w.version_args(), vec!["--version".to_string()]);
        assert_eq!(
            w.caretta_native_run_argv("hi"),
            vec!["exec".to_string(), "--json".to_string(), "hi".to_string()]
        );
    }
}
