use agent_common::{AdapterCapabilities, AgentCliAdapter};

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

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            tool_use: false,
            vision: false,
            streaming: true,
            context_window: Some(128_000),
        }
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
}
