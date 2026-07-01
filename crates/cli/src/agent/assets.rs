// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::types::{
    DEFAULT_ISSUE_SKILL_REPO_PATH, DEFAULT_USER_PERSONAS_REPO_PATH,
    DOT_CARETTA_ISSUE_SKILL_REPO_PATH, DOT_CARETTA_PERSONAS_DIR,
    DOT_CARETTA_USER_PERSONAS_REPO_PATH, SkillPaths, SkillPathsFile,
};
use rust_embed::RustEmbed;
use std::path::{Path, PathBuf};

/// Embedded-skill path prefix (under `assets/skills/`) for the bundled
/// persona JSON cards. Used by [`materialize_assets`] to identify files
/// that should yield to a user-maintained `.caretta/personas/` directory
/// instead of being unconditionally rewritten on every launch.
const EMBEDDED_PERSONAS_PREFIX: &str = "user-personas/personas/";

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
///
/// When the current working directory contains at least one persona JSON
/// under `<cwd>/.caretta/personas/`, the bundled persona cards (embedded at
/// `assets/skills/user-personas/personas/*.json`) are **not** re-written
/// into app-data on this launch. This lets users durably delete embedded
/// personas — once any user-authored persona is present in
/// `.caretta/personas/`, the user's directory is treated as the source of
/// truth for the persona set and the bundled seeds stop being recreated.
/// All other assets (the `user-personas/SKILL.md`, `issue-tracking/`,
/// workflows, AGENTS.md, etc.) still materialize unconditionally.
///
/// Returns the app-data root (e.g. `~/.local/share/caretta`).
pub fn materialize_assets() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dir = assets_dir();
    materialize_assets_into(&dir, &cwd);
    dir
}

/// Testable form of [`materialize_assets`] that writes into an explicit
/// `app_data_dir` and consults `repo_root` to decide whether to skip
/// bundled persona JSONs. `app_data_dir` must already exist (the public
/// entry point creates it via [`assets_dir`]).
pub(crate) fn materialize_assets_into(app_data_dir: &Path, repo_root: &Path) {
    // 1. AGENTS.md
    let agents_md = app_data_dir.join("AGENTS.md");
    let _ = std::fs::write(&agents_md, AGENTS_MD.as_bytes());

    // 2. Skills (skipping bundled persona JSONs if the user already has
    //    personas in `<repo_root>/.caretta/personas/`).
    let skip_embedded_personas = user_personas_present(repo_root);
    for file in SkillAssets::iter() {
        let rel = file.as_ref();
        if skip_embedded_personas && rel.starts_with(EMBEDDED_PERSONAS_PREFIX) {
            continue;
        }
        let path = app_data_dir.join("skills").join(rel);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(embedded) = SkillAssets::get(rel) {
            let _ = std::fs::write(&path, embedded.data);
        }
    }

    // 3. Workflows
    for file in WorkflowAssets::iter() {
        let path = app_data_dir.join("workflows").join(file.as_ref());
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(embedded) = WorkflowAssets::get(file.as_ref()) {
            let _ = std::fs::write(&path, embedded.data);
        }
    }
}

/// Returns `true` when `<repo_root>/.caretta/personas/` contains at least
/// one `*.json` file. Treated as a signal that the user is curating their
/// own persona set, so bundled persona seeds should not be re-materialized
/// on this launch.
fn user_personas_present(repo_root: &Path) -> bool {
    let personas_dir = repo_root.join(DOT_CARETTA_PERSONAS_DIR);
    let Ok(entries) = std::fs::read_dir(&personas_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
            && entry.path().is_file()
    })
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

#[cfg(test)]
mod materialize_assets_tests {
    use super::*;
    use std::fs;

    /// Collect the relative paths of bundled persona JSON cards (under
    /// `assets/skills/user-personas/personas/`) so tests don't have to
    /// hardcode the current bundle.
    fn embedded_persona_files() -> Vec<String> {
        SkillAssets::iter()
            .filter(|p| p.starts_with(EMBEDDED_PERSONAS_PREFIX))
            .map(|p| p.into_owned())
            .collect()
    }

    #[test]
    fn user_personas_present_detects_only_json_files() {
        let dir = tempfile::tempdir().unwrap();
        // Missing dir → not present.
        assert!(!user_personas_present(dir.path()));

        // Empty dir → not present.
        let personas = dir.path().join(".caretta/personas");
        fs::create_dir_all(&personas).unwrap();
        assert!(!user_personas_present(dir.path()));

        // Non-json file alone is not enough; we only react to actual
        // persona JSON documents.
        fs::write(personas.join("README"), "ignored").unwrap();
        assert!(!user_personas_present(dir.path()));

        fs::write(personas.join("custom.json"), "{\"persona\":{}}").unwrap();
        assert!(user_personas_present(dir.path()));
    }

    #[test]
    fn materialize_writes_embedded_personas_when_user_dir_absent() {
        let app_data = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        materialize_assets_into(app_data.path(), repo.path());

        let bundled = embedded_persona_files();
        assert!(
            !bundled.is_empty(),
            "test fixture: bundle must ship at least one persona seed"
        );
        for rel in &bundled {
            let path = app_data.path().join("skills").join(rel);
            assert!(
                path.is_file(),
                "expected bundled persona to materialize at {}",
                path.display()
            );
        }
        // The SKILL.md companion must still land in app-data.
        assert!(
            app_data
                .path()
                .join("skills/user-personas/SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn materialize_skips_embedded_personas_when_user_dir_has_personas() {
        let app_data = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        // User has authored their own persona — bundled seeds must not be
        // resurrected on this launch.
        let personas = repo.path().join(".caretta/personas");
        fs::create_dir_all(&personas).unwrap();
        fs::write(
            personas.join("mine.json"),
            "{\"persona\":{\"name\":\"Mine\"}}",
        )
        .unwrap();

        materialize_assets_into(app_data.path(), repo.path());

        for rel in embedded_persona_files() {
            let path = app_data.path().join("skills").join(&rel);
            assert!(
                !path.exists(),
                "bundled persona must not be materialized when user has \
                 `.caretta/personas/`: {}",
                path.display()
            );
        }
        // SKILL.md and other skill assets still ship, so workflows that
        // reference the user-personas skill keep working.
        assert!(
            app_data
                .path()
                .join("skills/user-personas/SKILL.md")
                .is_file(),
            "user-personas SKILL.md must still materialize"
        );
    }

    #[test]
    fn materialize_does_not_overwrite_app_data_personas_when_user_dir_has_personas() {
        let app_data = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        let Some(seed) = embedded_persona_files().into_iter().next() else {
            // No bundled personas to clobber — nothing to assert here.
            return;
        };

        // Simulate a previous launch that materialized the bundled persona
        // and was subsequently modified (or removed) by the user. With the
        // user-managed `.caretta/personas/` populated, the second launch
        // must leave the existing app-data file alone — either preserving
        // a user-modified copy or honoring a deletion by not recreating
        // it.
        let target = app_data.path().join("skills").join(&seed);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "USER MODIFIED").unwrap();

        let personas = repo.path().join(".caretta/personas");
        fs::create_dir_all(&personas).unwrap();
        fs::write(
            personas.join("mine.json"),
            "{\"persona\":{\"name\":\"Mine\"}}",
        )
        .unwrap();

        materialize_assets_into(app_data.path(), repo.path());

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "USER MODIFIED",
            "materialize must not overwrite app-data persona when user has \
             `.caretta/personas/`"
        );
    }
}
