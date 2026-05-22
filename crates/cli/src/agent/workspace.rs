//! Context workspaces let users opt into project-local overrides for
//! presets, workflows, skills, discovery-framing, and personas.
//!
//! The feature is opt-in: a user creates `<repo>/.caretta/workspaces/` and
//! one or more subdirectories under it (each subdirectory is a named
//! workspace). When the directory does **not** exist, caretta proceeds with
//! its default resolution chain unchanged. When it does, the CLI either uses
//! the explicit `--workspace <name>` selection or, if running interactively,
//! shows a small picker.
//!
//! A selected workspace name becomes [`crate::agent::types::Config::workspace`]
//! and the asset resolvers (`assets::resolve_skill_paths_with_workspace`,
//! `workflow::*`, `ui::personas::personas_dir`, `ui::discovery::workspace_path`)
//! prepend `<root>/.caretta/workspaces/<name>/...` to their lookup chain.
//!
//! Per repository guidelines this module is intentionally scoped to user
//! repository state; it makes no changes under `.github/`.
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// Directory under the repository root that holds opt-in context workspaces.
pub const WORKSPACES_DIR_REL: &str = ".caretta/workspaces";

/// Absolute path to `<root>/.caretta/workspaces`.
pub fn workspaces_dir(root: &Path) -> PathBuf {
    root.join(WORKSPACES_DIR_REL)
}

/// Absolute path to a single workspace folder (no existence check).
pub fn workspace_root(root: &Path, name: &str) -> PathBuf {
    workspaces_dir(root).join(name)
}

/// Returns `true` when `<root>/.caretta/workspaces/` exists as a directory.
pub fn workspaces_enabled(root: &Path) -> bool {
    workspaces_dir(root).is_dir()
}

/// List workspace names (sorted, deduplicated). Returns an empty vector when
/// `<root>/.caretta/workspaces/` is missing or unreadable so the rest of the
/// pipeline can treat absence as "feature disabled".
pub fn list_workspaces(root: &Path) -> Vec<String> {
    let dir = workspaces_dir(root);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .filter(|n| !n.starts_with('.'))
                .map(|n| n.to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Interactive (TTY) picker over `stdin`/`stderr`. Returns the chosen
/// workspace name, `None` when the user opts out, or an `Err` when input
/// fails. The picker is intentionally small (no extra dependencies) so it
/// works in plain terminals as well as the CI smoke shell.
pub fn pick_workspace_interactive(workspaces: &[String]) -> io::Result<Option<String>> {
    if workspaces.is_empty() {
        return Ok(None);
    }

    let stderr = io::stderr();
    let mut err = stderr.lock();
    writeln!(err, "Detected context workspaces in .caretta/workspaces/:")?;
    writeln!(err, "  0) (none — use default context)")?;
    for (idx, name) in workspaces.iter().enumerate() {
        writeln!(err, "  {}) {}", idx + 1, name)?;
    }
    write!(err, "Select a workspace [0]: ")?;
    err.flush()?;

    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Ok(None);
    }

    // Accept either a numeric index or an exact workspace name to keep the
    // picker forgiving when users tab-complete from shell history.
    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx == 0 {
            return Ok(None);
        }
        return Ok(workspaces.get(idx - 1).cloned());
    }
    Ok(workspaces
        .iter()
        .find(|name| name.as_str() == trimmed)
        .cloned())
}

/// Resolve the effective workspace name given an optional CLI flag.
///
/// Precedence:
/// 1. `explicit` (from `--workspace <NAME>`). An explicit empty string or the
///    sentinel `"none"` disables the feature for this invocation, even when
///    workspaces exist on disk.
/// 2. Otherwise, when `<root>/.caretta/workspaces/` exists and `interactive`
///    is `true`, run [`pick_workspace_interactive`].
/// 3. Otherwise, return `None` so the agent proceeds with default resolution.
pub fn select_workspace(root: &Path, explicit: Option<&str>, interactive: bool) -> Option<String> {
    if let Some(name) = explicit {
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return None;
        }
        return Some(name.to_string());
    }
    if !workspaces_enabled(root) {
        return None;
    }
    let names = list_workspaces(root);
    if names.is_empty() {
        return None;
    }
    if !interactive {
        return None;
    }
    pick_workspace_interactive(&names).unwrap_or(None)
}

/// Returns the relative path inside the workspace, joined onto the workspace
/// root, when the given workspace name is set. Returns `None` when no
/// workspace is selected.
pub fn workspace_relative(
    root: &Path,
    workspace: Option<&str>,
    relative: impl AsRef<Path>,
) -> Option<PathBuf> {
    workspace.map(|name| workspace_root(root, name).join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn list_workspaces_returns_empty_when_dir_missing() {
        let repo = tempdir().expect("tempdir");
        assert!(list_workspaces(repo.path()).is_empty());
        assert!(!workspaces_enabled(repo.path()));
    }

    #[test]
    fn list_workspaces_returns_sorted_directory_names() {
        let repo = tempdir().expect("tempdir");
        let base = repo.path().join(WORKSPACES_DIR_REL);
        fs::create_dir_all(base.join("zeta")).unwrap();
        fs::create_dir_all(base.join("alpha")).unwrap();
        fs::create_dir_all(base.join("beta")).unwrap();
        // A regular file should not be reported as a workspace.
        fs::write(base.join("README.md"), "ignored").unwrap();
        // Hidden dirs are skipped.
        fs::create_dir_all(base.join(".keep")).unwrap();

        let names = list_workspaces(repo.path());
        assert_eq!(names, vec!["alpha", "beta", "zeta"]);
        assert!(workspaces_enabled(repo.path()));
    }

    #[test]
    fn select_workspace_respects_explicit_flag() {
        let repo = tempdir().expect("tempdir");
        // Explicit selection wins even when the dir does not exist; downstream
        // resolvers will simply not find any overrides, which is the desired
        // behavior for forward-looking workspace names.
        assert_eq!(
            select_workspace(repo.path(), Some("custom"), false),
            Some("custom".to_string())
        );
    }

    #[test]
    fn select_workspace_disables_with_none_sentinel() {
        let repo = tempdir().expect("tempdir");
        let base = repo.path().join(WORKSPACES_DIR_REL);
        fs::create_dir_all(base.join("alpha")).unwrap();
        assert_eq!(select_workspace(repo.path(), Some("none"), false), None);
        assert_eq!(select_workspace(repo.path(), Some(""), false), None);
    }

    #[test]
    fn select_workspace_returns_none_without_interactive_and_without_flag() {
        let repo = tempdir().expect("tempdir");
        let base = repo.path().join(WORKSPACES_DIR_REL);
        fs::create_dir_all(base.join("alpha")).unwrap();
        // No flag, non-interactive → no picker is shown so the call is a
        // no-op and resolution proceeds normally.
        assert_eq!(select_workspace(repo.path(), None, false), None);
    }

    #[test]
    fn select_workspace_returns_none_when_no_workspaces_dir() {
        let repo = tempdir().expect("tempdir");
        assert_eq!(select_workspace(repo.path(), None, true), None);
    }

    #[test]
    fn workspace_relative_joins_path_when_set() {
        let repo = tempdir().expect("tempdir");
        let p = workspace_relative(repo.path(), Some("alpha"), "skills/user-personas/SKILL.md")
            .expect("Some path");
        assert!(p.ends_with(".caretta/workspaces/alpha/skills/user-personas/SKILL.md"));
        assert_eq!(workspace_relative(repo.path(), None, "skills/x"), None);
    }
}
