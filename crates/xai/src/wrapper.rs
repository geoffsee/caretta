use agent_common::{AdapterCapabilities, AgentCliAdapter};

#[derive(Debug, Clone, Copy, Default)]
pub struct XaiWrapper;

impl AgentCliAdapter for XaiWrapper {
    fn binary(&self) -> &'static str {
        "copilot"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            tool_use: true,
            vision: true,
            streaming: true,
            context_window: Some(131_072),
        }
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
        (
            Vec::new(),
            vec![("COPILOT_MODEL".to_string(), model.to_string())],
        )
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }
}

impl XaiWrapper {
    pub fn env_overrides_for_xai() -> &'static [(&'static str, &'static str)] {
        &[("XAI_BASE_URL", "https://api.x.ai/v1")]
    }
}

#[cfg(test)]
mod tests {
    use super::XaiWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn uses_copilot_binary_with_xai_flag_mapping() {
        let wrapper = XaiWrapper;
        assert_eq!(wrapper.binary(), "copilot");
        assert_eq!(
            wrapper.prompt_args("hello"),
            vec!["--prompt".to_string(), "hello".to_string()]
        );
        assert_eq!(
            wrapper.resume_args(Some("x1")),
            Some(vec!["--resume=x1".to_string()])
        );
    }

    #[test]
    fn native_run_uses_dash_p() {
        let wrapper = XaiWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("go"),
            vec!["-p".to_string(), "go".to_string()]
        );
    }

    #[test]
    fn exposes_xai_specific_env_overrides() {
        assert_eq!(
            XaiWrapper::env_overrides_for_xai(),
            &[("XAI_BASE_URL", "https://api.x.ai/v1")]
        );
    }
}
