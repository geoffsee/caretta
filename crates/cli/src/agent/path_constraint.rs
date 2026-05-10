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
/// specific path. Returns `None` for tools (e.g. `Bash`) whose arguments are
/// too complex to parse reliably.
fn extract_path_arg(tool_name: &str, args: &Value) -> Option<String> {
    match tool_name {
        "Read" | "Write" | "Edit" => args
            .get("file_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        "Glob" | "Grep" => args.get("path").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// Strip a leading `./` and trailing `/` so that prefix matching is consistent
/// regardless of whether the agent emits `src/`, `./src`, or `src`.
fn normalize_path(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

/// Return `true` if `path` begins with `prefix` (prefix-match semantics).
/// Both `path` and `prefix` are already normalized (no leading `./`, no
/// trailing `/`).
fn path_matches(path: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_start_matches("./").trim_end_matches('/');
    if prefix.is_empty() {
        return true;
    }
    // Exact match or path is inside the prefix directory.
    path == prefix || path.starts_with(&format!("{prefix}/"))
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
}
