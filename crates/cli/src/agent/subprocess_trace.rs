//! Structured tracing for agent subprocess executions.
//!
//! On each agent subprocess run, caretta appends a single JSON line to
//! `<root>/.caretta/runs.jsonl` capturing: agent name, prompt hash, model
//! identifier, endpoint, ISO-8601 start time, exit code, and duration in
//! milliseconds. The schema is identified by the `schema` field
//! (`caretta.run/v1`) so future revisions can extend without silently
//! renaming existing fields.

use crate::agent::cmd::log;
use crate::agent::types::{Agent, Config};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const RUN_RECORD_SCHEMA: &str = "caretta.run/v1";

/// One subprocess execution, as appended to `.caretta/runs.jsonl`.
///
/// Field names are part of the public schema. Do not rename without
/// bumping [`RUN_RECORD_SCHEMA`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema: String,
    pub agent: String,
    pub prompt_hash: String,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub started_at: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// Captures metadata at subprocess spawn time. Finalize via [`Self::finish`]
/// once the child exits to write the JSON line.
pub struct RunTracer {
    agent: String,
    prompt_hash: String,
    model: Option<String>,
    endpoint: Option<String>,
    started_at: DateTime<Utc>,
    started_instant: Instant,
    log_dir: PathBuf,
    finished: bool,
}

impl Drop for RunTracer {
    fn drop(&mut self) {
        if !self.finished {
            log("subprocess_trace: RunTracer dropped without calling finish — trace lost");
            #[cfg(debug_assertions)]
            panic!("RunTracer dropped without calling finish");
        }
    }
}

impl RunTracer {
    /// Build a tracer from a project config and the prompt that will be
    /// dispatched. The log file is anchored to `cfg.root`.
    pub fn from_config(cfg: &Config, prompt: &str) -> Self {
        Self::new(
            cfg.agent.to_string(),
            hash_prompt(prompt),
            cfg.pricing_model_key(),
            endpoint_for_config(cfg),
            Path::new(&cfg.root).join(".caretta"),
        )
    }

    fn new(
        agent: String,
        prompt_hash: String,
        model: Option<String>,
        endpoint: Option<String>,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            agent,
            prompt_hash,
            model,
            endpoint,
            started_at: Utc::now(),
            started_instant: Instant::now(),
            log_dir,
            finished: false,
        }
    }

    /// Build the record from captured metadata plus the supplied exit code
    /// (`None` if the child was killed or never spawned). Always returns a
    /// record so callers can serialize it for tests; I/O is the responsibility
    /// of [`Self::finish`].
    ///
    /// Marks the tracer as finished so that `Drop` does not warn when the
    /// caller inspects the record directly without going through `finish`.
    pub fn into_record(&mut self, exit_code: Option<i32>) -> RunRecord {
        self.finished = true;
        let duration_ms = u64::try_from(self.started_instant.elapsed().as_millis()).unwrap_or(0);
        RunRecord {
            schema: RUN_RECORD_SCHEMA.to_string(),
            agent: self.agent.clone(),
            prompt_hash: self.prompt_hash.clone(),
            model: self.model.clone(),
            endpoint: self.endpoint.clone(),
            started_at: iso8601_utc(self.started_at),
            exit_code,
            duration_ms,
        }
    }

    /// Finalize the trace by writing one JSON line to `.caretta/runs.jsonl`.
    /// I/O failures are logged but do not propagate — tracing must never
    /// influence the agent run outcome.
    pub fn finish(mut self, exit_code: Option<i32>) {
        let record = self.into_record(exit_code);
        append_record(&self.log_dir, &record);
    }
}

fn append_record(log_dir: &Path, record: &RunRecord) {
    let line = match serde_json::to_string(record) {
        Ok(line) => line,
        Err(err) => {
            log(&format!(
                "subprocess_trace: failed to serialize run record: {err}"
            ));
            return;
        }
    };

    if let Err(err) = fs::create_dir_all(log_dir) {
        log(&format!(
            "subprocess_trace: failed to create {}: {err}",
            log_dir.display()
        ));
        return;
    }

    let path = log_dir.join("runs.jsonl");
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")
        });
    if let Err(err) = result {
        log(&format!(
            "subprocess_trace: failed to append {}: {err}",
            path.display()
        ));
    }
}

/// Hex-encoded SHA-256 of the prompt, prefixed with the algorithm so the
/// schema can grow to other digests without breaking parsers.
pub fn hash_prompt(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(7 + digest.len() * 2);
    hex.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        // Each byte fits in two hex digits; write! to a String never fails.
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Resolve the API endpoint the subprocess will hit. Prefers an explicit
/// `local_inference.base_url` when advanced local inference is configured;
/// falls back to the documented default for each agent provider.
pub fn endpoint_for_config(cfg: &Config) -> Option<String> {
    if cfg.local_inference.advanced {
        let base_url = cfg.local_inference.base_url.trim();
        if !base_url.is_empty() {
            return Some(base_url.to_string());
        }
    }
    default_endpoint_for_agent(cfg.agent).map(str::to_string)
}

fn default_endpoint_for_agent(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::Claude | Agent::Cursor => Some("https://api.anthropic.com"),
        Agent::Codex => Some("https://api.openai.com/v1"),
        Agent::Copilot => Some("https://api.githubcopilot.com"),
        Agent::Gemini => Some("https://generativelanguage.googleapis.com"),
        Agent::Grok | Agent::Xai => Some("https://api.x.ai"),
        Agent::Cline | Agent::Junie => None,
    }
}

fn iso8601_utc(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_common::{
        BotSettings, Config, LocalInferenceConfig, LocalInferencePreset, PricingConfig,
        ScanTargets, SkillPaths, TestCommands,
    };
    use std::fs;
    use tempfile::TempDir;

    fn base_config(root: &Path, agent: Agent) -> Config {
        Config {
            agent,
            model: String::new(),
            auto_mode: false,
            dry_run: false,
            local_inference: LocalInferenceConfig {
                advanced: false,
                preset: LocalInferencePreset::Vllm,
                base_url: String::new(),
                model: String::new(),
                api_key: String::new(),
            },
            root: root.to_string_lossy().to_string(),
            project_name: "test".to_string(),
            scan_targets: ScanTargets::default(),
            skill_paths: SkillPaths::default(),
            bootstrap_agent_files: false,
            bootstrap_snapshot: false,
            workflow_preset: "default".to_string(),
            use_subscription: false,
            pricing: PricingConfig::default(),
            bot_settings: BotSettings::default(),
            bot_credentials: None,
            test: TestCommands::default(),
        }
    }

    #[test]
    fn hash_prompt_is_stable_sha256_hex_with_prefix() {
        let hash = hash_prompt("hello world");
        // Known SHA-256 of "hello world".
        assert_eq!(
            hash,
            "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn hash_prompt_is_deterministic_for_identical_inputs() {
        assert_eq!(hash_prompt("abc"), hash_prompt("abc"));
        assert_ne!(hash_prompt("abc"), hash_prompt("abcd"));
    }

    #[test]
    fn endpoint_for_config_prefers_local_inference_base_url() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = base_config(tmp.path(), Agent::Claude);
        cfg.local_inference.advanced = true;
        cfg.local_inference.base_url = "http://localhost:9000/v1".to_string();

        assert_eq!(
            endpoint_for_config(&cfg),
            Some("http://localhost:9000/v1".to_string())
        );
    }

    #[test]
    fn endpoint_for_config_falls_back_to_provider_default() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = base_config(tmp.path(), Agent::Claude);
        assert_eq!(
            endpoint_for_config(&cfg),
            Some("https://api.anthropic.com".to_string())
        );

        let cfg_codex = base_config(tmp.path(), Agent::Codex);
        assert_eq!(
            endpoint_for_config(&cfg_codex),
            Some("https://api.openai.com/v1".to_string())
        );
    }

    #[test]
    fn endpoint_for_config_ignores_local_inference_when_not_advanced() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = base_config(tmp.path(), Agent::Claude);
        cfg.local_inference.advanced = false;
        cfg.local_inference.base_url = "http://localhost:9000/v1".to_string();

        assert_eq!(
            endpoint_for_config(&cfg),
            Some("https://api.anthropic.com".to_string())
        );
    }

    #[test]
    fn endpoint_for_config_returns_none_for_agents_without_default() {
        let tmp = TempDir::new().expect("tempdir");
        assert_eq!(endpoint_for_config(&base_config(tmp.path(), Agent::Cline)), None);
        assert_eq!(endpoint_for_config(&base_config(tmp.path(), Agent::Junie)), None);
    }

    #[test]
    fn finish_appends_one_json_line_with_expected_schema() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = base_config(tmp.path(), Agent::Claude);
        cfg.model = "claude-3-5-sonnet".to_string();

        let tracer = RunTracer::from_config(&cfg, "audit-me");
        tracer.finish(Some(0));

        let log_path = tmp.path().join(".caretta").join("runs.jsonl");
        let contents = fs::read_to_string(&log_path).expect("runs.jsonl should exist");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one line per subprocess run");

        let value: serde_json::Value =
            serde_json::from_str(lines[0]).expect("log line must be valid JSON");
        assert_eq!(value["schema"], serde_json::json!(RUN_RECORD_SCHEMA));
        assert_eq!(value["agent"], serde_json::json!("claude"));
        assert_eq!(value["model"], serde_json::json!("claude-3-5-sonnet"));
        assert_eq!(
            value["endpoint"],
            serde_json::json!("https://api.anthropic.com")
        );
        assert_eq!(value["exit_code"], serde_json::json!(0));
        assert_eq!(
            value["prompt_hash"],
            serde_json::json!(hash_prompt("audit-me"))
        );

        let started_at = value["started_at"].as_str().expect("started_at string");
        assert!(
            DateTime::parse_from_rfc3339(started_at).is_ok(),
            "started_at must be ISO-8601: {started_at}"
        );
        assert!(value["duration_ms"].is_u64(), "duration_ms must be u64");
    }

    #[test]
    fn finish_appends_to_existing_log_without_truncation() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = base_config(tmp.path(), Agent::Codex);

        RunTracer::from_config(&cfg, "first").finish(Some(0));
        RunTracer::from_config(&cfg, "second").finish(Some(1));

        let log_path = tmp.path().join(".caretta").join("runs.jsonl");
        let contents = fs::read_to_string(&log_path).expect("runs.jsonl should exist");
        assert_eq!(contents.lines().count(), 2);
    }

    #[test]
    fn into_record_serializes_with_all_schema_fields() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = base_config(tmp.path(), Agent::Claude);
        cfg.model = "claude-3-5-sonnet".to_string();
        let record = RunTracer::from_config(&cfg, "p").into_record(Some(42));

        // Field renames would silently break downstream consumers; assert keys.
        let value = serde_json::to_value(&record).expect("RunRecord must serialize to JSON object");
        let object = value
            .as_object()
            .expect("record serializes to a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "agent",
                "duration_ms",
                "endpoint",
                "exit_code",
                "model",
                "prompt_hash",
                "schema",
                "started_at",
            ]
        );
        assert_eq!(record.exit_code, Some(42));
    }

    #[test]
    fn finish_writes_null_exit_code_when_subprocess_killed() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = base_config(tmp.path(), Agent::Claude);
        RunTracer::from_config(&cfg, "killed").finish(None);

        let log_path = tmp.path().join(".caretta").join("runs.jsonl");
        let contents = fs::read_to_string(&log_path).expect("runs.jsonl should exist");
        let value: serde_json::Value =
            serde_json::from_str(contents.lines().next().expect("one line")).expect("valid JSON");
        assert_eq!(value["exit_code"], serde_json::Value::Null);
    }
}
