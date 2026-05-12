use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::assets::assets_dir;
use crate::agent::shell::{cmd_stdout, log};
use crate::agent::tracker::list_open_prs;
use crate::agent::types::Config;
use cli_common::PathConstraints;

pub const DEFAULT_PRESET_VERSION: &str = "0.1.0";
/// Sentinel prefix embedded in version-mismatch errors from `resolve_preset`.
/// Used by callers to distinguish hard version violations from other failures
/// without matching on free-form English text.
pub const VERSION_MISMATCH_TAG: &str = "version mismatch:";

// ── YAML config types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub pattern: ExecutionPattern,
    #[serde(default = "default_context")]
    pub context: String,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Named action from the registry. When set, the generic dispatch calls
    /// this action instead of `run_workflow_draft`.
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub extra_context: Vec<ExtraContextFetch>,
    #[serde(default)]
    pub phases: IndexMap<String, PhaseConfig>,
    #[serde(default)]
    pub fragments: HashMap<String, String>,
    /// Optional per-workflow path constraints. When set, these override the
    /// project-level `[path_constraints]` from `caretta.toml` for this
    /// workflow's runs. Omitting this field inherits the project-level config.
    #[serde(default)]
    pub path_constraints: Option<PathConstraints>,
}

/// Fetch the body of a GitHub issue by its label and inject it as a template variable.
#[derive(Debug, Deserialize)]
pub struct ExtraContextFetch {
    /// Template variable name to inject (e.g. "report_synthesis").
    pub name: String,
    /// GitHub issue label to search for (e.g. "uxr-synthesis").
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPattern {
    TwoPhase,
    OneShot,
    MultiRound,
    Implementation,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_order")]
    pub order: u32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub requires_bot: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            category: default_category(),
            order: default_order(),
            visible: true,
            requires_bot: false,
        }
    }
}

/// Lightweight summary of a workflow for the UI sidebar.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub order: u32,
    pub requires_bot: bool,
}

/// Top-level manifest for a preset directory (`preset.yaml`).
#[derive(Debug, Deserialize)]
pub struct PresetManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
}

fn category_rank(category: &str) -> (u8, &str) {
    match category {
        "discovery" => (0, category),
        "planning" => (1, category),
        "review" => (2, category),
        "maintenance" => (3, category),
        _ => (4, category),
    }
}

/// Load all workflow configs and return sorted sidebar entries for a preset.
pub fn load_sidebar_entries(root: &str, preset: &str) -> Vec<WorkflowEntry> {
    let workflows = load_workflows(root, preset);
    let mut entries: Vec<WorkflowEntry> = workflows
        .values()
        .filter(|wf| wf.ui.visible)
        .map(|wf| WorkflowEntry {
            id: wf.id.clone(),
            name: wf.name.clone(),
            category: wf.ui.category.clone(),
            order: wf.ui.order,
            requires_bot: wf.ui.requires_bot,
        })
        .collect();
    entries.sort_by(|a, b| {
        category_rank(&a.category)
            .cmp(&category_rank(&b.category))
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

#[derive(Debug, Deserialize)]
pub struct PhaseConfig {
    pub template: String,
    #[serde(default)]
    pub log_start: String,
    #[serde(default)]
    pub log_complete: String,
}

fn default_context() -> String {
    "none".to_string()
}
fn default_category() -> String {
    "other".to_string()
}
fn default_order() -> u32 {
    99
}
fn default_true() -> bool {
    true
}

fn materialized_workflows_dir() -> PathBuf {
    assets_dir().join("workflows")
}

fn bundled_workflows_dir(root: &str) -> PathBuf {
    Path::new(root).join("assets/workflows")
}

fn local_workflows_dir(root: &str) -> PathBuf {
    Path::new(root).join(".agents/workflows")
}

fn preset_dir_roots(root: &str) -> Vec<PathBuf> {
    vec![
        materialized_workflows_dir(),
        bundled_workflows_dir(root),
        local_workflows_dir(root),
    ]
}

/// Returns preset directories in low-to-high priority order: materialized, bundled, local.
/// Callers that use last-insert-wins (e.g. `HashMap::insert`) iterate forwards.
/// Callers that use first-match-wins iterate `.rev()` so local still wins.
fn preset_dirs(root: &str, preset: &str) -> Vec<PathBuf> {
    vec![
        materialized_workflows_dir().join(preset),
        bundled_workflows_dir(root).join(preset),
        local_workflows_dir(root).join(preset),
    ]
}

pub fn preset_skill_dirs(root: &str, preset: &str) -> Vec<PathBuf> {
    preset_dirs(root, preset)
        .into_iter()
        .map(|p| p.join("skills"))
        .collect()
}

// ── Loader ───────────────────────────────────────────────────────────────

/// List available preset names by scanning bundled and project-local workflow roots.
pub fn list_presets(root: &str) -> Vec<String> {
    let mut presets = Vec::new();
    for base in preset_dir_roots(root) {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && path
                        .join(".")
                        .read_dir()
                        .is_ok_and(|mut d| d.next().is_some())
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    presets.push(name.to_string());
                }
            }
        }
    }
    presets.sort();
    presets.dedup();
    // Ensure "default" comes first if present.
    if let Some(pos) = presets.iter().position(|p| p == "default") {
        presets.remove(pos);
        presets.insert(0, "default".to_string());
    }
    presets
}

/// Scan bundled and project-local workflow directories for the selected preset.
/// Project-local workflows override bundled workflows with the same `id`.
pub fn load_workflows(root: &str, preset: &str) -> HashMap<String, WorkflowConfig> {
    let mut map = HashMap::new();
    for base in preset_dirs(root, preset) {
        let entries = match std::fs::read_dir(&base) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let yaml_path = path.join("workflow.yaml");
            let content = match std::fs::read_to_string(&yaml_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            match serde_yaml::from_str::<WorkflowConfig>(&content) {
                Ok(wf) => {
                    map.insert(wf.id.clone(), wf);
                }
                Err(e) => {
                    log(&format!(
                        "WARNING: failed to parse {}: {e}",
                        yaml_path.display()
                    ));
                }
            }
        }
    }
    map
}

/// Read a prompt template file from a workflow directory within a preset.
pub fn load_template(root: &str, preset: &str, workflow_dir: &str, filename: &str) -> String {
    for base in [
        local_workflows_dir(root),
        bundled_workflows_dir(root),
        materialized_workflows_dir(),
    ] {
        let path = base.join(preset).join(workflow_dir).join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return content;
        }
    }
    let path = local_workflows_dir(root)
        .join(preset)
        .join(workflow_dir)
        .join(filename);
    log(&format!(
        "WARNING: failed to read template {}",
        path.display()
    ));
    String::new()
}

// ── Preset manifest ──────────────────────────────────────────────────────

/// Load the `preset.yaml` manifest for `preset_name`.
/// Searches local, bundled, and materialized directories in priority order
/// (local overrides bundled; bundled overrides materialized).
/// First-found wins: if the first manifest found has an empty version field,
/// `DEFAULT_PRESET_VERSION` is applied to that manifest without falling through
/// to lower-priority directories.
/// Falls back to `DEFAULT_PRESET_VERSION` and logs a deprecation warning when
/// no manifest is found at all.
pub fn load_preset_manifest(root: &str, preset_name: &str) -> PresetManifest {
    if preset_name.contains('/') || preset_name.contains('\\') || preset_name.contains('.') {
        log(&format!(
            "WARNING: unsafe preset name '{preset_name}', defaulting version"
        ));
        return PresetManifest {
            name: String::new(),
            version: DEFAULT_PRESET_VERSION.to_string(),
        };
    }
    for dir in preset_dirs(root, preset_name).into_iter().rev() {
        let path = dir.join("preset.yaml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            match serde_yaml::from_str::<PresetManifest>(&content) {
                Ok(m) if !m.version.trim().is_empty() => return m,
                Ok(m) => {
                    log(&format!(
                        "DEPRECATION: preset '{preset_name}' preset.yaml has no version field; defaulting to {DEFAULT_PRESET_VERSION}"
                    ));
                    return PresetManifest {
                        name: m.name,
                        version: DEFAULT_PRESET_VERSION.to_string(),
                    };
                }
                Err(e) => {
                    log(&format!(
                        "ERROR: failed to parse {} (corrupt preset.yaml — falling back to next directory): {e}",
                        path.display()
                    ));
                }
            }
        }
    }
    log(&format!(
        "DEPRECATION: preset '{preset_name}' has no preset.yaml; defaulting to version {DEFAULT_PRESET_VERSION}"
    ));
    PresetManifest {
        name: String::new(),
        version: DEFAULT_PRESET_VERSION.to_string(),
    }
}

/// Parse a preset reference into `(name, version_requirement_string)`.
/// Supports bare `"name"` or `"name@req"` (e.g. `"default@0.1.0"`, `"xp@>=1.0.0"`).
/// Returns `Err` if the name contains path-traversal characters (`/`, `\`, `.`).
pub fn parse_preset_ref(preset_ref: &str) -> Result<(String, Option<String>), String> {
    let (raw_name, req) = match preset_ref.split_once('@') {
        Some((n, r)) => {
            let req = r.trim();
            if req.is_empty() {
                return Err("Version requirement after '@' must not be empty".to_string());
            }
            (n.trim(), Some(req.to_string()))
        }
        None => (preset_ref.trim(), None),
    };
    if raw_name.is_empty() {
        return Err("Preset name must not be empty".to_string());
    }
    if raw_name.contains('/') || raw_name.contains('\\') || raw_name.contains('.') {
        return Err(format!(
            "Invalid preset name '{raw_name}': must not contain path characters"
        ));
    }
    Ok((raw_name.to_string(), req))
}

/// Resolve a preset reference, returning `(preset_name, resolved_version)`.
///
/// When a version requirement is present (e.g. `"default@>=0.1.0"`), the
/// manifest version is validated against it.  A clear error is returned on
/// mismatch, showing both the required and found versions.
pub fn resolve_preset(root: &str, preset_ref: &str) -> Result<(String, String), String> {
    let (name, req_str) = parse_preset_ref(preset_ref)?;
    let manifest = load_preset_manifest(root, &name);

    let found = semver::Version::parse(manifest.version.trim()).map_err(|e| {
        format!(
            "Preset '{name}' has invalid version '{}': {e}",
            manifest.version
        )
    })?;

    if let Some(req_str) = req_str {
        let req = semver::VersionReq::parse(&req_str)
            .map_err(|e| format!("Invalid version requirement '{req_str}': {e}"))?;
        if !req.matches(&found) {
            return Err(format!(
                "Preset '{name}' {VERSION_MISMATCH_TAG} required {req_str}, found {found}"
            ));
        }
    }

    Ok((name, found.to_string()))
}

// ── Template rendering ───────────────────────────────────────────────────

/// Render a Handlebars template with the given variables and fragment partials.
pub fn render_prompt(
    template: &str,
    vars: &serde_json::Value,
    fragments: &HashMap<String, String>,
) -> Result<String, String> {
    let mut hbs = handlebars::Handlebars::new();
    hbs.set_strict_mode(false); // missing vars render as empty string

    for (name, body) in fragments {
        hbs.register_partial(name, body)
            .map_err(|e| format!("Fragment '{name}' parse error: {e}"))?;
    }

    hbs.render_template(template, vars)
        .map_err(|e| format!("Template render error: {e}"))
}

/// Convenience: load a workflow's phase template and render it.
pub fn load_and_render(
    root: &str,
    preset: &str,
    wf: &WorkflowConfig,
    phase: &str,
    vars: &serde_json::Value,
) -> Result<String, String> {
    let phase_cfg = wf
        .phases
        .get(phase)
        .ok_or_else(|| format!("No phase '{phase}' in workflow '{}'", wf.id))?;

    // Derive the directory name from the workflow id (underscore → hyphen)
    let dir = wf.id.replace('_', "-");
    let template = load_template(root, preset, &dir, &phase_cfg.template);
    if template.is_empty() {
        return Err(format!(
            "Empty template '{}' for workflow '{}'",
            phase_cfg.template, wf.id
        ));
    }
    render_prompt(&template, vars, &wf.fragments)
}

// ── Context gathering (JSON wrappers) ────────────────────────────────────

/// Gather context for the given gatherer name, returning a JSON object.
pub fn gather_context_as_json(cfg: &Config, gatherer: &str) -> serde_json::Value {
    match gatherer {
        "sprint" => {
            let open_issues = gh_open_issues(50);
            let open_prs = open_prs_json();
            let status = read_project_file(&cfg.root, "STATUS.md");
            let issues_md = read_project_file(&cfg.root, "ISSUES.md");
            serde_json::json!({
                "open_issues": open_issues,
                "open_prs": open_prs,
                "status": status,
                "issues_md": issues_md,
            })
        }
        "strategic" => {
            let open_issues = gh_open_issues(50);
            let open_prs = open_prs_json();
            let recent_commits = cmd_stdout("git", &["log", "--oneline", "--no-decorate", "-30"])
                .unwrap_or_default();
            let crate_tree =
                cmd_stdout("ls", &["-1", &format!("{}/crates", cfg.root)]).unwrap_or_default();
            let status = read_project_file(&cfg.root, "STATUS.md");
            let issues_md = read_project_file(&cfg.root, "ISSUES.md");
            serde_json::json!({
                "open_issues": open_issues,
                "open_prs": open_prs,
                "recent_commits": recent_commits,
                "crate_tree": crate_tree,
                "status": status,
                "issues_md": issues_md,
            })
        }
        "retro" => {
            let recent_commits = cmd_stdout("git", &["log", "--oneline", "--no-decorate", "-50"])
                .unwrap_or_default();
            let closed_issues = cmd_stdout(
                "gh",
                &[
                    "issue",
                    "list",
                    "--state",
                    "closed",
                    "--json",
                    "number,title,closedAt",
                    "--limit",
                    "30",
                ],
            )
            .unwrap_or_else(|| "[]".to_string());
            let merged_prs = cmd_stdout(
                "gh",
                &[
                    "pr",
                    "list",
                    "--state",
                    "merged",
                    "--json",
                    "number,title,mergedAt",
                    "--limit",
                    "30",
                ],
            )
            .unwrap_or_else(|| "[]".to_string());
            let open_issues = gh_open_issues(50);
            let open_prs = open_prs_json();
            let status = read_project_file(&cfg.root, "STATUS.md");
            let issues_md = read_project_file(&cfg.root, "ISSUES.md");
            serde_json::json!({
                "recent_commits": recent_commits,
                "closed_issues": closed_issues,
                "merged_prs": merged_prs,
                "open_issues": open_issues,
                "open_prs": open_prs,
                "status": status,
                "issues_md": issues_md,
            })
        }
        "housekeeping" => {
            let open_issues = cmd_stdout(
                "gh",
                &[
                    "issue",
                    "list",
                    "--state",
                    "open",
                    "--json",
                    "number,title,labels,updatedAt,assignees",
                    "--limit",
                    "100",
                ],
            )
            .unwrap_or_else(|| "[]".to_string());
            let open_prs = open_prs_json();
            let local_branches =
                cmd_stdout("git", &["branch", "--format=%(refname:short)"]).unwrap_or_default();
            let trackers = crate::agent::tracker::find_tracker();
            let mut tracker_bodies = String::new();
            for t in &trackers {
                let body = crate::agent::tracker::get_tracker_body(t.number);
                tracker_bodies.push_str(&format!(
                    "### Tracker #{} — {}\n{}\n\n",
                    t.number, t.title, body
                ));
            }
            let status = read_project_file(&cfg.root, "STATUS.md");
            let issues_md = read_project_file(&cfg.root, "ISSUES.md");
            serde_json::json!({
                "open_issues": open_issues,
                "open_prs": open_prs,
                "local_branches": local_branches,
                "tracker_bodies": tracker_bodies,
                "status": status,
                "issues_md": issues_md,
            })
        }
        _ => serde_json::json!({}),
    }
}

/// Fetch extra context variables declared in `extra_context` and inject them
/// into the vars map.
pub fn fetch_extra_context(wf: &WorkflowConfig, vars: &mut serde_json::Value) {
    for fetch in &wf.extra_context {
        let val = fetch_issue_by_label(&fetch.label);
        vars[&fetch.name] = serde_json::Value::String(val);
    }
}

/// Fetch the body of the most recent open GitHub issue with the given label.
/// Returns `"# <title>\n\n<body>"` or empty string if none found.
fn fetch_issue_by_label(label: &str) -> String {
    cmd_stdout(
        "gh",
        &[
            "issue",
            "list",
            "--label",
            label,
            "--state",
            "open",
            "--limit",
            "1",
            "--json",
            "number,title,body",
            "--jq",
            ".[0] // empty | \"# \\(.title)\\n\\n\\(.body)\"",
        ],
    )
    .unwrap_or_default()
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn gh_open_issues(limit: u32) -> String {
    cmd_stdout(
        "gh",
        &[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,labels",
            "--limit",
            &limit.to_string(),
        ],
    )
    .unwrap_or_else(|| "[]".to_string())
}

fn open_prs_json() -> String {
    let prs = list_open_prs();
    serde_json::to_string_pretty(&prs).unwrap_or_else(|_| "[]".to_string())
}

fn read_project_file(root: &str, name: &str) -> String {
    std::fs::read_to_string(format!("{root}/{name}")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, content).expect("write file");
    }

    fn temp_root() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn list_presets_includes_all_built_in_presets() {
        let root = env!("CARGO_MANIFEST_DIR");
        assert_eq!(
            list_presets(root),
            vec![
                "default".to_string(),
                "business-development".to_string(),
                "data-science".to_string(),
                "deep-research".to_string(),
                "pm".to_string(),
                "quality-assurance".to_string(),
                "software-factory".to_string(),
                "ux".to_string(),
                "xp".to_string(),
            ]
        );
    }

    #[test]
    fn business_development_preset_loads_sidebar_entries() {
        let root = env!("CARGO_MANIFEST_DIR");
        let entries = load_sidebar_entries(root, "business-development");
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|entry| entry.id == "market_research"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.id == "partnership_outreach")
        );
        assert!(entries.iter().any(|entry| entry.id == "sales_prospecting"));
    }

    #[test]
    fn sidebar_entries_group_by_category_then_order() {
        let root = env!("CARGO_MANIFEST_DIR");
        let entries = load_sidebar_entries(root, "default");
        let labels: Vec<(&str, &str)> = entries
            .iter()
            .map(|entry| (entry.category.as_str(), entry.name.as_str()))
            .collect();

        assert_eq!(
            labels,
            vec![
                ("discovery", "Ideation"),
                ("discovery", "UXR Synth"),
                ("discovery", "Interview"),
                ("planning", "Strategic Review"),
                ("planning", "Roadmapper"),
                ("planning", "Sprint Planning"),
                ("review", "Code Review"),
                ("review", "Security Review"),
                ("review", "Security Code Review"),
                ("review", "Retrospective"),
                ("maintenance", "Housekeeping"),
                ("maintenance", "Refresh Agents"),
                ("maintenance", "Refresh Docs"),
                ("maintenance", "Auto Merge"),
            ]
        );
    }

    #[test]
    fn xp_preset_loads_sidebar_entries() {
        let root = env!("CARGO_MANIFEST_DIR");
        let entries = load_sidebar_entries(root, "xp");
        assert_eq!(entries.len(), 10);
        assert!(entries.iter().any(|entry| entry.id == "sprint_planning"));
        assert!(entries.iter().any(|entry| entry.id == "sprint_poker"));
        assert!(entries.iter().any(|entry| entry.id == "pre_ipm"));
        assert!(entries.iter().any(|entry| entry.id == "ipm"));
        assert!(entries.iter().any(|entry| entry.id == "report_research"));
        assert!(!entries.iter().any(|entry| entry.id == "roadmapper"));
        assert!(!entries.iter().any(|entry| entry.id == "housekeeping"));
    }

    #[test]
    fn quality_assurance_preset_loads_sidebar_entries() {
        let root = env!("CARGO_MANIFEST_DIR");
        let entries = load_sidebar_entries(root, "quality-assurance");
        assert_eq!(entries.len(), 3);
        assert!(
            entries
                .iter()
                .any(|entry| entry.id == "test_plan_generation")
        );
        assert!(entries.iter().any(|entry| entry.id == "bug_report_triage"));
        assert!(entries.iter().any(|entry| entry.id == "regression_testing"));
    }

    #[test]
    fn software_factory_preset_loads_sidebar_entries() {
        let root = env!("CARGO_MANIFEST_DIR");
        let entries = load_sidebar_entries(root, "software-factory");
        assert_eq!(entries.len(), 5);
        assert!(entries.iter().any(|entry| entry.id == "factory_charter"));
        assert!(entries.iter().any(|entry| entry.id == "backlog_curation"));
        assert!(entries.iter().any(|entry| entry.id == "autonomous_sprint"));
        assert!(entries.iter().any(|entry| entry.id == "ci_governance"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.id == "factory_retrospective")
        );
    }

    #[test]
    fn xp_sprint_planning_prompt_mentions_xp_practices() {
        let root = env!("CARGO_MANIFEST_DIR");
        let template = load_template(root, "xp", "sprint-planning", "draft.md");
        assert!(template.contains("XP iteration"));
        assert!(template.contains("failing-then-passing test"));
        assert!(template.contains("pairing"));
    }

    #[test]
    fn preset_skill_dirs_includes_all_locations() {
        let root = temp_root();
        let root_str = root.path().to_str().unwrap();
        let dirs = preset_skill_dirs(root_str, "custom");

        let materialized = materialized_workflows_dir().join("custom").join("skills");
        let bundled = bundled_workflows_dir(root_str)
            .join("custom")
            .join("skills");
        let local = local_workflows_dir(root_str).join("custom").join("skills");

        assert_eq!(dirs, vec![materialized, bundled, local]);
    }

    #[test]
    fn list_presets_includes_project_local_presets() {
        let root = temp_root();
        write_file(
            &root
                .path()
                .join(".agents/workflows/custom/story/workflow.yaml"),
            r#"
name: Story
id: story
pattern: one_shot
context: none
"#,
        );

        let presets = list_presets(root.path().to_str().unwrap());
        assert!(
            presets.contains(&"custom".to_string()),
            "project-local preset 'custom' should be included: {presets:?}"
        );
    }

    #[test]
    fn parse_preset_ref_bare_name() {
        let (name, req) = parse_preset_ref("default").unwrap();
        assert_eq!(name, "default");
        assert!(req.is_none());
    }

    #[test]
    fn parse_preset_ref_with_exact_version() {
        let (name, req) = parse_preset_ref("default@0.1.0").unwrap();
        assert_eq!(name, "default");
        assert_eq!(req.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn parse_preset_ref_with_range_requirement() {
        let (name, req) = parse_preset_ref("xp@>=1.0.0").unwrap();
        assert_eq!(name, "xp");
        assert_eq!(req.as_deref(), Some(">=1.0.0"));
    }

    #[test]
    fn parse_preset_ref_rejects_path_traversal() {
        assert!(parse_preset_ref("../../etc/passwd").is_err());
        assert!(parse_preset_ref("default/evil").is_err());
        assert!(parse_preset_ref("preset.yaml").is_err());
    }

    #[test]
    fn resolve_preset_without_requirement_returns_manifest_version() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (name, ver) = resolve_preset(root, "default").expect("should resolve");
        assert_eq!(name, "default");
        // Version must be a valid semver string.
        semver::Version::parse(&ver).expect("resolved version should be valid semver");
    }

    #[test]
    fn resolve_preset_matching_version_requirement_succeeds() {
        let root = env!("CARGO_MANIFEST_DIR");
        // "default@>=0.0.1" should match any real version >= 0.0.1.
        resolve_preset(root, "default@>=0.0.1").expect(">=0.0.1 should match 0.1.0");
    }

    #[test]
    fn resolve_preset_mismatching_version_emits_error() {
        let root = env!("CARGO_MANIFEST_DIR");
        let err = resolve_preset(root, "default@>=99.0.0").expect_err("99.0.0 should not match");
        assert!(
            err.contains("version mismatch"),
            "error should mention version mismatch: {err}"
        );
        assert!(
            err.contains("99.0.0"),
            "error should mention required version: {err}"
        );
    }

    #[test]
    fn load_preset_manifest_missing_yaml_defaults_with_warning() {
        let root = temp_root();
        let manifest = load_preset_manifest(root.path().to_str().unwrap(), "nonexistent-preset");
        assert_eq!(manifest.version, DEFAULT_PRESET_VERSION);
    }

    #[test]
    fn load_preset_manifest_reads_bundled_yaml() {
        let root = env!("CARGO_MANIFEST_DIR");
        let manifest = load_preset_manifest(root, "default");
        assert_eq!(manifest.version, "0.1.0");
    }

    #[test]
    fn local_workflow_overrides_bundled_config_and_template() {
        let root = temp_root();
        write_file(
            &root
                .path()
                .join("assets/workflows/default/example/workflow.yaml"),
            r#"
name: Bundled Name
id: example
pattern: two_phase
context: none
ui:
  category: discovery
  order: 10
phases:
  draft:
    template: draft.md
"#,
        );
        write_file(
            &root
                .path()
                .join("assets/workflows/default/example/draft.md"),
            "bundled template",
        );
        write_file(
            &root
                .path()
                .join(".agents/workflows/default/example/workflow.yaml"),
            r#"
name: Local Name
id: example
pattern: two_phase
context: none
ui:
  category: planning
  order: 20
phases:
  draft:
    template: draft.md
"#,
        );
        write_file(
            &root
                .path()
                .join(".agents/workflows/default/example/draft.md"),
            "local template",
        );

        let workflows = load_workflows(root.path().to_str().unwrap(), "default");
        let wf = workflows.get("example").expect("example workflow");
        assert_eq!(wf.name, "Local Name");
        assert_eq!(wf.ui.category, "planning");
        assert_eq!(
            load_template(
                root.path().to_str().unwrap(),
                "default",
                "example",
                "draft.md"
            ),
            "local template"
        );
    }
}
