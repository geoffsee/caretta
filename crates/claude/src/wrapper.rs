use agent_common::{AgentCliAdapter, claude_family_native_argv};

fn local_inference_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        "local".to_string()
    } else {
        trimmed.to_string()
    }
}

fn claude_like_launch_model_selection(model: &str) -> (Vec<String>, Vec<(String, String)>) {
    (vec!["--model".to_string(), model.to_string()], Vec::new())
}

fn claude_like_launch_local_inference(
    base_url: &str,
    api_key: &str,
    local_model: &str,
) -> (Vec<String>, Vec<(String, String)>) {
    let env = vec![
        ("ANTHROPIC_BASE_URL".to_string(), base_url.to_string()),
        (
            "ANTHROPIC_API_KEY".to_string(),
            local_inference_api_key(api_key),
        ),
    ];
    let mut args = Vec::new();
    if !local_model.trim().is_empty() {
        args.extend(["--model".to_string(), local_model.trim().to_string()]);
    }
    (args, env)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeWrapper;

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorWrapper;

impl AgentCliAdapter for ClaudeWrapper {
    fn binary(&self) -> &'static str {
        "claude"
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

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn freqai_native_run_argv(&self, prompt: &str) -> Vec<String> {
        claude_family_native_argv(prompt)
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_model_selection(model)
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--dangerously-skip-permissions".to_string()]
    }

    fn launch_local_inference(
        &self,
        base_url: &str,
        api_key: &str,
        local_model: &str,
    ) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_local_inference(base_url, api_key, local_model)
    }
}

impl AgentCliAdapter for CursorWrapper {
    fn binary(&self) -> &'static str {
        "cursor"
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

    fn output_format_args(&self, format: &str) -> Option<Vec<String>> {
        Some(vec!["--output-format".to_string(), format.to_string()])
    }

    fn freqai_native_run_argv(&self, prompt: &str) -> Vec<String> {
        claude_family_native_argv(prompt)
    }

    fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
        claude_like_launch_model_selection(model)
    }

    fn launch_auto_mode(&self) -> Vec<String> {
        vec!["--yolo".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::{ClaudeWrapper, CursorWrapper};
    use agent_common::AgentCliAdapter;
    use agent_common::claude_family_native_argv;

    #[test]
    fn builds_model_and_native_argv() {
        let wrapper = ClaudeWrapper;
        assert_eq!(
            wrapper.model_args("opus"),
            Some(vec!["--model".to_string(), "opus".to_string()])
        );
        assert_eq!(
            wrapper.freqai_native_run_argv("hello"),
            claude_family_native_argv("hello")
        );
    }

    #[test]
    fn cursor_matches_claude_argv_and_binary_differs() {
        let c = ClaudeWrapper;
        let u = CursorWrapper;
        assert_eq!(c.freqai_native_run_argv("x"), u.freqai_native_run_argv("x"));
        assert_eq!(c.binary(), "claude");
        assert_eq!(u.binary(), "cursor");
        assert_eq!(
            c.launch_auto_mode(),
            vec!["--dangerously-skip-permissions".to_string()]
        );
        assert_eq!(u.launch_auto_mode(), vec!["--yolo".to_string()]);
    }

    #[test]
    fn builds_resume_with_and_without_session_id() {
        let wrapper = ClaudeWrapper;
        assert_eq!(
            wrapper.resume_args(None),
            Some(vec!["--resume".to_string()])
        );
        assert_eq!(
            wrapper.resume_args(Some("abc123")),
            Some(vec![
                "--resume".to_string(),
                "--session-id".to_string(),
                "abc123".to_string(),
            ])
        );
    }

    #[test]
    fn claude_launch_path_propagates_not_found_for_absent_binary() {
        use std::process::Command;

        let wrapper = ClaudeWrapper;
        let mut argv = wrapper.freqai_native_run_argv("freq-ai launch smoke");
        argv.extend(wrapper.launch_auto_mode());
        let (model_args, model_env) = wrapper.launch_model_selection("smoke-model");
        argv.extend(model_args);
        let (local_args, local_env) =
            wrapper.launch_local_inference("http://127.0.0.1:0", "", "smoke-local");
        argv.extend(local_args);

        assert_eq!(wrapper.binary(), "claude");
        assert!(!argv.is_empty(), "launch argv must be non-empty");
        assert!(argv.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(model_env.is_empty());
        assert!(
            local_env
                .iter()
                .any(|(k, _)| k == "ANTHROPIC_BASE_URL" || k == "ANTHROPIC_API_KEY")
        );

        let absent_binary = format!("{}-freq-ai-launch-smoke-absent", wrapper.binary());
        let err = Command::new(&absent_binary)
            .args(&argv)
            .envs(local_env)
            .spawn()
            .expect_err("spawn must fail when binary is absent");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn cursor_launch_path_propagates_not_found_for_absent_binary() {
        use std::process::Command;

        let wrapper = CursorWrapper;
        let mut argv = wrapper.freqai_native_run_argv("freq-ai launch smoke");
        argv.extend(wrapper.launch_auto_mode());
        let (model_args, model_env) = wrapper.launch_model_selection("smoke-model");
        argv.extend(model_args);

        assert_eq!(wrapper.binary(), "cursor");
        assert!(!argv.is_empty(), "launch argv must be non-empty");
        assert!(argv.iter().any(|a| a == "--yolo"));
        assert!(model_env.is_empty());

        let absent_binary = format!("{}-freq-ai-launch-smoke-absent", wrapper.binary());
        let err = Command::new(&absent_binary)
            .args(&argv)
            .spawn()
            .expect_err("spawn must fail when binary is absent");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
