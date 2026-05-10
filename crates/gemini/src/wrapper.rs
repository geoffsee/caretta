use agent_common::{AdapterCapabilities, AgentCliAdapter};

#[derive(Debug, Clone, Copy, Default)]
pub struct GeminiWrapper;

impl AgentCliAdapter for GeminiWrapper {
    fn binary(&self) -> &'static str {
        "gemini"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            tool_use: true,
            vision: true,
            streaming: true,
            context_window: Some(1_000_000),
        }
    }

    fn help_args(&self) -> Vec<String> {
        vec!["--help".to_string()]
    }

    fn version_args(&self) -> Vec<String> {
        vec!["--version".to_string()]
    }

    fn model_args(&self, model: &str) -> Option<Vec<String>> {
        Some(vec!["-m".to_string(), model.to_string()])
    }

    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        vec!["--prompt".to_string(), prompt.to_string()]
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>> {
        if session_id.is_some() {
            return None;
        }
        Some(vec!["--resume".to_string()])
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
        (vec!["-m".to_string(), model.to_string()], Vec::new())
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::GeminiWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn builds_model_prompt_and_output_format_args() {
        let wrapper = GeminiWrapper;
        assert_eq!(
            wrapper.model_args("gemini-2.5-pro"),
            Some(vec!["-m".to_string(), "gemini-2.5-pro".to_string(),])
        );
        assert_eq!(
            wrapper.prompt_args("summarize this"),
            vec!["--prompt".to_string(), "summarize this".to_string()]
        );
        assert_eq!(
            wrapper.output_format_args("json"),
            Some(vec!["--output-format".to_string(), "json".to_string()])
        );
    }

    #[test]
    fn native_run_uses_dash_p() {
        let wrapper = GeminiWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("x"),
            vec!["-p".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn resume_requires_provider_managed_session_ids() {
        let wrapper = GeminiWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["--resume".to_string()])
        );
        assert_eq!(wrapper.resume_args(Some("ignored")), None);
    }
}
