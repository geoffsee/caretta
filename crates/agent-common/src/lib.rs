/// Declared capabilities of an adapter CLI, returned by [`AgentCliAdapter::capabilities`].
///
/// Fields follow a conservative-default policy: unknown adapters are assumed to lack
/// optional capabilities. Override per adapter to surface accurate values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCapabilities {
    /// Whether the underlying model exposes tool/function-call support.
    pub tool_use: bool,
    /// Whether the underlying model accepts image inputs.
    pub vision: bool,
    /// Whether the adapter streams output tokens incrementally.
    pub streaming: bool,
    /// Maximum context window in tokens, if known.
    pub context_window: Option<u32>,
}

impl Default for AdapterCapabilities {
    fn default() -> Self {
        Self {
            tool_use: true,
            vision: false,
            streaming: true,
            context_window: None,
        }
    }
}

/// Requirements a workflow may declare against an adapter's capabilities.
///
/// All fields default to `false`/`None` (no requirement). Workflows that need
/// specific capabilities declare them in their YAML `requires_capabilities` block
/// so caretta can emit a clear pre-run error instead of a silent runtime failure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowCapabilityRequirements {
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    /// Minimum context window in tokens required by the workflow.
    pub min_context_window: Option<u32>,
}

impl WorkflowCapabilityRequirements {
    /// Returns `Ok(())` when `caps` satisfies every declared requirement, or
    /// an `Err` message listing the missing capabilities.
    pub fn check(&self, caps: &AdapterCapabilities, adapter_name: &str) -> Result<(), String> {
        let mut missing = Vec::new();
        if self.tool_use && !caps.tool_use {
            missing.push("tool_use");
        }
        if self.vision && !caps.vision {
            missing.push("vision");
        }
        if self.streaming && !caps.streaming {
            missing.push("streaming");
        }
        if let Some(min) = self.min_context_window
            && caps.context_window.is_none_or(|w| w < min)
        {
            missing.push("context_window");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "adapter '{adapter_name}' does not support required capabilities: {}",
                missing.join(", ")
            ))
        }
    }
}

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

pub trait AgentCliAdapter {
    fn binary(&self) -> &'static str;

    /// Declare the capabilities this adapter exposes.
    ///
    /// Caretta uses the returned value for `--list-adapters` output and for
    /// pre-run capability checks when a workflow declares requirements.
    /// The default is conservative: tool_use=true, vision=false, streaming=true,
    /// context_window=None.  Override per adapter to surface accurate values.
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::default()
    }

    fn help_args(&self) -> Vec<String>;

    fn version_args(&self) -> Vec<String>;

    fn model_args(&self, model: &str) -> Option<Vec<String>>;

    /// Alternate prompt argv shape for compatibility tooling or tests.
    /// `command_for(AgentInvocation::Prompt)` uses [`Self::caretta_native_run_argv`] only,
    /// not `prompt_args`, so the caretta runner matches native `run_agent` argv.
    fn prompt_args(&self, prompt: &str) -> Vec<String> {
        self.caretta_native_run_argv(prompt)
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
        Vec::new()
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

    #[test]
    fn default_capabilities_are_conservative() {
        let caps = super::AdapterCapabilities::default();
        assert!(caps.tool_use);
        assert!(!caps.vision);
        assert!(caps.streaming);
        assert!(caps.context_window.is_none());
    }

    #[test]
    fn mock_adapter_uses_default_capabilities() {
        let adapter = MockAdapter;
        assert_eq!(
            adapter.capabilities(),
            super::AdapterCapabilities::default()
        );
    }

    #[test]
    fn capability_check_passes_when_all_satisfied() {
        let caps = super::AdapterCapabilities {
            tool_use: true,
            vision: true,
            streaming: true,
            context_window: Some(100_000),
        };
        let reqs = super::WorkflowCapabilityRequirements {
            tool_use: true,
            vision: true,
            streaming: false,
            min_context_window: Some(50_000),
        };
        assert!(reqs.check(&caps, "test-adapter").is_ok());
    }

    #[test]
    fn capability_check_fails_and_names_missing_capabilities() {
        let caps = super::AdapterCapabilities {
            tool_use: false,
            vision: false,
            streaming: true,
            context_window: Some(32_000),
        };
        let reqs = super::WorkflowCapabilityRequirements {
            tool_use: true,
            vision: true,
            streaming: false,
            min_context_window: Some(64_000),
        };
        let err = reqs.check(&caps, "mock-cli").unwrap_err();
        assert!(
            err.contains("mock-cli"),
            "error should mention adapter name"
        );
        assert!(err.contains("tool_use"), "error should mention tool_use");
        assert!(err.contains("vision"), "error should mention vision");
        assert!(
            err.contains("context_window"),
            "error should mention context_window"
        );
        assert!(
            !err.contains("streaming"),
            "streaming was satisfied, should not appear"
        );
    }
}
