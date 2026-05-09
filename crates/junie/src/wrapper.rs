use agent_common::{AgentCliAdapter, claude_family_native_argv};

#[derive(Debug, Clone, Copy, Default)]
pub struct JunieWrapper;

impl AgentCliAdapter for JunieWrapper {
    fn binary(&self) -> &'static str {
        "junie"
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

    fn project_args(&self, project: &str) -> Option<Vec<String>> {
        Some(vec!["--project".to_string(), project.to_string()])
    }

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        Some(vec!["--brave".to_string()])
    }

    fn freqai_native_run_argv(&self, prompt: &str) -> Vec<String> {
        claude_family_native_argv(prompt)
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (vec!["--model".to_string(), model.to_string()], Vec::new())
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--brave".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::JunieWrapper;
    use agent_common::AgentCliAdapter;
    use agent_common::claude_family_native_argv;
    use std::process::Command;

    #[test]
    fn builds_model_prompt_project_and_output_args() {
        let wrapper = JunieWrapper;
        assert_eq!(
            wrapper.model_args("junie-pro"),
            Some(vec!["--model".to_string(), "junie-pro".to_string()])
        );
        assert_eq!(
            wrapper.freqai_native_run_argv("write tests"),
            claude_family_native_argv("write tests")
        );
        assert_eq!(
            wrapper.project_args("/tmp/proj"),
            Some(vec!["--project".to_string(), "/tmp/proj".to_string()])
        );
        assert_eq!(
            wrapper.output_format_args("json"),
            Some(vec!["--output-format".to_string(), "json".to_string()])
        );
    }

    #[test]
    fn builds_resume_with_and_without_session_id() {
        let wrapper = JunieWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["--resume".to_string()])
        );
        assert_eq!(
            wrapper.resume_args(Some("s-1")),
            Some(vec![
                "--resume".to_string(),
                "--session-id".to_string(),
                "s-1".to_string(),
            ])
        );
    }

    #[test]
    fn junie_launch_path_propagates_not_found_for_absent_binary() {
        let wrapper = JunieWrapper;
        let mut argv = wrapper.freqai_native_run_argv("freq-ai launch smoke");
        argv.extend(wrapper.launch_auto_mode());
        let (model_args, model_env) = wrapper.launch_model_selection("smoke-model");
        argv.extend(model_args);

        assert_eq!(wrapper.binary(), "junie");
        assert!(!argv.is_empty(), "launch argv must be non-empty");
        assert!(argv.iter().any(|a| a == "--brave"));
        assert!(argv.iter().any(|a| a == "--model"));
        assert!(model_env.is_empty());

        let absent_binary = format!("{}-freq-ai-launch-smoke-absent", wrapper.binary());
        let err = Command::new(&absent_binary)
            .args(&argv)
            .spawn()
            .expect_err("spawn must fail when binary is absent");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
