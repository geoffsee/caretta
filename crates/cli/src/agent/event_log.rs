//! Append-only SQLite event log for agent runs.
//!
//! Each invocation of the agent (via `work_on_issue`, `run_pr_review_fix`, etc.)
//! appends one row capturing the agent identifier, model, workflow phase, tool
//! calls, token counts, status, and wall-clock timestamps.
//!
//! # Location resolution (highest priority first)
//! 1. `CARETTA_EVENT_LOG` environment variable
//! 2. `event_log_path` field in `caretta.toml`
//! 3. `~/.local/share/caretta/event_log.db` (platform data-local dir)
//!
//! # Schema versioning
//! A `schema_version` table tracks the integer schema version. `migrate()` runs
//! forward migrations so that future schema additions only need a new `if version < N`
//! block — existing data is never destructively altered.
use crate::agent::types::{AgentEvent, ClaudeEvent, ContentBlock};
use cli_common::latest_event_model;
use cli_common::PathConstraints;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CURRENT_SCHEMA_VERSION: i64 = 3;

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub args: Value,
}

/// A tool call that targeted a path outside the active `PathConstraints`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyViolation {
    pub tool: String,
    pub path: String,
    pub reason: String,
}

/// All data captured for a single agent run, ready to persist or preview.
pub struct AgentRunRecord {
    pub agent_id: String,
    pub model: String,
    pub workflow_phase: String,
    pub issue_number: Option<u32>,
    pub tracker_number: Option<u32>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub status: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    /// Path constraints that were active during this run (empty = unconstrained).
    pub path_constraints: PathConstraints,
    /// Policy violations detected in this run (path accesses outside constraints).
    pub policy_violations: Vec<PolicyViolation>,
    /// Resolved workflow preset name (e.g. `"default"`, `"xp"`).
    pub preset_name: Option<String>,
    /// Resolved semver version of the workflow preset (e.g. `"0.1.0"`).
    pub preset_version: Option<String>,
}

// ── Path resolution ───────────────────────────────────────────────────────────

pub fn resolve_db_path(configured: Option<&str>) -> PathBuf {
    if let Some(p) = configured.filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("CARETTA_EVENT_LOG")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p.trim());
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("caretta")
        .join("event_log.db")
}

// ── Database management ───────────────────────────────────────────────────────

fn open_db(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;

    let version: i64 = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    if version < 1 {
        // Fresh installs: create complete table with all columns, jump straight to v3.
        conn.execute_batch(&format!(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS agent_runs (
                 id                INTEGER PRIMARY KEY AUTOINCREMENT,
                 agent_id          TEXT    NOT NULL,
                 model             TEXT    NOT NULL,
                 workflow_phase    TEXT    NOT NULL,
                 issue_number      INTEGER,
                 tracker_number    INTEGER,
                 tool_calls        TEXT    NOT NULL DEFAULT '[]',
                 input_tokens      INTEGER,
                 output_tokens     INTEGER,
                 status            TEXT    NOT NULL,
                 started_at        TEXT    NOT NULL,
                 finished_at       TEXT    NOT NULL,
                 duration_ms       INTEGER,
                 preset_name       TEXT,
                 preset_version    TEXT,
                 path_constraints  TEXT    NOT NULL DEFAULT '{{}}',
                 policy_violations TEXT    NOT NULL DEFAULT '[]'
             );
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES ({CURRENT_SCHEMA_VERSION});
             COMMIT;"
        ))?;
    }

    if version >= 1 && version < 2 {
        // Upgrade v1 databases: add both preset and path-constraint columns in one step.
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE agent_runs ADD COLUMN preset_name TEXT;
             ALTER TABLE agent_runs ADD COLUMN preset_version TEXT;
             ALTER TABLE agent_runs ADD COLUMN path_constraints TEXT NOT NULL DEFAULT '{}';
             ALTER TABLE agent_runs ADD COLUMN policy_violations TEXT NOT NULL DEFAULT '[]';
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (3);
             COMMIT;",
        )?;
    }

    if version >= 2 && version < 3 {
        // Upgrade v2 databases (from #74/issue-70: have preset columns but no
        // path-constraint columns): add the path-constraint audit columns.
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE agent_runs ADD COLUMN path_constraints TEXT NOT NULL DEFAULT '{}';
             ALTER TABLE agent_runs ADD COLUMN policy_violations TEXT NOT NULL DEFAULT '[]';
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (3);
             COMMIT;",
        )?;
    }

    Ok(())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Append `record` to the SQLite event log at `db_path`.
/// Logs a warning and returns without panicking on any database error.
pub fn append_run(record: &AgentRunRecord, db_path: &Path) {
    let conn = match open_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "event_log: failed to open database at {}: {e}",
                db_path.display()
            );
            return;
        }
    };

    let tool_calls_json =
        serde_json::to_string(&record.tool_calls).unwrap_or_else(|_| "[]".to_string());
    let path_constraints_json =
        serde_json::to_string(&record.path_constraints).unwrap_or_else(|_| "{}".to_string());
    let policy_violations_json =
        serde_json::to_string(&record.policy_violations).unwrap_or_else(|_| "[]".to_string());

    if let Err(e) = conn.execute(
        "INSERT INTO agent_runs (
            agent_id, model, workflow_phase,
            issue_number, tracker_number,
            tool_calls, input_tokens, output_tokens,
            status, started_at, finished_at, duration_ms,
            path_constraints, policy_violations,
            preset_name, preset_version
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            record.agent_id,
            record.model,
            record.workflow_phase,
            record.issue_number,
            record.tracker_number,
            tool_calls_json,
            record.input_tokens,
            record.output_tokens,
            record.status,
            record.started_at,
            record.finished_at,
            record.duration_ms as i64,
            path_constraints_json,
            policy_violations_json,
            record.preset_name,
            record.preset_version,
        ],
    ) {
        tracing::warn!("event_log: failed to insert run record: {e}");
    }
}

/// Return a pretty-printed JSON preview of `record` without touching the database.
/// Used by `--dry-run` to show what *would* be written.
pub fn preview_entry(record: &AgentRunRecord) -> String {
    let entry = serde_json::json!({
        "agent_id":          record.agent_id,
        "model":             record.model,
        "workflow_phase":    record.workflow_phase,
        "issue_number":      record.issue_number,
        "tracker_number":    record.tracker_number,
        "tool_calls":        record.tool_calls,
        "input_tokens":      record.input_tokens,
        "output_tokens":     record.output_tokens,
        "status":            record.status,
        "started_at":        record.started_at,
        "finished_at":       record.finished_at,
        "duration_ms":       record.duration_ms,
        "path_constraints":  record.path_constraints,
        "policy_violations": record.policy_violations,
        "preset_name":       record.preset_name,
        "preset_version":    record.preset_version,
    });
    serde_json::to_string_pretty(&entry).unwrap_or_else(|_| "{}".to_string())
}

// ── Event extraction ──────────────────────────────────────────────────────────

/// Distil a sequence of captured [`AgentEvent`]s into the fields needed for an
/// [`AgentRunRecord`]. Returns `(tool_calls, input_tokens, output_tokens, status, model)`.
///
/// `status` defaults to `"unknown"` when no terminal `Result` event is present
/// (e.g. the agent was killed mid-run). Callers should override with a definitive
/// value when they have one (see `agent_ok` in `work_on_issue`).
///
/// Events in the capture buffer are already truncated by `emit_event` in `process.rs`
/// before reaching here, so no additional truncation is applied.
pub fn extract_run_data(
    events: &[AgentEvent],
) -> (
    Vec<ToolCallRecord>,
    Option<u32>,
    Option<u32>,
    String,
    Option<String>,
) {
    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();
    let mut input_tokens: Option<u32> = None;
    let mut output_tokens: Option<u32> = None;
    let mut status = "unknown".to_string();

    for ev in events {
        match ev {
            AgentEvent::Claude(ClaudeEvent::Assistant { message }) => {
                for block in &message.content {
                    if let ContentBlock::ToolUse { name, input, .. } = block {
                        tool_calls.push(ToolCallRecord {
                            name: name.clone(),
                            args: input.clone(),
                        });
                    }
                }
            }
            AgentEvent::Claude(ClaudeEvent::Result {
                status: s,
                input_tokens: it,
                output_tokens: ot,
                ..
            }) => {
                status = s.clone();
                if it.is_some() {
                    input_tokens = *it;
                }
                if ot.is_some() {
                    output_tokens = *ot;
                }
            }
            _ => {}
        }
    }

    let model = latest_event_model(events);
    (tool_calls, input_tokens, output_tokens, status, model)
}

// ── Timestamp helpers ─────────────────────────────────────────────────────────

/// Current time as an ISO 8601 UTC string (e.g. `"2026-05-10T14:23:01Z"`).
pub fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (year, month, day, hour, min, sec) = unix_secs_to_utc(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn unix_secs_to_utc(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    (year, month, day, hour, min, sec)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_lengths: [u64; 12] = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for len in &month_lengths {
        if days < *len {
            break;
        }
        days -= len;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        AgentRunRecord, CURRENT_SCHEMA_VERSION, PolicyViolation, ToolCallRecord, append_run,
        extract_run_data, is_leap_year, iso8601_now, preview_entry, resolve_db_path,
        unix_secs_to_utc,
    };
    use crate::agent::types::{AgentEvent, AssistantMessage, ClaudeEvent, ContentBlock};
    use std::path::PathBuf;

    // Serialises env-var mutation tests so concurrent test threads can't race
    // on the CARETTA_EVENT_LOG process-global.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn resolve_db_path_uses_env_var() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("CARETTA_EVENT_LOG", "/tmp/test_event_log.db") };
        let path = resolve_db_path(None);
        unsafe { std::env::remove_var("CARETTA_EVENT_LOG") };
        assert_eq!(path, PathBuf::from("/tmp/test_event_log.db"));
    }

    #[test]
    fn resolve_db_path_prefers_configured_over_env() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("CARETTA_EVENT_LOG", "/tmp/env_log.db") };
        let path = resolve_db_path(Some("/tmp/config_log.db"));
        unsafe { std::env::remove_var("CARETTA_EVENT_LOG") };
        assert_eq!(path, PathBuf::from("/tmp/config_log.db"));
    }

    #[test]
    fn iso8601_now_has_expected_format() {
        let ts = iso8601_now();
        // e.g. "2026-05-10T14:23:01Z"
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }

    #[test]
    fn unix_epoch_converts_correctly() {
        assert_eq!(unix_secs_to_utc(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn one_day_after_epoch_converts_correctly() {
        assert_eq!(unix_secs_to_utc(86400), (1970, 1, 2, 0, 0, 0));
    }

    #[test]
    fn is_leap_year_correct() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn extract_run_data_collects_tool_calls_and_tokens() {
        let events = vec![
            AgentEvent::Claude(ClaudeEvent::System {
                subtype: "init".to_string(),
                model: Some("claude-sonnet-4-6".to_string()),
                description: None,
                session_id: None,
                claude_code_version: None,
                tools: None,
            }),
            AgentEvent::Claude(ClaudeEvent::Assistant {
                message: AssistantMessage {
                    content: vec![ContentBlock::ToolUse {
                        id: "t1".to_string(),
                        name: "Bash".to_string(),
                        input: serde_json::json!({"command": "ls"}),
                    }],
                },
            }),
            AgentEvent::Claude(ClaudeEvent::Result {
                status: "completed".to_string(),
                summary: None,
                duration_ms: Some(1500),
                input_tokens: Some(1000),
                output_tokens: Some(250),
            }),
        ];

        let (tool_calls, input_tokens, output_tokens, status, model) = extract_run_data(&events);

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "Bash");
        assert_eq!(input_tokens, Some(1000));
        assert_eq!(output_tokens, Some(250));
        assert_eq!(status, "completed");
        assert_eq!(model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn preview_entry_is_valid_json() {
        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: Some(42),
            tracker_number: Some(76),
            tool_calls: vec![ToolCallRecord {
                name: "Edit".to_string(),
                args: serde_json::json!({}),
            }],
            input_tokens: Some(500),
            output_tokens: Some(100),
            status: "dry-run".to_string(),
            started_at: "2026-05-10T00:00:00Z".to_string(),
            finished_at: "2026-05-10T00:00:01Z".to_string(),
            duration_ms: 1000,
            path_constraints: cli_common::PathConstraints {
                allow_paths: vec!["src/".to_string()],
                deny_paths: vec![],
            },
            policy_violations: vec![],
            preset_name: Some("default".to_string()),
            preset_version: Some("0.1.0".to_string()),
        };

        let preview = preview_entry(&record);
        let parsed: serde_json::Value =
            serde_json::from_str(&preview).expect("preview must be valid JSON");
        assert_eq!(parsed["agent_id"], "claude");
        assert_eq!(parsed["issue_number"], 42);
        assert_eq!(parsed["status"], "dry-run");
        assert!(parsed["path_constraints"]["allow_paths"].is_array());
        assert!(parsed["policy_violations"].is_array());
        assert_eq!(parsed["preset_name"], "default");
        assert_eq!(parsed["preset_version"], "0.1.0");
    }

    #[test]
    fn append_run_creates_and_writes_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test_event_log.db");

        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "test-model".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: Some(1),
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: Some(100),
            output_tokens: Some(50),
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 1000,
            path_constraints: cli_common::PathConstraints::default(),
            policy_violations: vec![],
            preset_name: Some("default".to_string()),
            preset_version: Some("0.1.0".to_string()),
        };

        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("db should exist after append_run");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .expect("count query");
        assert_eq!(count, 1);

        let agent_id: String = conn
            .query_row("SELECT agent_id FROM agent_runs LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("agent_id query");
        assert_eq!(agent_id, "claude");
    }

    #[test]
    fn append_run_stores_path_constraints_and_violations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("pc_test.db");

        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "test-model".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: Some(99),
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: None,
            output_tokens: None,
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 0,
            path_constraints: cli_common::PathConstraints {
                allow_paths: vec!["src/".to_string()],
                deny_paths: vec![],
            },
            policy_violations: vec![PolicyViolation {
                tool: "Read".to_string(),
                path: "vendor/foo.rs".to_string(),
                reason: "path is outside allow_paths: [src/]".to_string(),
            }],
            preset_name: None,
            preset_version: None,
        };

        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("db");
        let (pc_json, pv_json): (String, String) = conn
            .query_row(
                "SELECT path_constraints, policy_violations FROM agent_runs LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query");

        let pc: serde_json::Value = serde_json::from_str(&pc_json).expect("valid json");
        let pv: serde_json::Value = serde_json::from_str(&pv_json).expect("valid json");

        assert_eq!(pc["allow_paths"][0], "src/");
        assert_eq!(pv[0]["tool"], "Read");
        assert_eq!(pv[0]["path"], "vendor/foo.rs");
    }

    #[test]
    fn migrate_v1_to_v3_adds_path_columns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("v1_upgrade.db");

        // Manually create a v1-era schema (no path_constraints / policy_violations columns).
        let conn = rusqlite::Connection::open(&db_path).expect("db");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (1);
             CREATE TABLE agent_runs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 agent_id TEXT NOT NULL, model TEXT NOT NULL,
                 workflow_phase TEXT NOT NULL, issue_number INTEGER,
                 tracker_number INTEGER, tool_calls TEXT NOT NULL DEFAULT '[]',
                 input_tokens INTEGER, output_tokens INTEGER,
                 status TEXT NOT NULL, started_at TEXT NOT NULL,
                 finished_at TEXT NOT NULL, duration_ms INTEGER
             );",
        )
        .expect("setup v1");
        drop(conn);

        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "test-model".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: None,
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: None,
            output_tokens: None,
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 0,
            path_constraints: cli_common::PathConstraints::default(),
            policy_violations: vec![],
            preset_name: None,
            preset_version: None,
        };
        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("db");
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let _: String = conn
            .query_row(
                "SELECT path_constraints FROM agent_runs LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("path_constraints column should exist after migration");
        let _: String = conn
            .query_row(
                "SELECT policy_violations FROM agent_runs LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("policy_violations column should exist after migration");
    }

    #[test]
    fn migrate_v2_to_v3_adds_path_columns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("v2_upgrade.db");

        // Manually create a v2-era schema (preset columns present, no path columns).
        let conn = rusqlite::Connection::open(&db_path).expect("db");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (2);
             CREATE TABLE agent_runs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 agent_id TEXT NOT NULL, model TEXT NOT NULL,
                 workflow_phase TEXT NOT NULL, issue_number INTEGER,
                 tracker_number INTEGER, tool_calls TEXT NOT NULL DEFAULT '[]',
                 input_tokens INTEGER, output_tokens INTEGER,
                 status TEXT NOT NULL, started_at TEXT NOT NULL,
                 finished_at TEXT NOT NULL, duration_ms INTEGER,
                 preset_name TEXT, preset_version TEXT
             );",
        )
        .expect("setup v2");
        drop(conn);

        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "test-model".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: None,
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: None,
            output_tokens: None,
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 0,
            path_constraints: cli_common::PathConstraints::default(),
            policy_violations: vec![],
            preset_name: Some("default".to_string()),
            preset_version: Some("0.1.0".to_string()),
        };
        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("db");
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let _: String = conn
            .query_row(
                "SELECT path_constraints FROM agent_runs LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("path_constraints column should exist after v2→v3 migration");
        let _: String = conn
            .query_row(
                "SELECT policy_violations FROM agent_runs LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("policy_violations column should exist after v2→v3 migration");
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("idempotent.db");

        // Run migration twice; second call must not fail or duplicate rows.
        let record = AgentRunRecord {
            agent_id: "test".to_string(),
            model: "m".to_string(),
            workflow_phase: "test".to_string(),
            issue_number: None,
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: None,
            output_tokens: None,
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 0,
            path_constraints: cli_common::PathConstraints::default(),
            policy_violations: vec![],
            preset_name: None,
            preset_version: None,
        };

        append_run(&record, &db_path);
        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 2, "each append_run should add exactly one row");

        let ver: i64 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("schema_version should exist");
        assert_eq!(
            ver, CURRENT_SCHEMA_VERSION,
            "schema_version should be current"
        );
        let row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .expect("schema_version count");
        assert_eq!(row_count, 1, "schema_version should have exactly one row");
    }

    #[test]
    fn preset_columns_are_persisted_and_readable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("preset_test.db");

        let record = AgentRunRecord {
            agent_id: "claude".to_string(),
            model: "test-model".to_string(),
            workflow_phase: "issue".to_string(),
            issue_number: None,
            tracker_number: None,
            tool_calls: vec![],
            input_tokens: None,
            output_tokens: None,
            status: "completed".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:01Z".to_string(),
            duration_ms: 0,
            path_constraints: cli_common::PathConstraints::default(),
            policy_violations: vec![],
            preset_name: Some("xp".to_string()),
            preset_version: Some("0.1.0".to_string()),
        };

        append_run(&record, &db_path);

        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        let (pname, pver): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT preset_name, preset_version FROM agent_runs LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read preset columns");
        assert_eq!(pname.as_deref(), Some("xp"));
        assert_eq!(pver.as_deref(), Some("0.1.0"));
    }
}
