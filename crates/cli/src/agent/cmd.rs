use crate::agent::types::{AgentEvent, EVENT_SENDER};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::sync::{LazyLock, RwLock};
use tracing::info;

#[cfg(not(target_arch = "wasm32"))]
pub use toak_rs::count_tokens;

#[cfg(target_arch = "wasm32")]
pub fn count_tokens(s: &str) -> usize {
    s.len() / 4
}

/// Log the elapsed time for a labelled operation.
#[macro_export]
macro_rules! timed {
    ($label:expr, $body:expr) => {{
        let _t0 = Instant::now();
        let _result = $body;
        $crate::agent::cmd::log(&format!(
            "[timing] {} completed in {:.2?}",
            $label,
            _t0.elapsed()
        ));
        _result
    }};
}

pub fn die(msg: &str) -> ! {
    eprintln!("ERROR: {msg}");
    process::exit(1);
}

pub fn log(msg: &str) {
    let sanitized = sanitize_log_message(msg);
    info!("{sanitized}");
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(AgentEvent::Log(sanitized));
    }
}

static KV_QUOTED_SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)("?(?P<key>api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|token|secret|password|passwd|authorization)"?\s*[:=]\s*")(?P<value>[^"\\]{4,})(")"#)
        .expect("valid secret kv quoted regex")
});
static KV_BARE_SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?P<key>api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|token|secret|password|passwd|authorization)(?P<sep>\s*[:=]\s*)(?P<value>[^\s,;]+)"#)
        .expect("valid secret kv bare regex")
});
static AUTH_BEARER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(authorization\s*:\s*bearer\s+)([A-Za-z0-9._~+/\-=]+)"#)
        .expect("valid authorization bearer regex")
});
static GH_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(gh[pousr]_[A-Za-z0-9_]{16,})\b"#).expect("valid gh token regex")
});
static OPENAI_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(sk-[A-Za-z0-9_-]{16,})\b"#).expect("valid openai token regex")
});
static AWS_ACCESS_KEY_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(AKIA[0-9A-Z]{16})\b"#).expect("valid aws access key id regex")
});
static PRIVATE_KEY_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----"#)
        .expect("valid private key block regex")
});

#[derive(Default)]
struct RuntimeRedactionConfig {
    allowlist_keys: HashSet<String>,
    denylist_regexes: Vec<Regex>,
}

static LOG_REDACTION_CONFIG: LazyLock<RwLock<RuntimeRedactionConfig>> =
    LazyLock::new(|| RwLock::new(RuntimeRedactionConfig::default()));

pub fn configure_log_redaction(cfg: &cli_common::LogRedactionConfigFile) {
    let allowlist_keys = cfg
        .allowlist_keys
        .iter()
        .map(|k| k.trim().to_ascii_lowercase())
        .filter(|k| !k.is_empty())
        .collect::<HashSet<_>>();
    let denylist_regexes = cfg
        .denylist_patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect::<Vec<_>>();
    if let Ok(mut state) = LOG_REDACTION_CONFIG.write() {
        state.allowlist_keys = allowlist_keys;
        state.denylist_regexes = denylist_regexes;
    }
}

pub fn sanitize_log_message(msg: &str) -> String {
    let state = LOG_REDACTION_CONFIG.read().ok();
    sanitize_log_message_with_cfg(msg, state.as_deref())
}

fn sanitize_log_message_with_cfg(msg: &str, cfg: Option<&RuntimeRedactionConfig>) -> String {
    let out = PRIVATE_KEY_BLOCK_RE
        .replace_all(msg, "[REDACTED_PRIVATE_KEY]")
        .into_owned();
    let out = KV_QUOTED_SECRET_RE
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            let key = caps
                .name("key")
                .map_or("", |m| m.as_str())
                .to_ascii_lowercase();
            if cfg.is_some_and(|c| c.allowlist_keys.contains(&key)) {
                caps.get(0)
                    .map_or(String::new(), |m| m.as_str().to_string())
            } else {
                format!("{}[REDACTED]{}", &caps[1], &caps[4])
            }
        })
        .into_owned();
    let out = AUTH_BEARER_RE
        .replace_all(&out, "$1[REDACTED]")
        .into_owned();
    let out = KV_BARE_SECRET_RE
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            let key = caps
                .name("key")
                .map_or("", |m| m.as_str())
                .to_ascii_lowercase();
            if cfg.is_some_and(|c| c.allowlist_keys.contains(&key)) {
                return caps
                    .get(0)
                    .map_or(String::new(), |m| m.as_str().to_string());
            }
            let value = caps.name("value").map_or("", |m| m.as_str());
            if value.eq_ignore_ascii_case("bearer") {
                caps.get(0)
                    .map_or(String::new(), |m| m.as_str().to_string())
            } else {
                format!("{}{}[REDACTED]", &caps["key"], &caps["sep"])
            }
        })
        .into_owned();
    let out = GH_TOKEN_RE
        .replace_all(&out, "[REDACTED_GH_TOKEN]")
        .into_owned();
    let out = OPENAI_TOKEN_RE
        .replace_all(&out, "[REDACTED_OPENAI_KEY]")
        .into_owned();
    let out = AWS_ACCESS_KEY_ID_RE
        .replace_all(&out, "[REDACTED_AWS_ACCESS_KEY_ID]")
        .into_owned();
    if let Some(cfg) = cfg {
        cfg.denylist_regexes.iter().fold(out, |acc, re| {
            re.replace_all(&acc, "[REDACTED_CUSTOM]").into_owned()
        })
    } else {
        out
    }
}

/// Run a command, return trimmed stdout or None on failure.
pub fn cmd_stdout(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Run a command, return trimmed stdout or die.
pub fn cmd_stdout_or_die(program: &str, args: &[&str], context: &str) -> String {
    cmd_stdout(program, args).unwrap_or_else(|| die(context))
}

/// Default branch on `origin` (commonly `main` or `master`).
///
/// Uses `refs/remotes/origin/HEAD` when present; otherwise checks for
/// `origin/main` and `origin/master`. Falls back to `"main"`.
pub fn origin_default_branch() -> String {
    if let Some(sym) = cmd_stdout(
        "git",
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) && let Some(short) = trim_origin_head_to_branch(&sym)
    {
        return short.to_string();
    }
    for name in ["main", "master"] {
        if cmd_stdout(
            "git",
            &[
                "rev-parse",
                "--quiet",
                "--verify",
                &format!("refs/remotes/origin/{name}"),
            ],
        )
        .is_some()
        {
            return name.to_string();
        }
    }
    "main".to_string()
}

fn trim_origin_head_to_branch(sym: &str) -> Option<&str> {
    let s = sym.trim();
    s.strip_prefix("refs/remotes/origin/")
        .filter(|b| !b.is_empty())
}

/// Run a command, inheriting stdio. Returns success bool.
pub fn cmd_run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command in a specific directory, inheriting stdio. Returns success bool.
pub fn cmd_run_in(program: &str, args: &[&str], dir: &Path) -> bool {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command with extra env vars set, inheriting stdio. Returns success bool.
pub fn cmd_run_env(program: &str, args: &[&str], env: &[(String, String)]) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Run a command, capture combined stdout+stderr. Returns (success, output).
pub fn cmd_capture(program: &str, args: &[&str]) -> (bool, String) {
    match Command::new(program)
        .args(args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
    {
        Ok(o) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.success(), combined)
        }
        Err(e) => (false, e.to_string()),
    }
}

pub fn has_command(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn list_all_files(root: &str) -> Vec<String> {
    cmd_stdout(
        "git",
        &[
            "-C",
            root,
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .unwrap_or_default()
    .lines()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{configure_log_redaction, sanitize_log_message, trim_origin_head_to_branch};
    use cli_common::LogRedactionConfigFile;
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static LOG_REDACTION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn lock_redaction_tests() -> MutexGuard<'static, ()> {
        LOG_REDACTION_TEST_LOCK
            .lock()
            .expect("log redaction test mutex poisoned")
    }

    fn reset_redaction_config() {
        configure_log_redaction(&LogRedactionConfigFile::default());
    }

    #[test]
    fn redacts_json_secret_fields() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        let input = r#"claude: {"api_key":"supersecret123","token":"abc123"}"#;
        let out = sanitize_log_message(input);
        assert!(!out.contains("supersecret123"));
        assert!(!out.contains("abc123"));
        assert!(out.contains(r#""api_key":"[REDACTED]""#));
        assert!(out.contains(r#""token":"[REDACTED]""#));
    }

    #[test]
    fn redacts_bearer_auth() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let out = sanitize_log_message(input);
        assert!(!out.contains("eyJhbGci"));
        assert!(out.contains("Authorization: Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_provider_token_prefixes() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        let input = "tokens ghp_abcdefghijklmnopqrstuvwxyz123456 and sk-proj-abcDEF1234567890";
        let out = sanitize_log_message(input);
        assert!(out.contains("[REDACTED_GH_TOKEN]"));
        assert!(out.contains("[REDACTED_OPENAI_KEY]"));
    }

    #[test]
    fn redacts_private_key_blocks() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        let input = "BEGIN\n-----BEGIN PRIVATE KEY-----\nabc123\n-----END PRIVATE KEY-----\nEND";
        let out = sanitize_log_message(input);
        assert!(!out.contains("abc123"));
        assert!(out.contains("[REDACTED_PRIVATE_KEY]"));
    }

    #[test]
    fn redacts_custom_denylist_patterns() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        configure_log_redaction(&LogRedactionConfigFile {
            denylist_patterns: vec![r#"orgsec_[A-Za-z0-9]{6,}"#.to_string()],
            allowlist_keys: vec![],
        });
        let out = sanitize_log_message("token orgsec_AbCdEf1234");
        assert!(out.contains("[REDACTED_CUSTOM]"));
    }

    #[test]
    fn honors_allowlist_keys() {
        let _guard = lock_redaction_tests();
        reset_redaction_config();
        configure_log_redaction(&LogRedactionConfigFile {
            denylist_patterns: vec![],
            allowlist_keys: vec!["token".to_string()],
        });
        let out = sanitize_log_message(r#"{"token":"not_a_secret"}"#);
        assert!(out.contains(r#""token":"not_a_secret""#));
    }

    #[test]
    fn trim_origin_head_extracts_branch_short_name() {
        assert_eq!(
            trim_origin_head_to_branch("refs/remotes/origin/main"),
            Some("main")
        );
        assert_eq!(
            trim_origin_head_to_branch("refs/remotes/origin/master\n"),
            Some("master")
        );
        assert_eq!(trim_origin_head_to_branch("refs/heads/main"), None);
    }
}
