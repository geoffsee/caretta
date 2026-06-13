/// argv shape used by Claude Code, Junie, and Cursor in caretta's native runner.
pub fn claude_family_native_argv(prompt: &str) -> Vec<String> {
    vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCliCommand {
    pub binary: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentInvocation {
    Help,
    Version,
    Model(String),
    Prompt(String),
    Resume(Option<String>),
    Project(String),
    OutputFormat(String),
    Yolo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTransport {
    Argv,
    Stdin,
}

pub trait AgentCliAdapter: Send + Sync {
    fn binary(&self) -> &'static str;

    fn help_args(&self) -> Vec<String>;

    fn version_args(&self) -> Vec<String>;

    fn model_args(&self, model: &str) -> Option<Vec<String>>;

    /// Alternate prompt argv shape for compatibility tooling or tests.
    /// `command_for(AgentInvocation::Prompt)` uses [`Self::caretta_native_run_argv`] only,
    /// not `prompt_args`, so the caretta runner matches native `run_agent` argv.
    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        self.caretta_native_run_argv(prompt)
    }

    /// Appended system prompt guidance for this agent.
    fn system_prompt(&self) -> Option<&'static str> {
        None
    }

    fn resume_args(&self, session_id: Option<&str>) -> Option<Vec<String>>;

    fn project_args(&self, _project: &str) -> Option<Vec<String>> {
        None
    }

    fn output_format_args(&self, _format: &str) -> Option<Vec<String>> {
        None
    }

    fn yolo_args(&self) -> Option<Vec<String>> {
        None
    }

    /// Base argv for caretta's native runner (`run_agent` / `run_agent_with_env`),
    /// before [`Self::launch_model_selection`], [`Self::launch_auto_mode`], and
    /// [`Self::launch_local_inference`] fragments are appended.
    fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String>;

    /// Model selection from the caretta UI (`caretta.toml` / config). `model` is non-empty.
    fn launch_model_selection(&self, _model: &str) -> (Vec<String>, Vec<(String, String)>) {
        (Vec::new(), Vec::new())
    }

    /// `auto_mode` / permission-bypass flags for the native run argv.
    fn launch_auto_mode(&self) -> Vec<String> {
        vec![]
    }

    /// Local inference (advanced) fragments when base URL is set. `local_model` may be empty.
    fn launch_local_inference(
        &self,
        _base_url: &str,
        _api_key: &str,
        _local_model: &str,
    ) -> (Vec<String>, Vec<(String, String)>) {
        (Vec::new(), Vec::new())
    }

    /// Preferred prompt transport for the native runner. Defaults to Argv.
    fn prompt_transport(&self) -> PromptTransport {
        PromptTransport::Argv
    }

    /// Parse a single line of stdout into structured JSON events.
    /// Used by the native runner to handle agent-specific output formats.
    fn parse_output_line(&self, _line: &str) -> Option<Vec<serde_json::Value>> {
        None
    }

    /// Parse a single line of stderr into structured JSON events.
    /// Used by the native runner to handle agent-specific error conditions or status updates.
    fn parse_stderr_line(&self, _line: &str) -> Option<Vec<serde_json::Value>> {
        None
    }

    fn command_for(&self, invocation: AgentInvocation) -> Option<AgentCliCommand> {
        let args = match invocation {
            AgentInvocation::Help => Some(self.help_args()),
            AgentInvocation::Version => Some(self.version_args()),
            AgentInvocation::Model(model) => self.model_args(&model),
            AgentInvocation::Prompt(prompt) => Some(self.caretta_native_run_argv(&prompt)),
            AgentInvocation::Resume(session_id) => self.resume_args(session_id.as_deref()),
            AgentInvocation::Project(project) => self.project_args(&project),
            AgentInvocation::OutputFormat(format) => self.output_format_args(&format),
            AgentInvocation::Yolo => self.yolo_args(),
        }?;

        Some(AgentCliCommand {
            binary: self.binary().to_string(),
            args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentCliAdapter, AgentInvocation};

    #[derive(Debug, Clone, Copy)]
    struct MockAdapter;

    impl AgentCliAdapter for MockAdapter {
        fn binary(&self) -> &'static str {
            "mock-cli"
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

        fn caretta_native_run_argv(&self, prompt: &str) -> Vec<String> {
            vec!["--prompt".to_string(), prompt.to_string()]
        }

        fn launch_model_selection(&self, model: &str) -> (Vec<String>, Vec<(String, String)>) {
            (vec!["--model".to_string(), model.to_string()], Vec::new())
        }

        fn launch_auto_mode(&self) -> Vec<String> {
            vec!["--yolo".to_string()]
        }

        fn launch_local_inference(
            &self,
            _base_url: &str,
            _api_key: &str,
            _local_model: &str,
        ) -> (Vec<String>, Vec<(String, String)>) {
            (Vec::new(), Vec::new())
        }

        fn resume_args(&self, _session_id: Option<&str>) -> Option<Vec<String>> {
            None
        }
    }

    #[test]
    fn command_for_maps_supported_invocations() {
        let adapter = MockAdapter;
        let cmd = adapter
            .command_for(AgentInvocation::Prompt("hello".to_string()))
            .expect("prompt should be supported");
        assert_eq!(cmd.binary, "mock-cli");
        assert_eq!(cmd.args, vec!["--prompt".to_string(), "hello".to_string()]);
    }

    #[test]
    fn command_for_returns_none_for_unsupported_invocations() {
        let adapter = MockAdapter;
        assert_eq!(adapter.command_for(AgentInvocation::Resume(None)), None);
    }
}
