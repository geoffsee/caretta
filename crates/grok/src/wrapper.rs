use agent_common::AgentCliAdapter;

#[derive(Debug, Clone, Copy, Default)]
pub struct GrokWrapper;

impl AgentCliAdapter for GrokWrapper {
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

    fn freqai_native_run_argv(&self, prompt: &str) -> Vec<String> {
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
    use std::process::Command;

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
            wrapper.freqai_native_run_argv("x"),
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

    #[test]
    fn grok_launch_path_propagates_not_found_for_absent_binary() {
        let wrapper = GrokWrapper;
        let mut argv = wrapper.freqai_native_run_argv("freq-ai launch smoke");
        argv.extend(wrapper.launch_auto_mode());
        let (model_args, model_env) = wrapper.launch_model_selection("smoke-model");
        argv.extend(model_args);

        assert_eq!(wrapper.binary(), "grok");
        assert!(!argv.is_empty(), "launch argv must be non-empty");
        assert_eq!(argv[0], "-p");
        assert!(argv.iter().any(|a| a == "--sandbox"));
        assert!(argv.iter().any(|a| a == "-m"));
        assert!(model_env.is_empty());

        let absent_binary = format!("{}-freq-ai-launch-smoke-absent", wrapper.binary());
        let err = Command::new(&absent_binary)
            .args(&argv)
            .spawn()
            .expect_err("spawn must fail when binary is absent");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
