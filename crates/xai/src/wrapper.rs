use agent_common::AgentCliAdapter;

#[derive(Debug, Clone, Copy, Default)]
pub struct XaiWrapper;

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

    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
        vec!["-p".to_string(), prompt.to_string()]
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (vec!["-m".to_string(), model.to_string()], Vec::new())
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--sandbox".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::XaiWrapper;
    use agent_common::AgentCliAdapter;

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
    fn native_run_uses_dash_p() {
        let wrapper = XaiWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("go"),
            vec!["-p".to_string(), "go".to_string()]
        );
    }

    #[test]
    fn version_uses_flag() {
        let wrapper = XaiWrapper;
        assert_eq!(wrapper.version_args(), vec!["--version".to_string()]);
    }
}
