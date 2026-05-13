use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::assets::assets_dir;
use crate::agent::cmd::die;
use crate::agent::shell::{cmd_stdout, log};
use crate::agent::tracker::list_open_prs;
use crate::agent::types::Config;

// ── YAML config types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
}

/// Fetch the body of a GitHub issue by its label and inject it as a template variable.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtraContextFetch {
    /// Template variable name to inject (e.g. "report_synthesis").
    pub name: String,
    /// GitHub issue label to search for (e.g. "uxr-synthesis").
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPattern {
    TwoPhase,
    OneShot,
    MultiRound,
    Implementation,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
///
/// Each `workflow.yaml` is validated against [`WorkflowConfig`]; misconfigured
/// files abort startup with a field-named error rather than silently falling
/// back to defaults.
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
            match parse_workflow_yaml(&content) {
                Ok(wf) => {
                    map.insert(wf.id.clone(), wf);
                }
                Err(e) => {
                    die(&format!(
                        "Invalid workflow config {}: {e}",
                        yaml_path.display()
                    ));
                }
            }
        }
    }
    map
}

/// Parse and validate a single `workflow.yaml` string against
/// [`WorkflowConfig`]'s schema.
///
/// Unknown fields are rejected (no silent defaults), and the returned error is
/// rewritten to highlight the offending field name and, when possible, a
/// suggested correction (`did you mean \`visible\`?`).
pub fn parse_workflow_yaml(content: &str) -> Result<WorkflowConfig, String> {
    serde_yaml::from_str::<WorkflowConfig>(content).map_err(|e| format_yaml_error(&e))
}

/// Return the JSON Schema derived from [`WorkflowConfig`].
///
/// Generated via `schemars` so that documentation, tooling, and external
/// validators can consume the same source of truth that load-time validation
/// uses.
pub fn workflow_json_schema() -> schemars::Schema {
    schemars::schema_for!(WorkflowConfig)
}

/// Rewrite a `serde_yaml` error into a startup-friendly message.
///
/// `serde_yaml` already names the offending field; we append a "did you mean"
/// suggestion when the unknown field is a near-miss for a known one. Missing
/// required fields and parse errors pass through with their location.
fn format_yaml_error(err: &serde_yaml::Error) -> String {
    let msg = err.to_string();
    let location = err
        .location()
        .map(|loc| format!(" (line {}, column {})", loc.line(), loc.column()))
        .unwrap_or_default();

    if let Some((path, bad, known)) = parse_unknown_field(&msg) {
        let suggestion = closest_match(&bad, &known)
            .map(|s| format!(" — did you mean `{s}`?"))
            .unwrap_or_default();
        let in_path = if path.is_empty() {
            String::new()
        } else {
            format!(" in `{path}`")
        };
        return format!(
            "unknown field `{bad}`{in_path}{location}{suggestion}. Allowed fields: {}",
            known.join(", ")
        );
    }

    if let Some((path, field)) = parse_missing_field(&msg) {
        let in_path = if path.is_empty() {
            String::new()
        } else {
            format!(" in `{path}`")
        };
        return format!("missing required field `{field}`{in_path}{location}");
    }

    // Fall back to the raw serde_yaml message, which still names fields when
    // the error is a type mismatch on a specific value.
    format!("{msg}{location}")
}

/// Pull the offending field name, dotted path, and list of allowed fields out
/// of a `serde_yaml` "unknown field" error. Nested errors are prefixed by the
/// parent path (e.g. `ui: unknown field …`); we capture that prefix as `path`
/// so the rewritten message can name the location.
///
/// Matches serde_yaml 0.9.x error format:
///   "unknown field `<name>`, expected one of `f1`, `f2`, …"
///   or with nesting: "ui: unknown field `<name>`, …"
fn parse_unknown_field(msg: &str) -> Option<(String, String, Vec<String>)> {
    let needle = "unknown field `";
    let idx = msg.find(needle)?;
    let path = msg[..idx].trim_end_matches(": ").to_string();
    let after = &msg[idx + needle.len()..];
    let (bad, rest) = after.split_once('`')?;
    let known = if let Some(list) = rest.strip_prefix(", expected one of ") {
        list.split(',')
            .filter_map(|tok| {
                let tok = tok.trim();
                tok.strip_prefix('`').and_then(|t| t.strip_suffix('`'))
            })
            .map(|s| s.to_string())
            .collect()
    } else if let Some(single) = rest.strip_prefix(", expected `") {
        // `expected `field`` — singular case used by some serde versions.
        single
            .strip_suffix('`')
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Some((path, bad.to_string(), known))
}

fn parse_missing_field(msg: &str) -> Option<(String, String)> {
    let needle = "missing field `";
    let idx = msg.find(needle)?;
    let path = msg[..idx].trim_end_matches(": ").to_string();
    let after = &msg[idx + needle.len()..];
    let (field, _) = after.split_once('`')?;
    Some((path, field.to_string()))
}

/// Return the closest match by Levenshtein distance, if any is within the
/// half-the-shorter-name threshold (so wildly different typos don't produce
/// misleading suggestions).
fn closest_match(needle: &str, haystack: &[String]) -> Option<String> {
    let mut best: Option<(usize, &String)> = None;
    for candidate in haystack {
        let d = levenshtein(needle, candidate);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, candidate));
        }
    }
    let (dist, candidate) = best?;
    let threshold = needle.len().min(candidate.len()).max(2) / 2;
    if dist <= threshold.max(1) {
        Some(candidate.clone())
    } else {
        None
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
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

    #[test]
    fn parse_workflow_yaml_accepts_valid_config() {
        let yaml = r#"
name: Example
id: example
description: An example workflow
pattern: two_phase
context: none
ui:
  category: planning
  order: 20
  visible: true
phases:
  draft:
    template: draft.md
"#;
        let wf = parse_workflow_yaml(yaml).expect("valid workflow should parse");
        assert_eq!(wf.id, "example");
        assert_eq!(wf.pattern, ExecutionPattern::TwoPhase);
        assert_eq!(wf.ui.order, 20);
        assert!(wf.phases.contains_key("draft"));
    }

    #[test]
    fn parse_workflow_yaml_rejects_unknown_top_level_field_with_suggestion() {
        // `pattren` is a near-miss for `pattern` — the error should name the
        // offending field and suggest the closest known one.
        let yaml = r#"
name: Example
id: example
pattren: two_phase
"#;
        let err = parse_workflow_yaml(yaml).expect_err("unknown field should fail");
        assert!(
            err.contains("unknown field `pattren`"),
            "expected unknown-field error to name the field, got: {err}"
        );
        assert!(
            err.contains("did you mean `pattern`?"),
            "expected did-you-mean suggestion, got: {err}"
        );
    }

    #[test]
    fn parse_workflow_yaml_rejects_unknown_nested_field() {
        // `visibel` inside `ui` is also a near-miss for `visible`.
        let yaml = r#"
name: Example
id: example
pattern: one_shot
ui:
  visibel: true
"#;
        let err = parse_workflow_yaml(yaml).expect_err("unknown nested field should fail");
        assert!(
            err.contains("unknown field `visibel`"),
            "expected unknown-field error to name the field, got: {err}"
        );
        assert!(
            err.contains("did you mean `visible`?"),
            "expected did-you-mean suggestion, got: {err}"
        );
    }

    #[test]
    fn parse_workflow_yaml_reports_missing_required_field() {
        // `name` is required and has no default.
        let yaml = r#"
id: example
pattern: one_shot
"#;
        let err = parse_workflow_yaml(yaml).expect_err("missing required field should fail");
        assert!(
            err.contains("missing required field `name`"),
            "expected missing-field error to name the field, got: {err}"
        );
    }

    #[test]
    fn workflow_json_schema_is_derivable_and_lists_known_fields() {
        // The schema is derived from `WorkflowConfig` via `schemars` and acts
        // as a single source of truth for tooling/docs. We don't assert the
        // full shape here, only that it exposes the top-level field names so
        // downstream consumers can rely on it.
        let schema = workflow_json_schema();
        let json = serde_json::to_value(&schema).expect("schema serializes");
        let props = json
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("schema has properties");
        for field in [
            "name",
            "id",
            "pattern",
            "context",
            "ui",
            "depends_on",
            "phases",
        ] {
            assert!(
                props.contains_key(field),
                "schema should expose `{field}` property: {props:?}"
            );
        }
    }

    #[test]
    fn closest_match_finds_near_typos() {
        let known: Vec<String> = ["pattern", "context", "phases"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(closest_match("pattren", &known), Some("pattern".into()));
        assert_eq!(closest_match("contxt", &known), Some("context".into()));
        // Wildly different inputs should not produce a misleading suggestion.
        assert_eq!(closest_match("xyz", &known), None);
    }
}
