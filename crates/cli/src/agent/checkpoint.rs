use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointStatus {
    Running,
    Paused,
    Complete,
}

/// Persisted state for a single `caretta loop` invocation.
/// Written to `.caretta/run-<id>.json` at each phase boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckpoint {
    pub run_id: String,
    pub started_at: String,
    pub tracker: u32,
    /// Issue numbers (as strings) completed during this run, in order.
    pub completed_phases: Vec<String>,
    pub last_completed: Option<String>,
    pub status: CheckpointStatus,
}

/// Generate a unique run ID from the current Unix timestamp and process ID.
pub fn new_run_id() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pid = std::process::id();
    format!("{secs}-{pid:04x}")
}

/// Return the `.caretta` directory under the project root.
pub fn checkpoint_dir(root: &str) -> PathBuf {
    PathBuf::from(root).join(".caretta")
}

/// Return the path for `run-<run_id>.json` inside `.caretta/`.
pub fn checkpoint_path(root: &str, run_id: &str) -> PathBuf {
    checkpoint_dir(root).join(format!("run-{run_id}.json"))
}

/// Load and deserialise a checkpoint file; returns `None` when absent or malformed.
pub fn load_checkpoint(root: &str, run_id: &str) -> Option<RunCheckpoint> {
    let path = checkpoint_path(root, run_id);
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Serialise `checkpoint` to `.caretta/run-<id>.json`, creating the directory if needed.
pub fn save_checkpoint(root: &str, checkpoint: &RunCheckpoint) -> Result<(), String> {
    let dir = checkpoint_dir(root);
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create .caretta/: {e}"))?;
    let path = checkpoint_path(root, &checkpoint.run_id);
    let json = serde_json::to_string_pretty(checkpoint)
        .map_err(|e| format!("failed to serialise checkpoint: {e}"))?;
    fs::write(path, json).map_err(|e| format!("failed to write checkpoint: {e}"))
}

/// Format Unix seconds as a human-readable ISO 8601 UTC string without external crates.
pub fn unix_secs_to_iso8601(secs: u64) -> String {
    let (year, month, day) = days_to_ymd(secs / 86400);
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u8, u8) {
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366u64 } else { 365u64 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u8; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
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
    let mut month = 1u8;
    for &dm in &months {
        if days < dm as u64 {
            break;
        }
        days -= dm as u64;
        month += 1;
    }
    (year, month, days as u8 + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn new_run_id_is_non_empty() {
        let id = new_run_id();
        assert!(!id.is_empty());
        assert!(id.contains('-'), "run ID should contain a hyphen separator");
    }

    #[test]
    fn checkpoint_path_has_correct_format() {
        let p = checkpoint_path("/tmp/proj", "123-abc");
        assert_eq!(p, Path::new("/tmp/proj/.caretta/run-123-abc.json"));
    }

    #[test]
    fn load_checkpoint_returns_none_for_missing_file() {
        let result = load_checkpoint("/nonexistent/path", "no-such-run");
        assert!(result.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_str().unwrap();
        let cp = RunCheckpoint {
            run_id: "test-run-1".to_string(),
            started_at: "2026-05-10T12:00:00Z".to_string(),
            tracker: 76,
            completed_phases: vec!["71".to_string()],
            last_completed: Some("71".to_string()),
            status: CheckpointStatus::Paused,
        };
        save_checkpoint(root, &cp).expect("save");
        let loaded = load_checkpoint(root, "test-run-1").expect("load");
        assert_eq!(loaded.run_id, cp.run_id);
        assert_eq!(loaded.tracker, cp.tracker);
        assert_eq!(loaded.completed_phases, cp.completed_phases);
        assert_eq!(loaded.status, CheckpointStatus::Paused);
    }

    #[test]
    fn unix_secs_to_iso8601_known_value() {
        // 2025-05-10T00:00:00Z = 1746835200 seconds since epoch
        assert_eq!(unix_secs_to_iso8601(1746835200), "2025-05-10T00:00:00Z");
    }

    #[test]
    fn unix_secs_to_iso8601_epoch() {
        assert_eq!(unix_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }
}
