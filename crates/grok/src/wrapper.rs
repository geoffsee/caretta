use agent_common::{AdapterCapabilities, AgentCliAdapter};

#[derive(Debug, Clone, Copy, Default)]
pub struct GrokWrapper;

impl AgentCliAdapter for GrokWrapper {
    fn binary(&self) -> &'static str {
        "grok"
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
    use super::GrokWrapper;
    use agent_common::AgentCliAdapter;

    #[test]
    fn builds_model_prompt_and_project_args() {
        let wrapper = GrokWrapper;
        assert_eq!(
            wrapper.model_args("grok-4"),
            Some(vec!["-m".to_string(), "grok-4".to_string()])
        );
        assert_eq!(
            wrapper.prompt_args("diff this"),
            vec!["--prompt".to_string(), "diff this".to_string()]
        );
        assert_eq!(
            wrapper.project_args("/tmp/proj"),
            Some(vec!["--directory".to_string(), "/tmp/proj".to_string()])
        );
    }

    #[test]
    fn native_run_uses_dash_p() {
        let wrapper = GrokWrapper;
        assert_eq!(
            wrapper.caretta_native_run_argv("x"),
            vec!["-p".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn resume_is_not_supported() {
        let wrapper = GrokWrapper;
        assert_eq!(wrapper.resume_args(None), None);
        assert_eq!(wrapper.resume_args(Some("x")), None);
    }

    #[test]
    fn version_uses_flag() {
        let wrapper = GrokWrapper;
        assert_eq!(wrapper.version_args(), vec!["--version".to_string()]);
    }
}
