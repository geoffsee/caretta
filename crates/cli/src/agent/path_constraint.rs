/// Path-constraint enforcement for agent tool calls.
///
/// Caretta records every tool call emitted by the agent during a run.
/// After the run completes, `check_run` inspects those records against the
/// active `PathConstraints` and returns any policy violations.  Violations
/// are appended to the SQLite event log alongside the run record so that
/// audit teams can review them post-hoc.
///
/// The agent is also instructed at invocation time (via an appended system
/// prompt fragment) not to touch files outside the declared scope —
/// `build_system_prompt_fragment` generates that text.
use cli_common::PathConstraints;
use serde_json::Value;

use crate::agent::event_log::{PolicyViolation, ToolCallRecord};

/// Check all tool calls from a run against the active path constraints.
///
/// Returns every detected violation in declaration order. An empty result
/// means either no constraints are configured or all tool calls were in scope.
pub fn check_run(
    tool_calls: &[ToolCallRecord],
    constraints: &PathConstraints,
) -> Vec<PolicyViolation> {
    if constraints.is_unconstrained() {
        return Vec::new();
    }
    tool_calls
        .iter()
        .filter_map(|tc| check_tool_call(tc, constraints))
        .collect()
}

/// Build a system-prompt fragment that instructs the agent to respect path
/// constraints. Returns `None` when no constraints are configured so that
/// callers can skip the `--append-system-prompt-file` flag entirely.
pub fn build_system_prompt_fragment(constraints: &PathConstraints) -> Option<String> {
    if constraints.is_unconstrained() {
        return None;
    }
    let mut lines = vec!["Path constraints are active for this run.".to_string()];
    if !constraints.allow_paths.is_empty() {
        lines.push(format!(
            "Allowed path prefixes: {}",
            constraints.allow_paths.join(", ")
        ));
        lines.push(
            "Do not read, write, or modify any file whose path does not begin with \
             one of the allowed prefixes above."
                .to_string(),
        );
    }
    if !constraints.deny_paths.is_empty() {
        lines.push(format!(
            "Denied path prefixes: {}",
            constraints.deny_paths.join(", ")
        ));
        lines.push(
            "Do not read, write, or modify any file whose path begins with a denied prefix above."
                .to_string(),
        );
    }
    lines.push(
        "Always use the dedicated file tools (Read, Write, Edit, Glob, Grep) for \
         file access — do not use Bash or shell commands to read or write files, \
         as those cannot be audited for path-constraint compliance."
            .to_string(),
    );
    lines.push(
        "Accessing a path outside these constraints is a policy violation that will be \
         recorded in the audit log."
            .to_string(),
    );
    Some(lines.join("\n"))
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn check_tool_call(
    record: &ToolCallRecord,
    constraints: &PathConstraints,
) -> Option<PolicyViolation> {
    let path = extract_path_arg(&record.name, &record.args)?;
    let normalized = normalize_path(&path);

    // Deny list always wins, even if the path would pass the allow list.
    for denied in &constraints.deny_paths {
        if path_matches(&normalized, denied) {
            return Some(PolicyViolation {
                tool: record.name.clone(),
                path: path.clone(),
                reason: format!("path matches deny_paths entry: {denied}"),
            });
        }
    }

    // If an allow list is configured the path must match at least one entry.
    if !constraints.allow_paths.is_empty()
        && !constraints
            .allow_paths
            .iter()
            .any(|a| path_matches(&normalized, a))
    {
        return Some(PolicyViolation {
            tool: record.name.clone(),
            path: path.clone(),
            reason: format!(
                "path is outside allow_paths: [{}]",
                constraints.allow_paths.join(", ")
            ),
        });
    }

    None
}

/// Extract the file-path argument from a tool call, if the tool operates on a
/// specific path. Returns `None` for tools whose path arguments cannot be
/// reliably extracted.
///
/// # Limitations
/// - **Bash**: skipped entirely — shell commands embed paths as free-form text
///   that cannot be parsed without a full shell parser. Configure `deny_paths`
///   and system-prompt guidance to discourage shell-level access.
/// - **Glob**: only the `path` (search root) is inspected; the `pattern` arg
///   (e.g. `"vendor/**/*.rs"`) is not checked. Glob auditing is best-effort.
/// - **Grep**: when called without a `path` arg, the whole workspace is the
///   scope; this is represented as `"."` and checked against constraints.
fn extract_path_arg(tool_name: &str, args: &Value) -> Option<String> {
    match tool_name {
        "Read" | "Write" | "Edit" => args
            .get("file_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        "Glob" => args.get("path").and_then(Value::as_str).map(str::to_string),
        "Grep" => Some(
            args.get("path")
                .and_then(Value::as_str)
                .unwrap_or(".")
                .to_string(),
        ),
        _ => None,
    }
}

/// Collapse `..` and `.` components so that prefix matching cannot be defeated
/// by path traversal strings like `src/../vendor/secret.rs`.
///
/// Normalization rules:
/// - Leading `./` is stripped.
/// - `.` components are dropped.
/// - `..` pops the preceding directory segment (like a real filesystem would).
/// - Trailing `/` is stripped.
/// - Absolute paths (those with a leading `/` or Windows drive prefix) are
///   represented with a leading `"/"` sentinel so they never match a relative
///   allow/deny prefix. For example, `/src/main.rs` normalizes to `//src/main.rs`,
///   which will not match `allow_paths = ["src/"]`.
fn normalize_path(path: &str) -> String {
    use std::path::{Component, Path};
    let without_dot_prefix = path.trim_start_matches("./");
    let mut parts: Vec<&str> = Vec::new();
    for c in Path::new(without_dot_prefix).components() {
        match c {
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(s) => parts.push(s.to_str().unwrap_or("")),
            Component::RootDir | Component::Prefix(_) => {
                // Absolute path: use a sentinel so the normalized form starts
                // with "/" and can never match a relative prefix like "src/".
                // This prevents "/src/main.rs" from falsely satisfying
                // allow_paths = ["src/"].
                parts.clear();
                parts.push("/");
            }
            Component::CurDir => {} // already stripped by trim_start_matches("./") above
        }
    }
    parts.join("/")
}

/// Return `true` if `path` begins with `prefix` (prefix-match semantics).
/// `path` must already be normalized via `normalize_path`; `prefix` is
/// normalized here so that operators who write `deny_paths = ["src/../vendor/"]`
/// get the expected behaviour rather than a silently-inert rule.
fn path_matches(path: &str, prefix: &str) -> bool {
    let norm_prefix = normalize_path(prefix);
    if norm_prefix.is_empty() {
        // A prefix that normalizes to "" (e.g. ".", "./", "") would match
        // everything; treat it as non-matching so a degenerate entry cannot
        // accidentally bypass constraints.
        return false;
    }
    // Exact match or path is inside the prefix directory.
    path == norm_prefix || path.starts_with(&format!("{norm_prefix}/"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::event_log::ToolCallRecord;
    use cli_common::PathConstraints;

    fn tc(name: &str, key: &str, val: &str) -> ToolCallRecord {
        ToolCallRecord {
            name: name.to_string(),
            args: serde_json::json!({ key: val }),
        }
    }

    #[test]
    fn allow_paths_blocks_out_of_scope_read() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        let v = check_tool_call(&tc("Read", "file_path", "vendor/foo.rs"), &c)
            .expect("should detect violation");
        assert_eq!(v.tool, "Read");
        assert!(v.reason.contains("allow_paths"));
    }

    #[test]
    fn allow_paths_permits_in_scope_write() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        assert!(check_tool_call(&tc("Write", "file_path", "src/main.rs"), &c).is_none());
    }

    #[test]
    fn deny_paths_blocks_despite_allowlist() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec!["src/secrets/".to_string()],
        };
        let v = check_tool_call(&tc("Edit", "file_path", "src/secrets/key.pem"), &c)
            .expect("should detect violation");
        assert!(v.reason.contains("deny_paths"));
    }

    #[test]
    fn unconstrained_check_run_returns_empty() {
        let calls = vec![tc("Read", "file_path", "/etc/passwd")];
        assert!(check_run(&calls, &PathConstraints::default()).is_empty());
    }

    #[test]
    fn non_path_tool_is_not_checked() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        let record = ToolCallRecord {
            name: "Bash".to_string(),
            args: serde_json::json!({"command": "rm -rf /"}),
        };
        assert!(check_tool_call(&record, &c).is_none());
    }

    #[test]
    fn build_prompt_fragment_none_when_unconstrained() {
        assert!(build_system_prompt_fragment(&PathConstraints::default()).is_none());
    }

    #[test]
    fn build_prompt_fragment_contains_allow_paths() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string(), "tests/".to_string()],
            deny_paths: vec![],
        };
        let frag = build_system_prompt_fragment(&c).expect("should produce fragment");
        assert!(frag.contains("src/"));
        assert!(frag.contains("tests/"));
        assert!(frag.contains("Allowed path prefixes"));
    }

    #[test]
    fn build_prompt_fragment_contains_deny_paths() {
        let c = PathConstraints {
            allow_paths: vec![],
            deny_paths: vec!["vendor/".to_string()],
        };
        let frag = build_system_prompt_fragment(&c).expect("should produce fragment");
        assert!(frag.contains("vendor/"));
        assert!(frag.contains("Denied path prefixes"));
    }

    #[test]
    fn path_matches_prefix_semantics() {
        assert!(path_matches("src/main.rs", "src/"));
        assert!(path_matches("src", "src/"));
        assert!(!path_matches("srcx/main.rs", "src/"));
        assert!(!path_matches("vendor/foo.rs", "src/"));
        assert!(path_matches("tests/unit/foo.rs", "tests/"));
    }

    #[test]
    fn check_run_collects_multiple_violations() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        let calls = vec![
            tc("Read", "file_path", "src/ok.rs"),
            tc("Edit", "file_path", "vendor/bad.rs"),
            tc("Write", "file_path", "/etc/hosts"),
        ];
        let violations = check_run(&calls, &c);
        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].path, "vendor/bad.rs");
        assert_eq!(violations[1].path, "/etc/hosts");
    }

    #[test]
    fn glob_and_grep_path_args_are_checked() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        let glob_outside = tc("Glob", "path", "vendor/");
        let grep_inside = tc("Grep", "path", "src/");
        assert!(check_tool_call(&glob_outside, &c).is_some());
        assert!(check_tool_call(&grep_inside, &c).is_none());
    }

    #[test]
    fn path_traversal_is_caught() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        // src/../vendor/secret.rs normalizes to vendor/secret.rs — not in allow_paths.
        let v = check_tool_call(&tc("Read", "file_path", "src/../vendor/secret.rs"), &c)
            .expect("traversal should produce a violation");
        assert!(v.reason.contains("allow_paths"));

        // src/../../etc/passwd normalizes to etc/passwd — also not in allow_paths.
        let v2 = check_tool_call(&tc("Read", "file_path", "src/../../etc/passwd"), &c)
            .expect("double traversal should produce a violation");
        assert!(v2.reason.contains("allow_paths"));
    }

    #[test]
    fn traversal_cannot_bypass_deny_paths() {
        let c = PathConstraints {
            allow_paths: vec![],
            deny_paths: vec!["vendor/".to_string()],
        };
        // src/../vendor/lib.rs normalizes to vendor/lib.rs — matches deny_paths.
        let v = check_tool_call(&tc("Write", "file_path", "src/../vendor/lib.rs"), &c)
            .expect("traversal into deny_paths should produce a violation");
        assert!(v.reason.contains("deny_paths"));
    }

    #[test]
    fn grep_without_path_is_whole_workspace() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        // Grep with no path arg defaults to "." (whole workspace), which is
        // outside the allow_paths prefix → violation.
        let record = ToolCallRecord {
            name: "Grep".to_string(),
            args: serde_json::json!({"pattern": "secret"}),
        };
        assert!(check_tool_call(&record, &c).is_some());
    }

    #[test]
    fn grep_with_explicit_path_inside_allow() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        assert!(check_tool_call(&tc("Grep", "path", "src/"), &c).is_none());
    }

    #[test]
    fn absolute_path_is_not_allowed_by_relative_allow_path() {
        let c = PathConstraints {
            allow_paths: vec!["src/".to_string()],
            deny_paths: vec![],
        };
        // An absolute path like /src/main.rs must not pass allow_paths = ["src/"].
        // Without the sentinel fix, "/src/main.rs" would normalize to "src/main.rs"
        // and falsely satisfy the allow-list.
        let v = check_tool_call(&tc("Read", "file_path", "/src/main.rs"), &c)
            .expect("absolute path should be a violation even when it looks like an allowed prefix");
        assert!(v.reason.contains("allow_paths"));
    }

    #[test]
    fn deny_path_with_traversal_components_still_fires() {
        let c = PathConstraints {
            allow_paths: vec![],
            deny_paths: vec!["src/../vendor/".to_string()],
        };
        // "src/../vendor/" normalizes to "vendor", so it should still catch vendor/foo.rs.
        let v = check_tool_call(&tc("Read", "file_path", "vendor/foo.rs"), &c)
            .expect("traversal-containing deny prefix should normalize and still match");
        assert!(v.reason.contains("deny_paths"));
    }
}
