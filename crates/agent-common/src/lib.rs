use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

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

/// Discrete categories an adapter may declare in its [`CapabilityManifest`].
///
/// These mirror the variants of [`AgentInvocation`] but without payloads, so a
/// manifest can be queried before constructing a concrete invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    Help,
    Version,
    Model,
    Prompt,
    Resume,
    Project,
    OutputFormat,
    Yolo,
}

impl Capability {
    pub fn name(self) -> &'static str {
        match self {
            Capability::Help => "help",
            Capability::Version => "version",
            Capability::Model => "model",
            Capability::Prompt => "prompt",
            Capability::Resume => "resume",
            Capability::Project => "project",
            Capability::OutputFormat => "output_format",
            Capability::Yolo => "yolo",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl AgentInvocation {
    /// Capability kind for this invocation, ignoring any payload value.
    pub fn capability(&self) -> Capability {
        match self {
            AgentInvocation::Help => Capability::Help,
            AgentInvocation::Version => Capability::Version,
            AgentInvocation::Model(_) => Capability::Model,
            AgentInvocation::Prompt(_) => Capability::Prompt,
            AgentInvocation::Resume(_) => Capability::Resume,
            AgentInvocation::Project(_) => Capability::Project,
            AgentInvocation::OutputFormat(_) => Capability::OutputFormat,
            AgentInvocation::Yolo => Capability::Yolo,
        }
    }
}

/// Declarative manifest of [`AgentInvocation`] variants an adapter supports.
///
/// Adapters return a manifest from [`AgentCliAdapter::capabilities`] so callers
/// can inspect the supported surface without probing each argv builder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityManifest {
    supported: BTreeSet<Capability>,
}

impl CapabilityManifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, capability: Capability) -> Self {
        self.supported.insert(capability);
        self
    }

    pub fn with_all<I: IntoIterator<Item = Capability>>(mut self, capabilities: I) -> Self {
        self.supported.extend(capabilities);
        self
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.supported.contains(&capability)
    }

    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.supported.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.supported.len()
    }

    pub fn is_empty(&self) -> bool {
        self.supported.is_empty()
    }
}

/// Error returned when [`AgentCliAdapter::command_for`] is asked for a
/// capability the adapter does not declare in its manifest, or when the
/// adapter's argv builder for a declared capability cannot serve the supplied
/// payload (e.g. a session id the underlying CLI does not accept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedCapabilityError {
    pub binary: String,
    pub capability: Capability,
}

impl fmt::Display for UnsupportedCapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "agent adapter `{}` does not support capability `{}`",
            self.binary,
            self.capability.name()
        )
    }
}

impl Error for UnsupportedCapabilityError {}

pub trait AgentCliAdapter {
    fn binary(&self) -> &'static str;

    /// Declarative manifest of supported [`AgentInvocation`] variants.
    ///
    /// Adapters must enumerate every capability they handle so callers can
    /// inspect support without probing each argv builder. Returning an empty
    /// manifest means the adapter accepts no invocations.
    fn capabilities(&self) -> CapabilityManifest;

    /// Returns `true` if this adapter supports `cap`.
    ///
    /// Overrideable so adapters that want a cheaper check (e.g. a static
    /// `[Capability]` slice) avoid the `BTreeSet` allocation on every
    /// `command_for` call.
    fn supports_capability(&self, cap: Capability) -> bool {
        self.capabilities().supports(cap)
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

    fn command_for(
        &self,
        invocation: AgentInvocation,
    ) -> Result<AgentCliCommand, UnsupportedCapabilityError> {
        let capability = invocation.capability();
        let unsupported = || UnsupportedCapabilityError {
            binary: self.binary().to_string(),
            capability,
        };

        if !self.supports_capability(capability) {
            return Err(unsupported());
        }

        let args = match invocation {
            AgentInvocation::Help => self.help_args(),
            AgentInvocation::Version => self.version_args(),
            AgentInvocation::Model(model) => self.model_args(&model).ok_or_else(unsupported)?,
            AgentInvocation::Prompt(prompt) => self.caretta_native_run_argv(&prompt),
            AgentInvocation::Resume(session_id) => self
                .resume_args(session_id.as_deref())
                .ok_or_else(unsupported)?,
            AgentInvocation::Project(project) => {
                self.project_args(&project).ok_or_else(unsupported)?
            }
            AgentInvocation::OutputFormat(format) => {
                self.output_format_args(&format).ok_or_else(unsupported)?
            }
            AgentInvocation::Yolo => self.yolo_args().ok_or_else(unsupported)?,
        };

        Ok(AgentCliCommand {
            binary: self.binary().to_string(),
            args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentCliAdapter, AgentInvocation, Capability, CapabilityManifest,
        UnsupportedCapabilityError,
    };

    #[derive(Debug, Clone, Copy)]
    struct MockAdapter;

    impl AgentCliAdapter for MockAdapter {
        fn binary(&self) -> &'static str {
            "mock-cli"
        }

        fn capabilities(&self) -> CapabilityManifest {
            CapabilityManifest::new()
                .with(Capability::Help)
                .with(Capability::Version)
                .with(Capability::Model)
                .with(Capability::Prompt)
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
    fn command_for_returns_structured_error_for_undeclared_capability() {
        let adapter = MockAdapter;
        let err = adapter
            .command_for(AgentInvocation::Resume(None))
            .expect_err("resume is not declared in MockAdapter's manifest");
        assert_eq!(
            err,
            UnsupportedCapabilityError {
                binary: "mock-cli".to_string(),
                capability: Capability::Resume,
            }
        );
        assert!(err.to_string().contains("resume"));
        assert!(err.to_string().contains("mock-cli"));
    }

    #[test]
    fn manifest_supports_and_iter_reflect_declared_capabilities() {
        let manifest = MockAdapter.capabilities();
        assert!(manifest.supports(Capability::Help));
        assert!(manifest.supports(Capability::Version));
        assert!(manifest.supports(Capability::Model));
        assert!(manifest.supports(Capability::Prompt));
        assert!(!manifest.supports(Capability::Resume));
        assert!(!manifest.supports(Capability::Project));
        assert!(!manifest.supports(Capability::OutputFormat));
        assert!(!manifest.supports(Capability::Yolo));
        assert_eq!(manifest.len(), 4);
    }

    #[test]
    fn invocation_capability_matches_variant() {
        assert_eq!(AgentInvocation::Help.capability(), Capability::Help);
        assert_eq!(
            AgentInvocation::Model("x".into()).capability(),
            Capability::Model
        );
        assert_eq!(
            AgentInvocation::Resume(None).capability(),
            Capability::Resume
        );
        assert_eq!(AgentInvocation::Yolo.capability(), Capability::Yolo);
    }
}
