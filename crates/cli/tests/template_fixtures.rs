//! Handlebars template fixture harness.
//!
//! For every workflow phase template under `assets/workflows/<preset>/<workflow>/*.md`,
//! this test looks for a sibling `<basename>.fixtures.yaml` and:
//!
//!   1. Renders the template with the fixture's `vars` (plus fragments declared
//!      in the workflow's `workflow.yaml`) and asserts that each substring in
//!      `expect_contains` appears in the rendered output.
//!   2. Removes the variable named in `required_var` and renders with
//!      `render_prompt_strict`; asserts the render fails.
//!
//! ### Fixture file format
//!
//! ```yaml
//! vars:
//!   project_name: "TestProject"
//!   open_issues: "[]"
//!   # ... other variables the template references
//! expect_contains:
//!   - "TestProject"
//!   - "ideation partner"
//! required_var: project_name
//! ```
//!
//! ### Coverage policy
//!
//! Every `*.md` template under `assets/workflows/default/` MUST have a sibling
//! `*.fixtures.yaml`. Templates under other presets get a printed warning when
//! a fixture is missing so adding a new template surfaces in CI output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use caretta::agent::workflow::{WorkflowConfig, render_prompt, render_prompt_strict};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TemplateFixture {
    /// Variables passed to the renderer for the well-formed case.
    #[serde(default)]
    vars: serde_yaml::Mapping,
    /// Substrings that MUST appear in the rendered output.
    #[serde(default)]
    expect_contains: Vec<String>,
    /// Variable name to omit when running the strict-mode missing-required test.
    /// Must reference a top-level `{{var}}` in the template (not `{{#if var}}`,
    /// which is tolerant of absence even in strict mode).
    required_var: String,
}

fn assets_workflows_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflows")
}

fn yaml_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    match value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let key = k.as_str().map(str::to_string).unwrap_or_else(|| {
                    serde_yaml::to_string(k)
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                });
                out.insert(key, yaml_to_json(v));
            }
            serde_json::Value::Object(out)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}

/// Format a fixture failure message showing the full rendered output so the
/// offending needle can be located. Used only for `expect_contains` misses.
fn render_missing_substring_excerpt(template_path: &Path, missing: &str, output: &str) -> String {
    let preview = if output.len() > 4000 {
        let truncated: String = output.chars().take(4000).collect();
        format!(
            "{truncated}\n... [truncated, {} bytes total]",
            output.len()
        )
    } else {
        output.to_string()
    };
    format!(
        "fixture mismatch: {}\n  expected substring NOT found: {missing:?}\n--- rendered output ---\n{preview}\n--- end ---",
        template_path.display(),
    )
}

fn load_workflow_fragments(workflow_yaml: &Path) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(workflow_yaml) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    match serde_yaml::from_str::<WorkflowConfig>(&content) {
        Ok(wf) => wf.fragments,
        Err(e) => {
            eprintln!("WARNING: failed to parse {}: {e}", workflow_yaml.display());
            HashMap::new()
        }
    }
}

struct TemplateInfo {
    /// Path to the .md template file.
    template_path: PathBuf,
    /// Path to the sibling fixture file (may or may not exist).
    fixture_path: PathBuf,
    /// Path to the workflow.yaml in the same directory.
    workflow_yaml: PathBuf,
    /// Human-readable identifier like `default/ideation/draft.md`.
    relative_id: String,
}

fn discover_templates(preset_filter: Option<&str>) -> Vec<TemplateInfo> {
    let root = assets_workflows_dir();
    let mut out = Vec::new();
    let preset_iter = match std::fs::read_dir(&root) {
        Ok(it) => it,
        Err(_) => return out,
    };
    for preset_entry in preset_iter.flatten() {
        let preset_path = preset_entry.path();
        if !preset_path.is_dir() {
            continue;
        }
        let preset_name = match preset_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if let Some(filter) = preset_filter
            && preset_name != filter
        {
            continue;
        }

        let wf_iter = match std::fs::read_dir(&preset_path) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for wf_entry in wf_iter.flatten() {
            let wf_path = wf_entry.path();
            if !wf_path.is_dir() {
                continue;
            }
            let wf_yaml = wf_path.join("workflow.yaml");
            if !wf_yaml.exists() {
                continue;
            }
            let wf_name = wf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let template_iter = match std::fs::read_dir(&wf_path) {
                Ok(it) => it,
                Err(_) => continue,
            };
            for tmpl_entry in template_iter.flatten() {
                let path = tmpl_entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let basename = match path.file_stem().and_then(|n| n.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let fixture_path = wf_path.join(format!("{basename}.fixtures.yaml"));
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                out.push(TemplateInfo {
                    template_path: path.clone(),
                    fixture_path,
                    workflow_yaml: wf_yaml.clone(),
                    relative_id: format!("{preset_name}/{wf_name}/{file_name}"),
                });
            }
        }
    }
    out.sort_by(|a, b| a.relative_id.cmp(&b.relative_id));
    out
}

fn load_fixture(path: &Path) -> Result<TemplateFixture, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read fixture {}: {e}", path.display()))?;
    serde_yaml::from_str::<TemplateFixture>(&content)
        .map_err(|e| format!("cannot parse fixture {}: {e}", path.display()))
}

fn run_fixture_for_template(info: &TemplateInfo) -> Result<(), String> {
    let fixture = load_fixture(&info.fixture_path)?;
    let fragments = load_workflow_fragments(&info.workflow_yaml);
    let template = std::fs::read_to_string(&info.template_path)
        .map_err(|e| format!("cannot read template {}: {e}", info.template_path.display()))?;

    let vars_yaml = serde_yaml::Value::Mapping(fixture.vars.clone());
    let vars_json = yaml_to_json(&vars_yaml);

    // Case 1: well-formed render must succeed and contain each expected substring.
    let rendered = render_prompt(&template, &vars_json, &fragments).map_err(|e| {
        format!(
            "well-formed render failed for {}: {e}",
            info.template_path.display()
        )
    })?;

    for needle in &fixture.expect_contains {
        if !rendered.contains(needle) {
            return Err(render_missing_substring_excerpt(
                &info.template_path,
                needle,
                &rendered,
            ));
        }
    }

    // Case 2: omit `required_var` and confirm strict-mode render returns Err.
    if !fixture
        .vars
        .contains_key(serde_yaml::Value::String(fixture.required_var.clone()))
    {
        return Err(format!(
            "{}: required_var {:?} is not present in vars, so the missing-required test cannot run",
            info.fixture_path.display(),
            fixture.required_var,
        ));
    }
    let mut reduced = fixture.vars.clone();
    reduced.remove(serde_yaml::Value::String(fixture.required_var.clone()));
    let reduced_json = yaml_to_json(&serde_yaml::Value::Mapping(reduced));
    match render_prompt_strict(&template, &reduced_json, &fragments) {
        Ok(out) => Err(format!(
            "{}: strict render unexpectedly succeeded after removing {:?}\n--- output ---\n{}\n--- end ---",
            info.template_path.display(),
            fixture.required_var,
            if out.len() > 1000 { &out[..1000] } else { &out },
        )),
        Err(_) => Ok(()),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

/// Every `default` preset `.md` template must have a sibling fixture file.
/// This is the hard CI gate: adding a new template under `default/` without a
/// fixture fails the build.
#[test]
fn every_default_template_has_a_fixture() {
    let templates = discover_templates(Some("default"));
    assert!(
        !templates.is_empty(),
        "no templates discovered under assets/workflows/default — harness is wired wrong"
    );
    let missing: Vec<String> = templates
        .iter()
        .filter(|t| !t.fixture_path.exists())
        .map(|t| t.relative_id.clone())
        .collect();
    assert!(
        missing.is_empty(),
        "the following default-preset templates are missing fixtures:\n  - {}\n\nAdd a sibling `<basename>.fixtures.yaml` for each.",
        missing.join("\n  - "),
    );
}

/// All `default` preset fixtures must pass both the well-formed render and the
/// missing-required strict-mode check.
#[test]
fn default_preset_template_fixtures_pass() {
    let templates = discover_templates(Some("default"));
    let mut failures: Vec<String> = Vec::new();
    for info in &templates {
        if !info.fixture_path.exists() {
            continue; // covered by every_default_template_has_a_fixture
        }
        if let Err(e) = run_fixture_for_template(info) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} default-preset fixture(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}

/// Cross-preset coverage: any fixture file that exists must pass. Templates
/// without fixtures in non-default presets are reported via stderr so CI surfaces
/// them, but they do not fail the build.
#[test]
fn all_existing_fixtures_pass_and_missing_ones_are_reported() {
    let templates = discover_templates(None);
    let mut failures: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for info in &templates {
        if info.fixture_path.exists() {
            if let Err(e) = run_fixture_for_template(info) {
                failures.push(e);
            }
        } else if !info.relative_id.starts_with("default/") {
            missing.push(info.relative_id.clone());
        }
    }

    if !missing.is_empty() {
        eprintln!(
            "WARNING: {} non-default template(s) lack a fixture (add `<basename>.fixtures.yaml` next to each):\n  - {}",
            missing.len(),
            missing.join("\n  - "),
        );
    }

    assert!(
        failures.is_empty(),
        "{} fixture(s) failed across all presets:\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}
