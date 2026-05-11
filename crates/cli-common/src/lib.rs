use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workflow {
    Ideation,
    ReportResearch,
    StrategicReview,
    Roadmapper,
    SprintPlanning,
    SprintPoker,
    PreIpm,
    Ipm,
    Retrospective,
    Housekeeping,
    Interview,
    Chat,
}

impl std::fmt::Display for Workflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Workflow::Ideation => write!(f, "Ideation"),
            Workflow::ReportResearch => write!(f, "UXR Synth"),
            Workflow::StrategicReview => write!(f, "Strategic Review"),
            Workflow::Roadmapper => write!(f, "Roadmapper"),
            Workflow::SprintPlanning => write!(f, "Sprint Planning"),
            Workflow::SprintPoker => write!(f, "Sprint Poker"),
            Workflow::PreIpm => write!(f, "Pre-IPM"),
            Workflow::Ipm => write!(f, "IPM"),
            Workflow::Retrospective => write!(f, "Retrospective"),
            Workflow::Housekeeping => write!(f, "Housekeeping"),
            Workflow::Interview => write!(f, "Interview"),
            Workflow::Chat => write!(f, "Chat"),
        }
    }
}

impl Workflow {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "ideation" => Some(Self::Ideation),
            "report_research" => Some(Self::ReportResearch),
            "strategic_review" => Some(Self::StrategicReview),
            "roadmapper" => Some(Self::Roadmapper),
            "sprint_planning" => Some(Self::SprintPlanning),
            "sprint_poker" => Some(Self::SprintPoker),
            "pre_ipm" => Some(Self::PreIpm),
            "ipm" => Some(Self::Ipm),
            "retrospective" => Some(Self::Retrospective),
            "housekeeping" => Some(Self::Housekeeping),
            "interview" => Some(Self::Interview),
            "chat" => Some(Self::Chat),
            _ => None,
        }
    }

    pub fn to_id(&self) -> &'static str {
        match self {
            Self::Ideation => "ideation",
            Self::ReportResearch => "report_research",
            Self::StrategicReview => "strategic_review",
            Self::Roadmapper => "roadmapper",
            Self::SprintPlanning => "sprint_planning",
            Self::SprintPoker => "sprint_poker",
            Self::PreIpm => "pre_ipm",
            Self::Ipm => "ipm",
            Self::Retrospective => "retrospective",
            Self::Housekeeping => "housekeeping",
            Self::Interview => "interview",
            Self::Chat => "chat",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InterviewTurn {
    pub is_agent: bool,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerInfo {
    pub number: u32,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingIssue {
    pub number: u32,
    pub title: String,
    pub blockers: Vec<u32>,
    pub pr_number: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Done,
    Log(String),
    Claude(ClaudeEvent),
    AwaitingFeedback(Workflow),
    TrackerUpdate(Vec<PendingIssue>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelPricing {
    /// USD per 1M input tokens. This is intentionally project-configured:
    /// provider prices change too often to bake into caretta.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub input_per_million: f64,
    /// USD per 1M output tokens.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub output_per_million: f64,
}

impl ModelPricing {
    pub fn has_rate(self) -> bool {
        (self.input_per_million.is_finite() && self.input_per_million > 0.0)
            || (self.output_per_million.is_finite() && self.output_per_million > 0.0)
    }

    pub fn estimate_cost_usd(self, input_tokens: u32, output_tokens: u32) -> Option<f64> {
        if !self.has_rate() {
            return None;
        }

        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_million.max(0.0);
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_million.max(0.0);
        Some(input_cost + output_cost)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PricingConfig {
    /// Per-model token rates keyed by provider model ID.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub models: HashMap<String, ModelPricing>,
}

impl PricingConfig {
    pub fn rate_for_model(&self, model: &str) -> Option<ModelPricing> {
        let key = model.trim();
        if key.is_empty() {
            return None;
        }
        self.models
            .get(key)
            .copied()
            .or_else(|| {
                let lower = key.to_lowercase();
                self.models
                    .iter()
                    .find(|(candidate, _)| candidate.to_lowercase() == lower)
                    .map(|(_, rate)| *rate)
            })
            .filter(|rate| rate.has_rate())
    }

    pub fn estimate_cost_usd(
        &self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Option<f64> {
        self.rate_for_model(model)
            .and_then(|rate| rate.estimate_cost_usd(input_tokens, output_tokens))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    System {
        subtype: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        claude_code_version: Option<String>,
        #[serde(default)]
        tools: Option<Vec<String>>,
    },
    Assistant {
        message: AssistantMessage,
    },
    User {
        message: UserMessage,
    },
    Result {
        status: String,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        duration_ms: Option<u64>,
        #[serde(default)]
        input_tokens: Option<u32>,
        #[serde(default)]
        output_tokens: Option<u32>,
    },
    ContentBlockDelta {
        index: u32,
        delta: ContentBlockDelta,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct ContentBlockDelta {
    #[serde(rename = "type")]
    pub delta_type: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLaunchOverrides {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: Vec<ContentBlock>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        #[serde(rename = "tool_use_id")]
        id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Agent {
    Claude,
    Cline,
    Codex,
    Copilot,
    Gemini,
    Grok,
    Junie,
    Xai,
    Cursor,
}

impl ValueEnum for Agent {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Claude,
            Self::Cline,
            Self::Codex,
            Self::Copilot,
            Self::Gemini,
            Self::Grok,
            Self::Junie,
            Self::Xai,
            Self::Cursor,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::Claude => clap::builder::PossibleValue::new("claude"),
            Self::Cline => clap::builder::PossibleValue::new("cline"),
            Self::Codex => clap::builder::PossibleValue::new("codex"),
            Self::Copilot => clap::builder::PossibleValue::new("copilot"),
            Self::Gemini => clap::builder::PossibleValue::new("gemini"),
            Self::Grok => clap::builder::PossibleValue::new("grok"),
            Self::Junie => clap::builder::PossibleValue::new("junie"),
            Self::Xai => clap::builder::PossibleValue::new("xai"),
            Self::Cursor => clap::builder::PossibleValue::new("cursor"),
        })
    }
}

impl FromStr for Agent {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "cline" => Ok(Self::Cline),
            "codex" => Ok(Self::Codex),
            "copilot" => Ok(Self::Copilot),
            "gemini" => Ok(Self::Gemini),
            "grok" => Ok(Self::Grok),
            "junie" => Ok(Self::Junie),
            "xai" => Ok(Self::Xai),
            "cursor" => Ok(Self::Cursor),
            _ => Err(format!("Unknown agent: {}", s)),
        }
    }
}

impl Agent {
    /// Basename of the provider CLI on `PATH`, aligned with each crate's
    /// `agent_common::AgentCliAdapter::binary` implementation.
    ///
    /// caretta constructs subprocess argv via `crates/cli/src/agent/adapter_dispatch.rs`
    /// and those adapters — not via this method alone.
    pub fn binary(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Cline => "cline",
            Agent::Codex => "codex",
            Agent::Copilot => "copilot",
            Agent::Gemini => "gemini",
            Agent::Grok => "grok",
            Agent::Junie => "junie",
            Agent::Xai => "copilot", // xAI proxies the copilot CLI
            Agent::Cursor => "cursor",
        }
    }

    pub fn co_author(self) -> &'static str {
        match self {
            Agent::Claude => "Co-Authored-By: Claude <noreply@anthropic.com>",
            Agent::Cline => "Co-Authored-By: Cline <noreply@cline.bot>",
            Agent::Codex => "Co-Authored-By: Codex <noreply@openai.com>",
            Agent::Copilot => "Co-Authored-By: GitHub Copilot <noreply@github.com>",
            Agent::Gemini => "Co-Authored-By: Gemini <noreply@google.com>",
            Agent::Grok => "Co-Authored-By: Grok <noreply@x.ai>",
            Agent::Junie => "Co-Authored-By: Junie <junie@jetbrains.com>",
            Agent::Xai => "Co-Authored-By: xAI <noreply@x.ai>",
            Agent::Cursor => "Co-Authored-By: Cursor <noreply@cursor.com>",
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Claude => "claude",
            Self::Cline => "cline",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Junie => "junie",
            Self::Xai => "xai",
            Self::Cursor => "cursor",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalInferencePreset {
    #[default]
    Vllm,
    LmStudio,
    Ollama,
    Custom,
}

impl LocalInferencePreset {
    pub fn default_base_url(self) -> Option<&'static str> {
        match self {
            LocalInferencePreset::Vllm => Some("http://localhost:8000/v1"),
            LocalInferencePreset::LmStudio => Some("http://localhost:1234/v1"),
            LocalInferencePreset::Ollama => Some("http://localhost:11434/v1"),
            LocalInferencePreset::Custom => None,
        }
    }

    pub fn infer_from_base_url(base_url: &str) -> Self {
        for preset in [
            LocalInferencePreset::Vllm,
            LocalInferencePreset::LmStudio,
            LocalInferencePreset::Ollama,
        ] {
            if preset.default_base_url() == Some(base_url) {
                return preset;
            }
        }
        LocalInferencePreset::Custom
    }
}

impl FromStr for LocalInferencePreset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vllm" => Ok(Self::Vllm),
            "lm_studio" | "lm-studio" | "lmstudio" => Ok(Self::LmStudio),
            "ollama" => Ok(Self::Ollama),
            "custom" => Ok(Self::Custom),
            _ => Err(format!("Unknown local inference preset: {s}")),
        }
    }
}

impl fmt::Display for LocalInferencePreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vllm => write!(f, "vllm"),
            Self::LmStudio => write!(f, "lm_studio"),
            Self::Ollama => write!(f, "ollama"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalInferenceConfig {
    pub advanced: bool,
    pub preset: LocalInferencePreset,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

impl Default for LocalInferenceConfig {
    fn default() -> Self {
        Self {
            advanced: false,
            preset: LocalInferencePreset::Vllm,
            base_url: LocalInferencePreset::Vllm
                .default_base_url()
                .unwrap_or_default()
                .to_string(),
            model: String::new(),
            api_key: String::new(),
        }
    }
}

impl LocalInferenceConfig {
    pub fn apply_preset(&mut self, preset: LocalInferencePreset) {
        self.preset = preset;
        if let Some(base_url) = preset.default_base_url() {
            self.base_url = base_url.to_string();
        }
    }

    pub fn set_base_url(&mut self, base_url: String) {
        self.preset = LocalInferencePreset::infer_from_base_url(base_url.trim());
        self.base_url = base_url;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotAuthMode {
    #[default]
    Disabled,
    Token,
    #[serde(rename = "github_app")]
    GitHubApp,
}

impl fmt::Display for BotAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "Disabled"),
            Self::Token => write!(f, "Token"),
            Self::GitHubApp => write!(f, "GitHub App"),
        }
    }
}

impl FromStr for BotAuthMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disabled" | "none" => Ok(Self::Disabled),
            "token" => Ok(Self::Token),
            "github_app" | "github-app" | "githubapp" => Ok(Self::GitHubApp),
            _ => Err(format!("Unknown auth mode: {}", s)),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum BotCredentials {
    Token(String),
    GitHubApp {
        app_id: String,
        installation_id: String,
        private_key_pem: String,
    },
}

impl fmt::Debug for BotCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(_) => write!(f, "Token(***)"),
            Self::GitHubApp {
                app_id,
                installation_id,
                ..
            } => f
                .debug_struct("GitHubApp")
                .field("app_id", app_id)
                .field("installation_id", installation_id)
                .field("private_key_pem", &"***")
                .finish(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BotSettings {
    pub mode: BotAuthMode,
    pub token: String,
    pub app_id: String,
    pub installation_id: String,
    pub private_key_pem: String,
}

impl BotSettings {
    pub fn from_credentials(creds: &BotCredentials) -> Self {
        match creds {
            BotCredentials::Token(token) => Self {
                mode: BotAuthMode::Token,
                token: token.clone(),
                ..Self::default()
            },
            BotCredentials::GitHubApp {
                app_id,
                installation_id,
                private_key_pem,
            } => Self {
                mode: BotAuthMode::GitHubApp,
                app_id: app_id.clone(),
                installation_id: installation_id.clone(),
                private_key_pem: private_key_pem.clone(),
                ..Self::default()
            },
        }
    }

    pub fn to_credentials(&self) -> Option<BotCredentials> {
        match self.mode {
            BotAuthMode::Disabled => None,
            BotAuthMode::Token => {
                let token = self.token.trim();
                if token.is_empty() {
                    None
                } else {
                    Some(BotCredentials::Token(token.to_string()))
                }
            }
            BotAuthMode::GitHubApp => {
                let app_id = self.app_id.trim();
                let installation_id = self.installation_id.trim();
                let private_key_pem = self.private_key_pem.trim();
                if app_id.is_empty() || installation_id.is_empty() || private_key_pem.is_empty() {
                    None
                } else {
                    Some(BotCredentials::GitHubApp {
                        app_id: app_id.to_string(),
                        installation_id: installation_id.to_string(),
                        private_key_pem: private_key_pem.to_string(),
                    })
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillPaths {
    pub user_personas: String,
    pub issue_tracking: String,
}

impl Default for SkillPaths {
    fn default() -> Self {
        Self {
            user_personas: "assets/skills/user-personas/SKILL.md".to_string(),
            issue_tracking: "assets/skills/issue-tracking/SKILL.md".to_string(),
        }
    }
}

/// Paths the user has declared as security-relevant via `[security_scan].paths`
/// in `caretta.toml`. The scanner runs its generic per-file checks against each
/// declared path. Declaring nothing leaves the per-file checks idle so agents
/// don't spend tokens guessing which files matter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanTargets {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub agent: Agent,
    pub model: String,
    pub auto_mode: bool,
    pub dry_run: bool,
    pub local_inference: LocalInferenceConfig,
    pub root: String,
    pub project_name: String,
    pub scan_targets: ScanTargets,
    pub skill_paths: SkillPaths,
    pub bootstrap_agent_files: bool,
    pub bootstrap_snapshot: bool,
    pub workflow_preset: String,
    pub use_subscription: bool,
    pub pricing: PricingConfig,
    pub bot_settings: BotSettings,
    pub bot_credentials: Option<BotCredentials>,
    pub test: TestCommands,
    /// Override path for the agent event log SQLite database.
    /// When `None`, falls back to the `CARETTA_EVENT_LOG` env var, then the platform data-local dir.
    pub event_log_path: Option<String>,
}

/// Project-specific test/format commands run after an agent edit.
///
/// Both fields are argv-style (program followed by args). An empty
/// `command` skips the test step entirely; an empty `format_command`
/// skips post-fix formatting. Configured via the `[test]` section of
/// `caretta.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestCommands {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub format_command: Vec<String>,
}

impl Config {
    pub fn effective_bot_credentials(&self) -> Option<BotCredentials> {
        self.bot_settings
            .to_credentials()
            .or_else(|| self.bot_credentials.clone())
    }

    pub fn has_bot_credentials(&self) -> bool {
        self.effective_bot_credentials().is_some()
    }

    pub fn pricing_model_key(&self) -> Option<String> {
        if self.local_inference.advanced && !self.local_inference.model.trim().is_empty() {
            Some(self.local_inference.model.trim().to_string())
        } else if !self.model.trim().is_empty() {
            Some(self.model.trim().to_string())
        } else {
            None
        }
    }
}

pub fn latest_event_model(events: &[AgentEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        AgentEvent::Claude(ClaudeEvent::System { model, .. }) => model
            .as_ref()
            .filter(|model| !model.trim().is_empty())
            .cloned(),
        _ => None,
    })
}

pub fn should_use_event_model(model: &str, has_configured_model: bool) -> bool {
    let model = model.trim();
    if model.is_empty() {
        return false;
    }

    !has_configured_model
        || !matches!(
            model,
            "claude"
                | "cline"
                | "codex"
                | "copilot"
                | "cursor"
                | "gemini"
                | "grok"
                | "junie"
                | "xai"
        )
}

pub fn usage_model_for_events(config: &Config, events: &[AgentEvent]) -> Option<String> {
    let mut active_model = config.pricing_model_key();
    let has_configured_model = active_model.is_some();

    for event in events {
        if let AgentEvent::Claude(ClaudeEvent::System {
            model: Some(model), ..
        }) = event
            && should_use_event_model(model, has_configured_model)
        {
            active_model = Some(model.trim().to_string());
        }
    }

    active_model
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("agent", &self.agent)
            .field("model", &self.model)
            .field("auto_mode", &self.auto_mode)
            .field("dry_run", &self.dry_run)
            .field("local_inference", &self.local_inference)
            .field("root", &self.root)
            .field("project_name", &self.project_name)
            .field("scan_targets", &self.scan_targets)
            .field("skill_paths", &self.skill_paths)
            .field("bootstrap_agent_files", &self.bootstrap_agent_files)
            .field("bootstrap_snapshot", &self.bootstrap_snapshot)
            .field("workflow_preset", &self.workflow_preset)
            .field("use_subscription", &self.use_subscription)
            .field("pricing", &self.pricing)
            .field("bot_settings", &self.bot_settings)
            .field("bot_credentials", &self.bot_credentials)
            .field("event_log_path", &self.event_log_path)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChangeKind {
    Read,
    Created,
    Modified,
    Deleted,
}

impl fmt::Display for FileChangeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "Read"),
            Self::Created => write!(f, "Created"),
            Self::Modified => write!(f, "Modified"),
            Self::Deleted => write!(f, "Deleted"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub kind: FileChangeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrSummary {
    pub number: u32,
    pub title: String,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    #[serde(default)]
    pub author: Option<PrAuthor>,
    #[serde(default)]
    pub unresolved_thread_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrAuthor {
    pub login: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DevConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub local_inference: LocalInferenceConfigFile,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub security_scan: ScanTargetsFile,
    #[serde(default, skip_serializing_if = "is_default")]
    pub skills: SkillPathsFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_agent_files: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bot: BotSettingsFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_snapshot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_subscription: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub pricing: PricingConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    pub log_redaction: LogRedactionConfigFile,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_models: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub test: TestCommands,
    /// Override path for the SQLite event log (also: CARETTA_EVENT_LOG env var).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_log_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct LogRedactionConfigFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denylist_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist_keys: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ScanTargetsFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

impl ScanTargetsFile {
    pub fn into_scan_targets(self) -> ScanTargets {
        ScanTargets { paths: self.paths }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct LocalInferenceConfigFile {
    #[serde(skip_serializing_if = "is_none")]
    pub advanced: Option<bool>,
    #[serde(skip_serializing_if = "is_none")]
    pub preset: Option<LocalInferencePreset>,
    #[serde(skip_serializing_if = "is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl LocalInferenceConfigFile {
    pub fn into_local_inference_config(self) -> LocalInferenceConfig {
        let mut cfg = LocalInferenceConfig::default();
        if let Some(advanced) = self.advanced {
            cfg.advanced = advanced;
        }
        if let Some(preset) = self.preset {
            cfg.apply_preset(preset);
        }
        if let Some(base_url) = self.base_url {
            cfg.set_base_url(base_url);
        }
        if let Some(model) = self.model {
            cfg.model = model;
        }
        if let Some(api_key) = self.api_key {
            cfg.api_key = api_key;
        }
        cfg
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct SkillPathsFile {
    #[serde(skip_serializing_if = "is_none")]
    pub user_personas: Option<String>,
    #[serde(skip_serializing_if = "is_none")]
    pub issue_tracking: Option<String>,
}

impl SkillPathsFile {
    pub fn into_skill_paths(self) -> SkillPaths {
        let def = SkillPaths::default();
        SkillPaths {
            user_personas: self.user_personas.unwrap_or(def.user_personas),
            issue_tracking: self.issue_tracking.unwrap_or(def.issue_tracking),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct BotSettingsFile {
    #[serde(skip_serializing_if = "is_none")]
    pub mode: Option<BotAuthMode>,
    #[serde(skip_serializing_if = "is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "is_none")]
    pub installation_id: Option<String>,
}

impl BotSettingsFile {
    pub fn into_bot_settings(self) -> BotSettings {
        BotSettings {
            mode: self.mode.unwrap_or_default(),
            token: String::new(),
            app_id: self.app_id.unwrap_or_default(),
            installation_id: self.installation_id.unwrap_or_default(),
            private_key_pem: String::new(),
        }
    }
}

fn is_none<T>(opt: &Option<T>) -> bool {
    opt.is_none()
}

fn is_zero_f64(value: &f64) -> bool {
    *value == 0.0
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}

/// The common interface for an agent-cli.
pub trait AgentCli {
    fn run(&self, prompt: &str, config: &Config) -> anyhow::Result<()>;
}

pub use agent_common::AgentCliAdapter as ProviderCliWrapper;
pub use agent_common::{AgentCliAdapter, AgentCliCommand, AgentInvocation};

#[cfg(test)]
mod agent_binary_tests {
    use super::{Agent, ModelPricing, PricingConfig};
    use std::collections::HashMap;

    #[test]
    fn cursor_binary_matches_adapter_spawn_name() {
        assert_eq!(Agent::Cursor.binary(), "cursor");
    }

    #[test]
    fn pricing_estimates_cost_from_configured_rates() {
        let pricing = PricingConfig {
            models: HashMap::from([(
                "model-a".to_string(),
                ModelPricing {
                    input_per_million: 3.0,
                    output_per_million: 15.0,
                },
            )]),
        };

        let cost = pricing
            .estimate_cost_usd("model-a", 1_000_000, 500_000)
            .expect("configured rate should estimate cost");
        assert_eq!(cost, 10.5);
    }

    #[test]
    fn pricing_skips_unknown_or_zero_rates() {
        let pricing = PricingConfig {
            models: HashMap::from([("model-a".to_string(), ModelPricing::default())]),
        };

        assert_eq!(pricing.estimate_cost_usd("model-a", 1_000, 1_000), None);
        assert_eq!(pricing.estimate_cost_usd("missing", 1_000, 1_000), None);
    }

    #[test]
    fn generic_event_model_does_not_override_configured_model() {
        assert!(!super::should_use_event_model("codex", true));
        assert!(super::should_use_event_model("gpt-5.2", true));
        assert!(super::should_use_event_model("codex", false));
    }
}
