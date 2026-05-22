use crate::agent::types::{
    DEFAULT_ISSUE_SKILL_REPO_PATH, DEFAULT_USER_PERSONAS_REPO_PATH,
    DOT_CARETTA_ISSUE_SKILL_REPO_PATH, DOT_CARETTA_USER_PERSONAS_REPO_PATH, SkillPaths,
    SkillPathsFile,
};
use rust_embed::RustEmbed;
use std::path::{Path, PathBuf};

pub const AGENTS_MD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/AGENTS.md"));
pub const LABELS_YML: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/labels.yml"));
pub const AVAILABLE_MODELS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/available-models.json"
));

#[derive(RustEmbed)]
#[folder = "assets/skills/"]
pub struct SkillAssets;

#[derive(RustEmbed)]
#[folder = "assets/workflows/"]
pub struct WorkflowAssets;

/// Return the stable app-data directory for materialized assets
/// (`~/.local/share/caretta`). Created on first call if missing.
pub fn assets_dir() -> PathBuf {
    #[cfg(not(target_arch = "wasm32"))]
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));

    #[cfg(target_arch = "wasm32")]
    let base = PathBuf::from(".");

    let dir = base.join("caretta");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Resolve skill paths declared in `[skills]` merged with sane defaults:
/// check **`.caretta/skills/...`** first (recommended for forks / consumer repos), then the
/// upstream **`assets/skills/...`** tree when those files exist in the git checkout, otherwise
/// use the materialized bundled copy under `material_skills_root` (normally
/// [`assets_dir`] `join("skills")`) so workflows run without any repo-local skill tree.
///
/// Explicit `caretta.toml` overrides (`user_personas` / `issue_tracking`) are kept verbatim.
pub fn resolve_skill_paths(repo_root: &Path, skills_file: SkillPathsFile) -> SkillPaths {
    resolve_skill_paths_with_roots(repo_root, skills_file, &assets_dir().join("skills"), None)
}

/// Workspace-aware variant of [`resolve_skill_paths`]. When `workspace` is
/// `Some(name)`, candidates under `<repo>/.caretta/workspaces/<name>/skills/`
/// are checked first so users can override skills per workspace without
/// touching the shared `.caretta/skills/` tree.
pub fn resolve_skill_paths_with_workspace(
    repo_root: &Path,
    skills_file: SkillPathsFile,
    workspace: Option<&str>,
) -> SkillPaths {
    resolve_skill_paths_with_roots(
        repo_root,
        skills_file,
        &assets_dir().join("skills"),
        workspace,
    )
}

pub(crate) fn resolve_skill_paths_with_roots(
    repo_root: &Path,
    skills_file: SkillPathsFile,
    material_skills_root: &Path,
    workspace: Option<&str>,
) -> SkillPaths {
    fn pick(
        repo_root: &Path,
        configured: Option<String>,
        repo_candidate_paths: &[String],
        material_file: PathBuf,
    ) -> String {
        if let Some(path) = configured {
            return path;
        }
        for rel in repo_candidate_paths {
            if repo_root.join(rel).is_file() {
                return rel.clone();
            }
        }
        material_file
            .canonicalize()
            .unwrap_or(material_file)
            .to_string_lossy()
            .into_owned()
    }

    // Build the candidate list, prepending workspace-local paths when a
    // workspace is selected so per-workspace overrides win without altering
    // shared `.caretta/skills/` content.
    let issue_candidates: Vec<String> = workspace
        .map(|ws| {
            vec![format!(
                ".caretta/workspaces/{ws}/skills/issue-tracking/SKILL.md"
            )]
        })
        .unwrap_or_default()
        .into_iter()
        .chain([
            DOT_CARETTA_ISSUE_SKILL_REPO_PATH.to_string(),
            DEFAULT_ISSUE_SKILL_REPO_PATH.to_string(),
        ])
        .collect();
    let personas_candidates: Vec<String> = workspace
        .map(|ws| {
            vec![format!(
                ".caretta/workspaces/{ws}/skills/user-personas/SKILL.md"
            )]
        })
        .unwrap_or_default()
        .into_iter()
        .chain([
            DOT_CARETTA_USER_PERSONAS_REPO_PATH.to_string(),
            DEFAULT_USER_PERSONAS_REPO_PATH.to_string(),
        ])
        .collect();

    SkillPaths {
        issue_tracking: pick(
            repo_root,
            skills_file.issue_tracking,
            &issue_candidates,
            material_skills_root.join("issue-tracking/SKILL.md"),
        ),
        user_personas: pick(
            repo_root,
            skills_file.user_personas,
            &personas_candidates,
            material_skills_root.join("user-personas/SKILL.md"),
        ),
    }
}

/// Materialize embedded AGENTS.md and skills into the app-data directory.
/// Existing files are refreshed so the bundled guidance stays in sync with
/// the current binary.
/// Returns the app-data root (e.g. `~/.local/share/caretta`).
pub fn materialize_assets() -> PathBuf {
    let dir = assets_dir();

    // 1. AGENTS.md
    let agents_md = dir.join("AGENTS.md");
    let _ = std::fs::write(&agents_md, AGENTS_MD.as_bytes());

    // 2. Skills
    for file in SkillAssets::iter() {
        let path = dir.join("skills").join(file.as_ref());
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(embedded) = SkillAssets::get(file.as_ref()) {
            let _ = std::fs::write(&path, embedded.data);
        }
    }

    // 3. Workflows
    for file in WorkflowAssets::iter() {
        let path = dir.join("workflows").join(file.as_ref());
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(embedded) = WorkflowAssets::get(file.as_ref()) {
            let _ = std::fs::write(&path, embedded.data);
        }
    }

    dir
}

#[cfg(test)]
mod skill_path_resolve_tests {
    use super::*;
    use std::fs;

    #[test]
    fn uses_repo_relative_when_issue_skill_present() {
        let repo = tempfile::tempdir().expect("repo tempdir");
        let rel = repo.path().join(DEFAULT_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(rel.parent().expect("skill parent")).expect("mkdir");
        fs::write(&rel, "local skill").expect("write skill");

        let mirror = tempfile::tempdir().expect("mirror tempdir");

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            mirror.path(),
            None,
        );

        assert_eq!(sp.issue_tracking, DEFAULT_ISSUE_SKILL_REPO_PATH);
    }

    #[test]
    fn uses_dot_caretta_layout_when_present() {
        let repo = tempfile::tempdir().expect("repo tempdir");
        let rel = repo.path().join(DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(rel.parent().expect("skill parent")).expect("mkdir");
        fs::write(&rel, "forked skill").expect("write skill");

        let mirror = tempfile::tempdir().expect("mirror tempdir");

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            mirror.path(),
            None,
        );

        assert_eq!(sp.issue_tracking, DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        assert_eq!(
            fs::read_to_string(repo.path().join(&sp.issue_tracking)).unwrap(),
            "forked skill"
        );
    }

    #[test]
    fn prefers_dot_caretta_over_assets_when_both_exist() {
        let repo = tempfile::tempdir().expect("repo");
        let dot = repo.path().join(DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        let leg = repo.path().join(DEFAULT_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(dot.parent().expect("p")).expect("md");
        fs::create_dir_all(leg.parent().expect("p")).expect("md");
        fs::write(&dot, "dot wins").unwrap();
        fs::write(&leg, "legacy").unwrap();

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            tempfile::tempdir().unwrap().path(),
            None,
        );
        assert_eq!(sp.issue_tracking, DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        assert_eq!(
            fs::read_to_string(repo.path().join(&sp.issue_tracking)).unwrap(),
            "dot wins"
        );
    }

    #[test]
    fn falls_back_to_materialized_path_when_repo_lacks_assets_skills() {
        let repo = tempfile::tempdir().expect("repo tempdir");

        let mirror = tempfile::tempdir().expect("mirror tempdir");
        for (sub, body) in [
            ("issue-tracking/SKILL.md", "bundled issue"),
            ("user-personas/SKILL.md", "bundled personas"),
        ] {
            let p = mirror.path().join(sub);
            fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            fs::write(&p, body).expect("write mirror skill");
        }

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            mirror.path(),
            None,
        );

        assert_eq!(
            fs::read_to_string(&sp.issue_tracking).expect("read issue skill"),
            "bundled issue"
        );
        assert_eq!(
            fs::read_to_string(&sp.user_personas).expect("read personas skill"),
            "bundled personas"
        );
    }

    #[test]
    fn caretta_toml_paths_win_over_repo_and_mirror() {
        let repo = tempfile::tempdir().expect("repo");
        let mirror = tempfile::tempdir().expect("mirror");

        let repo_issue = repo.path().join(DEFAULT_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(repo_issue.parent().expect("p")).expect("md");
        fs::write(&repo_issue, "local").expect("write repo skill");

        let mirrored = mirror.path().join("issue-tracking/SKILL.md");
        fs::create_dir_all(mirrored.parent().expect("p")).expect("md");
        fs::write(&mirrored, "mirror").expect("w");

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile {
                issue_tracking: Some("/custom/issue.md".into()),
                user_personas: Some("/custom/personas.md".into()),
            },
            mirror.path(),
            None,
        );

        assert_eq!(sp.issue_tracking, "/custom/issue.md");
        assert_eq!(sp.user_personas, "/custom/personas.md");
    }

    #[test]
    fn workspace_local_skill_files_win_over_dot_caretta_and_mirror() {
        let repo = tempfile::tempdir().expect("repo tempdir");

        // Layer 1: dot-caretta default (should lose to workspace override).
        let dot = repo.path().join(DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(dot.parent().expect("p")).expect("md");
        fs::write(&dot, "dot wins normally").expect("w");

        // Layer 0: workspace-local override.
        let ws_rel = ".caretta/workspaces/alpha/skills/issue-tracking/SKILL.md";
        let ws_path = repo.path().join(ws_rel);
        fs::create_dir_all(ws_path.parent().expect("p")).expect("md");
        fs::write(&ws_path, "workspace wins").expect("w");

        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            tempfile::tempdir().unwrap().path(),
            Some("alpha"),
        );

        assert_eq!(sp.issue_tracking, ws_rel);
        assert_eq!(
            fs::read_to_string(repo.path().join(&sp.issue_tracking)).unwrap(),
            "workspace wins"
        );
    }

    #[test]
    fn unknown_workspace_falls_back_to_default_resolution() {
        let repo = tempfile::tempdir().expect("repo tempdir");
        let dot = repo.path().join(DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
        fs::create_dir_all(dot.parent().expect("p")).expect("md");
        fs::write(&dot, "dot wins").expect("w");

        // No files under `.caretta/workspaces/missing/...` exist, so the
        // resolver must fall through to the existing `.caretta/skills/`
        // location and not invent paths that point at nothing.
        let sp = resolve_skill_paths_with_roots(
            repo.path(),
            SkillPathsFile::default(),
            tempfile::tempdir().unwrap().path(),
            Some("missing"),
        );
        assert_eq!(sp.issue_tracking, DOT_CARETTA_ISSUE_SKILL_REPO_PATH);
    }
}
