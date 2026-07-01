// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::types::{
    DOT_CARETTA_PERSONAS_DIR, DOT_CARETTA_USER_PERSONAS_PERSONAS_DIR,
    LEGACY_ASSETS_USER_PERSONAS_PERSONAS_DIR,
};
use dioxus::prelude::*;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const FACT_PREFIXES: [&str; 6] = [
    "jobs_to_be_done",
    "pains",
    "adoption_yes_if",
    "rejection_no_if",
    "anti_goals",
    "recognition_cues",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonaForm {
    pub original_file_name: Option<String>,
    pub file_name: String,
    pub name: String,
    pub title: String,
    pub organization: String,
    pub summary: String,
    pub communication_style: String,
    pub jobs_to_be_done: String,
    pub pains: String,
    pub adoption_yes_if: String,
    pub rejection_no_if: String,
    pub anti_goals: String,
    pub recognition_cues: String,
}

impl PersonaForm {
    fn has_content(&self) -> bool {
        [
            &self.file_name,
            &self.name,
            &self.title,
            &self.organization,
            &self.summary,
            &self.communication_style,
            &self.jobs_to_be_done,
            &self.pains,
            &self.adoption_yes_if,
            &self.rejection_no_if,
            &self.anti_goals,
            &self.recognition_cues,
        ]
        .iter()
        .any(|field| !field.trim().is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonaSummary {
    pub file_name: String,
    pub path_display: String,
    pub name: String,
    pub title: String,
    pub organization: String,
    pub summary: String,
    pub jobs_to_be_done: Vec<String>,
    pub pains: Vec<String>,
    pub recognition_cues: Vec<String>,
}

fn resolve_skill_path(root: &str, skill_path: &str) -> PathBuf {
    let path = Path::new(skill_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(root).join(path)
    }
}

/// Project-relative directory where the Persona Studio writes new personas.
///
/// All user-created persona JSON now lives at `<root>/.caretta/personas/`
/// so it stays inside the project's working tree (mirroring Discovery's
/// `.caretta/discovery/` layout) instead of leaking into the OS app-data
/// directory via the materialized skill fallback. The `skill_path`
/// argument is accepted purely so call sites and the public API remain
/// stable; the value is only consulted to read legacy locations.
pub fn personas_dir(root: &str, _skill_path: &str) -> PathBuf {
    Path::new(root).join(DOT_CARETTA_PERSONAS_DIR)
}

/// Directories that may contain previously-saved personas, in priority order:
/// 1. The canonical `<root>/.caretta/personas/` location used for new writes.
/// 2. The legacy `.caretta/skills/user-personas/personas/` directory used by
///    older builds that nested personas under the skill tree.
/// 3. The upstream `assets/skills/user-personas/personas/` directory.
/// 4. The `personas/` directory adjacent to the resolved `SKILL.md`, which
///    may point at the materialized app-data copy when no repo-local skill
///    exists.
fn persona_read_dirs(root: &str, skill_path: &str) -> Vec<PathBuf> {
    let root_path = Path::new(root);
    let mut dirs: Vec<PathBuf> = vec![
        root_path.join(DOT_CARETTA_PERSONAS_DIR),
        root_path.join(DOT_CARETTA_USER_PERSONAS_PERSONAS_DIR),
        root_path.join(LEGACY_ASSETS_USER_PERSONAS_PERSONAS_DIR),
    ];
    if let Some(parent) = resolve_skill_path(root, skill_path).parent() {
        let adjacent = parent.join("personas");
        if !dirs.iter().any(|existing| existing == &adjacent) {
            dirs.push(adjacent);
        }
    }
    dirs
}

#[cfg(target_arch = "wasm32")]
pub fn load_personas(_root: &str, _skill_path: &str) -> Result<Vec<PersonaSummary>, String> {
    Err("Persona Studio persistence is available in the desktop app.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_personas(root: &str, skill_path: &str) -> Result<Vec<PersonaSummary>, String> {
    use std::collections::HashSet;

    let mut personas = Vec::new();
    let mut seen_file_names: HashSet<String> = HashSet::new();
    for dir in persona_read_dirs(root, skill_path) {
        if !dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match load_persona_summary_from_path(&path) {
                Ok(persona) => {
                    // Personas in higher-priority dirs (canonical `.caretta/personas/`)
                    // shadow same-named files from legacy fallback locations.
                    if seen_file_names.insert(persona.file_name.clone()) {
                        personas.push(persona);
                    }
                }
                Err(err) => tracing::warn!("Skipping persona {}: {err}", path.display()),
            }
        }
    }

    personas.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    Ok(personas)
}

fn load_persona_summary_from_path(path: &Path) -> Result<PersonaSummary, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read persona: {e}"))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid persona JSON: {e}"))?;
    let persona = value
        .get("persona")
        .and_then(Value::as_object)
        .ok_or_else(|| "Missing `persona` object".to_string())?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("persona.json")
        .to_string();
    let facts = other_facts(persona);
    let occupation = persona.get("occupation").and_then(Value::as_object);
    let summary = occupation
        .and_then(|obj| get_string(obj, "description"))
        .or_else(|| get_string(persona, "description"))
        .unwrap_or_default();

    Ok(PersonaSummary {
        file_name,
        path_display: path.to_string_lossy().to_string(),
        name: get_string(persona, "name").unwrap_or_else(|| "Untitled persona".to_string()),
        title: occupation
            .and_then(|obj| get_string(obj, "title"))
            .unwrap_or_default(),
        organization: occupation
            .and_then(|obj| get_string(obj, "organization"))
            .unwrap_or_default(),
        summary,
        jobs_to_be_done: collect_prefixed(&facts, "jobs_to_be_done"),
        pains: collect_prefixed(&facts, "pains"),
        recognition_cues: collect_prefixed(&facts, "recognition_cues"),
    })
}

#[cfg(target_arch = "wasm32")]
pub fn load_persona_form(
    _root: &str,
    _skill_path: &str,
    _file_name: &str,
) -> Result<PersonaForm, String> {
    Err("Persona Studio persistence is available in the desktop app.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_persona_form(
    root: &str,
    skill_path: &str,
    file_name: &str,
) -> Result<PersonaForm, String> {
    let safe_file_name = safe_json_file_name(file_name, "persona");
    // Look in the canonical `.caretta/personas/` dir first, then fall back to
    // legacy locations so personas saved by older builds remain editable.
    let path = persona_read_dirs(root, skill_path)
        .into_iter()
        .map(|dir| dir.join(&safe_file_name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| personas_dir(root, skill_path).join(&safe_file_name));
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read persona: {e}"))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid persona JSON: {e}"))?;
    let persona = value
        .get("persona")
        .and_then(Value::as_object)
        .ok_or_else(|| "Missing `persona` object".to_string())?;
    let facts = other_facts(persona);
    let occupation = persona.get("occupation").and_then(Value::as_object);

    Ok(PersonaForm {
        original_file_name: Some(safe_file_name.clone()),
        file_name: safe_file_name,
        name: get_string(persona, "name").unwrap_or_default(),
        title: occupation
            .and_then(|obj| get_string(obj, "title"))
            .unwrap_or_default(),
        organization: occupation
            .and_then(|obj| get_string(obj, "organization"))
            .unwrap_or_default(),
        summary: occupation
            .and_then(|obj| get_string(obj, "description"))
            .or_else(|| get_string(persona, "description"))
            .unwrap_or_default(),
        communication_style: get_string(persona, "communication_style").unwrap_or_default(),
        jobs_to_be_done: collect_prefixed(&facts, "jobs_to_be_done").join("\n"),
        pains: collect_prefixed(&facts, "pains").join("\n"),
        adoption_yes_if: collect_prefixed(&facts, "adoption_yes_if").join("\n"),
        rejection_no_if: collect_prefixed(&facts, "rejection_no_if").join("\n"),
        anti_goals: collect_prefixed(&facts, "anti_goals").join("\n"),
        recognition_cues: collect_prefixed(&facts, "recognition_cues").join("\n"),
    })
}

#[cfg(target_arch = "wasm32")]
pub fn save_persona(_root: &str, _skill_path: &str, _form: &PersonaForm) -> Result<String, String> {
    Err("Persona Studio persistence is available in the desktop app.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_persona(root: &str, skill_path: &str, form: &PersonaForm) -> Result<String, String> {
    let dir = personas_dir(root, skill_path);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    let fallback_stem = if form.name.trim().is_empty() {
        "persona"
    } else {
        &form.name
    };
    let requested_file_name = safe_json_file_name(&form.file_name, fallback_stem);
    let file_name = match &form.original_file_name {
        Some(original) if original != &requested_file_name => {
            let target = dir.join(&requested_file_name);
            if target.exists() {
                return Err(format!("{} already exists", requested_file_name));
            }
            requested_file_name
        }
        Some(_) => requested_file_name,
        None => unique_json_file_name(&dir, &requested_file_name),
    };

    // Resolve the existing persona file by searching the canonical save dir
    // first and then the legacy read dirs. This lets a persona originally
    // saved in a legacy location migrate to `<root>/.caretta/personas/` on
    // the next save without losing unrelated `other_facts` entries.
    let existing_path = form.original_file_name.as_ref().and_then(|name| {
        let original = safe_json_file_name(name, "persona");
        persona_read_dirs(root, skill_path)
            .into_iter()
            .map(|d| d.join(&original))
            .find(|candidate| candidate.is_file())
    });
    let target_path = dir.join(&file_name);

    let mut document = existing_path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .unwrap_or_else(|| json!({ "type": "TinyPerson", "persona": {} }));

    let preserved_facts = document
        .get("persona")
        .and_then(Value::as_object)
        .map(other_facts)
        .unwrap_or_default()
        .into_iter()
        .filter(|fact| !is_known_prefixed_fact(fact))
        .collect::<Vec<_>>();
    let mut facts = preserved_facts;
    append_fact_lines(&mut facts, "jobs_to_be_done", &form.jobs_to_be_done);
    append_fact_lines(&mut facts, "pains", &form.pains);
    append_fact_lines(&mut facts, "adoption_yes_if", &form.adoption_yes_if);
    append_fact_lines(&mut facts, "rejection_no_if", &form.rejection_no_if);
    append_fact_lines(&mut facts, "anti_goals", &form.anti_goals);
    append_fact_lines(&mut facts, "recognition_cues", &form.recognition_cues);

    let document_obj = ensure_object(&mut document);
    document_obj
        .entry("type".to_string())
        .or_insert_with(|| Value::String("TinyPerson".to_string()));
    let persona_obj = ensure_child_object(document_obj, "persona");
    set_string(
        persona_obj,
        "name",
        clean_or_default(&form.name, "Untitled persona"),
    );
    set_optional_string(
        persona_obj,
        "communication_style",
        form.communication_style.trim(),
    );

    let occupation_obj = ensure_child_object(persona_obj, "occupation");
    set_optional_string(occupation_obj, "title", form.title.trim());
    set_optional_string(occupation_obj, "organization", form.organization.trim());
    set_optional_string(occupation_obj, "description", form.summary.trim());

    persona_obj.insert(
        "other_facts".to_string(),
        Value::Array(facts.into_iter().map(Value::String).collect()),
    );

    let json =
        serde_json::to_string_pretty(&document).map_err(|e| format!("Serialize failed: {e}"))?;
    std::fs::write(&target_path, json)
        .map_err(|e| format!("Failed to write {}: {e}", target_path.display()))?;

    if let Some(old_path) = existing_path
        && old_path != target_path
        && old_path.exists()
    {
        std::fs::remove_file(&old_path)
            .map_err(|e| format!("Saved new file, but failed to remove old file: {e}"))?;
    }

    Ok(file_name)
}

#[cfg(target_arch = "wasm32")]
pub fn delete_persona(_root: &str, _skill_path: &str, _file_name: &str) -> Result<(), String> {
    Err("Persona Studio persistence is available in the desktop app.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn delete_persona(root: &str, skill_path: &str, file_name: &str) -> Result<(), String> {
    let safe = safe_json_file_name(file_name, "persona");
    // Remove the persona from the canonical save dir as well as any legacy
    // locations so it does not silently reappear from a fallback directory
    // after deletion.
    let mut removed_any = false;
    let mut last_err: Option<String> = None;
    for dir in persona_read_dirs(root, skill_path) {
        let path = dir.join(&safe);
        if !path.is_file() {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => removed_any = true,
            Err(e) => last_err = Some(format!("Failed to delete {}: {e}", path.display())),
        }
    }
    if removed_any {
        Ok(())
    } else if let Some(err) = last_err {
        Err(err)
    } else {
        Err(format!("Persona not found: {safe}"))
    }
}

pub fn generate_persona_from_notes(notes: &str, sequence: usize) -> PersonaForm {
    let clean = collapse_whitespace(notes);
    let focus = if clean.is_empty() {
        "a user with an emerging workflow need".to_string()
    } else {
        trim_to_chars(&clean, 180)
    };
    let inferred_title = infer_title(&clean).unwrap_or_else(|| "Emerging User Segment".to_string());
    let inferred_org = infer_organization(&clean).unwrap_or_default();
    let inferred_name = infer_name(&clean).unwrap_or_else(|| format!("Persona {sequence}"));
    let cue_terms = keyword_cues(&clean);
    let cue_line = if cue_terms.is_empty() {
        format!(
            "Mentions workflows similar to '{}'.",
            trim_to_chars(&focus, 80)
        )
    } else {
        format!("Uses phrases such as {}.", cue_terms.join(", "))
    };

    PersonaForm {
        original_file_name: None,
        file_name: format!("{}.json", slugify(&inferred_name)),
        name: inferred_name,
        title: inferred_title,
        organization: inferred_org,
        summary: focus.clone(),
        communication_style:
            "Prefers concrete, context-rich discussion tied to their current workflow and constraints."
                .to_string(),
        jobs_to_be_done: [
            format!("Make this workflow easier to complete: {focus}"),
            "Evaluate the product quickly against their real constraints.".to_string(),
            "Move from an ad hoc workaround to a repeatable operating pattern.".to_string(),
        ]
        .join("\n"),
        pains: [
            format!("Current process is harder than it should be: {focus}"),
            "Hard to tell whether tooling will fit before investing setup time.".to_string(),
            "Switching costs feel risky without clear proof of value.".to_string(),
        ]
        .join("\n"),
        adoption_yes_if: [
            "The first run maps directly to their current workflow.".to_string(),
            "Docs and examples speak to their constraints without extra translation.".to_string(),
            "They can validate value before changing production habits.".to_string(),
        ]
        .join("\n"),
        rejection_no_if: [
            "Setup requires them to replace working tools before proving value.".to_string(),
            "The product hides tradeoffs they need to inspect.".to_string(),
            "The workflow adds review or operational overhead without a clear payoff.".to_string(),
        ]
        .join("\n"),
        anti_goals: [
            "They do not want a generic demo detached from their context.".to_string(),
            "They do not want a solution that assumes a larger team or heavier process.".to_string(),
        ]
        .join("\n"),
        recognition_cues: cue_line,
    }
}

fn get_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn other_facts(persona: &Map<String, Value>) -> Vec<String> {
    persona
        .get("other_facts")
        .and_then(Value::as_array)
        .map(|facts| {
            facts
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|fact| !fact.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn collect_prefixed(facts: &[String], prefix: &str) -> Vec<String> {
    facts
        .iter()
        .filter_map(|fact| strip_fact_prefix(fact, prefix))
        .collect()
}

fn strip_fact_prefix(fact: &str, prefix: &str) -> Option<String> {
    let trimmed = fact.trim();
    let marker = format!("{prefix}:");
    if trimmed
        .to_ascii_lowercase()
        .starts_with(&marker.to_ascii_lowercase())
    {
        Some(trimmed[marker.len()..].trim().to_string())
    } else {
        None
    }
}

fn is_known_prefixed_fact(fact: &str) -> bool {
    FACT_PREFIXES
        .iter()
        .any(|prefix| strip_fact_prefix(fact, prefix).is_some())
}

fn append_fact_lines(facts: &mut Vec<String>, prefix: &str, text: &str) {
    for line in text.lines() {
        let cleaned = clean_fact_line(line);
        if cleaned.is_empty() {
            continue;
        }
        if strip_fact_prefix(&cleaned, prefix).is_some() {
            facts.push(cleaned);
        } else {
            facts.push(format!("{prefix}: {cleaned}"));
        }
    }
}

fn clean_fact_line(line: &str) -> String {
    line.trim()
        .trim_start_matches('-')
        .trim_start_matches('*')
        .trim()
        .to_string()
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("value is object")
}

fn ensure_child_object<'a>(
    map: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    let entry = map
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("child is object")
}

fn set_string(map: &mut Map<String, Value>, key: &str, value: String) {
    map.insert(key.to_string(), Value::String(value));
}

fn set_optional_string(map: &mut Map<String, Value>, key: &str, value: &str) {
    if value.trim().is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), Value::String(value.trim().to_string()));
    }
}

fn clean_or_default(value: &str, fallback: &str) -> String {
    let clean = value.trim();
    if clean.is_empty() {
        fallback.to_string()
    } else {
        clean.to_string()
    }
}

fn safe_json_file_name(input: &str, fallback_stem: &str) -> String {
    let raw = Path::new(input.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let stem = raw.strip_suffix(".json").unwrap_or(raw).trim().to_string();
    let safe_stem = if stem.is_empty() {
        slugify(fallback_stem)
    } else {
        slugify(&stem)
    };
    format!("{safe_stem}.json")
}

fn unique_json_file_name(dir: &Path, requested: &str) -> String {
    let requested = safe_json_file_name(requested, "persona");
    if !dir.join(&requested).exists() {
        return requested;
    }
    let stem = requested.trim_end_matches(".json");
    for i in 2..1000 {
        let candidate = format!("{stem}-{i}.json");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{stem}-copy.json")
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let slug = out.trim_matches('-');
    if slug.is_empty() {
        "persona".to_string()
    } else {
        slug.to_string()
    }
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_to_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        input.to_string()
    } else {
        format!("{}...", input.chars().take(max).collect::<String>())
    }
}

fn infer_name(input: &str) -> Option<String> {
    extract_after_markers(input, &["named ", "called "]).map(|name| {
        name.split([',', '.', ';'])
            .next()
            .unwrap_or(&name)
            .trim()
            .to_string()
    })
}

fn infer_title(input: &str) -> Option<String> {
    let role = extract_after_markers(
        input,
        &[
            "persona for ",
            "user is a ",
            "user is an ",
            "as a ",
            "as an ",
            "for a ",
            "for an ",
        ],
    )?;
    Some(title_case_fragment(&role))
}

fn infer_organization(input: &str) -> Option<String> {
    extract_after_markers(input, &[" at ", " from "]).map(|org| title_case_fragment(&org))
}

fn extract_after_markers(input: &str, markers: &[&str]) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            let start = idx + marker.len();
            let rest = input.get(start..).unwrap_or("").trim();
            let end = first_boundary(rest);
            let candidate = strip_leading_article(rest[..end].trim());
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn first_boundary(input: &str) -> usize {
    [
        ",", ".", ";", ":", " at ", " from ", " who ", " that ", " needs ", " wants ", " with ",
    ]
    .iter()
    .filter_map(|needle| input.find(needle))
    .min()
    .unwrap_or(input.len())
}

fn strip_leading_article(input: &str) -> &str {
    input
        .strip_prefix("a ")
        .or_else(|| input.strip_prefix("an "))
        .or_else(|| input.strip_prefix("the "))
        .unwrap_or(input)
}

fn title_case_fragment(input: &str) -> String {
    let fragment = input
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::new();
    for word in fragment.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    out
}

fn keyword_cues(input: &str) -> Vec<String> {
    let stop_words = [
        "about", "after", "against", "because", "before", "being", "their", "there", "these",
        "those", "through", "using", "wants", "needs", "without", "workflow", "person", "persona",
        "users", "user",
    ];
    let mut cues = Vec::new();
    for raw in input.split_whitespace() {
        let word = raw
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
            .to_ascii_lowercase();
        if word.len() < 5 || stop_words.contains(&word.as_str()) {
            continue;
        }
        if !cues.iter().any(|cue| cue == &word) {
            cues.push(word);
        }
        if cues.len() == 6 {
            break;
        }
    }
    cues
}

fn update_form(mut form: Signal<PersonaForm>, update: impl FnOnce(&mut PersonaForm)) {
    let mut next = form.read().clone();
    update(&mut next);
    form.set(next);
}

fn bump_reload(mut reload_tick: Signal<u64>) {
    let next = {
        let current = *reload_tick.read();
        current + 1
    };
    reload_tick.set(next);
}

#[component]
pub fn PersonasPanel(root: Signal<String>, skill_path: Signal<String>) -> Element {
    let mut personas = use_signal(Vec::<PersonaSummary>::new);
    let mut selected = use_signal(|| None::<String>);
    let mut form = use_signal(PersonaForm::default);
    let mut notes = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);
    let reload_tick = use_signal(|| 0_u64);
    let mut delete_confirm = use_signal(|| None::<String>);

    use_effect(move || {
        let _ = *reload_tick.read();
        let r = root.read().clone();
        let s = skill_path.read().clone();
        match load_personas(&r, &s) {
            Ok(list) => {
                let current = selected.read().clone();
                let next_selected = current
                    .filter(|file| list.iter().any(|persona| persona.file_name == *file))
                    .or_else(|| list.first().map(|persona| persona.file_name.clone()));
                personas.set(list);
                if selected.read().as_ref() != next_selected.as_ref() {
                    selected.set(next_selected.clone());
                    if let Some(file_name) = next_selected {
                        match load_persona_form(&r, &s, &file_name) {
                            Ok(next_form) => form.set(next_form),
                            Err(err) => status.set(Some(err)),
                        }
                    } else {
                        form.set(PersonaForm::default());
                    }
                }
            }
            Err(err) => status.set(Some(err)),
        }
    });

    let current_form = form.read().clone();
    let current_notes = notes.read().clone();
    let current_status = status.read().clone();
    let selected_file = selected.read().clone();
    let delete_target = delete_confirm.read().clone();
    let persona_count = personas.read().len();
    let personas_dir_display = personas_dir(&root.read(), &skill_path.read())
        .to_string_lossy()
        .to_string();

    rsx! {
        div { class: "persona-studio",
            div { class: "persona-nav",
                div { class: "persona-nav-header",
                    div { class: "persona-title", "Personas" }
                    div { class: "persona-count", "{persona_count}" }
                }
                div { class: "persona-generate",
                    textarea {
                        class: "persona-seed-input",
                        placeholder: "Describe a user segment...",
                        value: "{current_notes}",
                        oninput: move |evt| notes.set(evt.value()),
                    }
                    button {
                        class: "btn btn-sm btn-go",
                        disabled: notes.read().trim().is_empty(),
                        onclick: move |_| {
                            let draft = generate_persona_from_notes(&notes.read(), personas.read().len() + 1);
                            selected.set(None);
                            delete_confirm.set(None);
                            form.set(draft);
                            status.set(Some("Draft generated. Review and save.".to_string()));
                        },
                        "Generate Draft"
                    }
                }
                div { class: "persona-nav-actions",
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| {
                            selected.set(None);
                            delete_confirm.set(None);
                            form.set(PersonaForm {
                                file_name: "new-persona.json".to_string(),
                                ..PersonaForm::default()
                            });
                            status.set(None);
                        },
                        "New"
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| bump_reload(reload_tick),
                        "Refresh"
                    }
                }
                div { class: "persona-list",
                    if personas.read().is_empty() {
                        div { class: "persona-empty", "No personas saved." }
                    }
                    for persona in personas.read().iter() {
                        {
                            let file_name = persona.file_name.clone();
                            let active = selected_file.as_ref() == Some(&persona.file_name);
                            rsx! {
                                div {
                                    key: "{persona.file_name}",
                                    class: if active { "persona-list-item persona-list-item-active" } else { "persona-list-item" },
                                    title: "{persona.path_display}",
                                    onclick: move |_| {
                                        let r = root.read().clone();
                                        let s = skill_path.read().clone();
                                        selected.set(Some(file_name.clone()));
                                        delete_confirm.set(None);
                                        match load_persona_form(&r, &s, &file_name) {
                                            Ok(next_form) => {
                                                form.set(next_form);
                                                status.set(None);
                                            }
                                            Err(err) => status.set(Some(err)),
                                        }
                                    },
                                    div { class: "persona-list-name", "{persona.name}" }
                                    if !persona.title.is_empty() {
                                        div { class: "persona-list-role", "{persona.title}" }
                                    }
                                    if !persona.recognition_cues.is_empty() {
                                        div { class: "persona-list-cue", "{trim_to_chars(&persona.recognition_cues[0], 76)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "persona-editor",
                div { class: "persona-editor-toolbar",
                    div { class: "persona-path", title: "{personas_dir_display}", "{personas_dir_display}" }
                    div { class: "persona-toolbar-actions",
                        button {
                            class: "btn btn-sm btn-go",
                            disabled: !current_form.has_content(),
                            onclick: move |_| {
                                let r = root.read().clone();
                                let s = skill_path.read().clone();
                                let current = form.read().clone();
                                match save_persona(&r, &s, &current) {
                                    Ok(file_name) => {
                                        selected.set(Some(file_name.clone()));
                                        delete_confirm.set(None);
                                        match load_persona_form(&r, &s, &file_name) {
                                            Ok(saved_form) => form.set(saved_form),
                                            Err(err) => status.set(Some(err)),
                                        }
                                        bump_reload(reload_tick);
                                        status.set(Some(format!("Saved {file_name}")));
                                    }
                                    Err(err) => status.set(Some(err)),
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "btn btn-sm btn-danger",
                            disabled: current_form.original_file_name.is_none(),
                            onclick: move |_| {
                                let original_file_name = {
                                    form.read().original_file_name.clone()
                                };
                                if let Some(file_name) = original_file_name {
                                    if delete_confirm.read().as_ref() == Some(&file_name) {
                                        let r = root.read().clone();
                                        let s = skill_path.read().clone();
                                        match delete_persona(&r, &s, &file_name) {
                                            Ok(()) => {
                                                selected.set(None);
                                                form.set(PersonaForm::default());
                                                delete_confirm.set(None);
                                                bump_reload(reload_tick);
                                                status.set(Some(format!("Deleted {file_name}")));
                                            }
                                            Err(err) => status.set(Some(err)),
                                        }
                                    } else {
                                        delete_confirm.set(Some(file_name));
                                        status.set(Some("Click Delete again to confirm.".to_string()));
                                    }
                                }
                            },
                            if delete_target.as_ref() == current_form.original_file_name.as_ref() {
                                "Confirm Delete"
                            } else {
                                "Delete"
                            }
                        }
                    }
                }

                if let Some(message) = current_status {
                    div { class: "persona-status", "{message}" }
                }

                if !current_form.has_content() {
                    div { class: "persona-editor-empty", "Select a persona or generate a draft." }
                } else {
                    div { class: "persona-form",
                        div { class: "persona-form-grid",
                            label { class: "persona-field",
                                span { "File" }
                                input {
                                    class: "text-input",
                                    r#type: "text",
                                    value: "{current_form.file_name}",
                                    oninput: move |evt| update_form(form, move |draft| draft.file_name = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Name" }
                                input {
                                    class: "text-input",
                                    r#type: "text",
                                    value: "{current_form.name}",
                                    oninput: move |evt| update_form(form, move |draft| draft.name = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Role" }
                                input {
                                    class: "text-input",
                                    r#type: "text",
                                    value: "{current_form.title}",
                                    oninput: move |evt| update_form(form, move |draft| draft.title = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Organization" }
                                input {
                                    class: "text-input",
                                    r#type: "text",
                                    value: "{current_form.organization}",
                                    oninput: move |evt| update_form(form, move |draft| draft.organization = evt.value()),
                                }
                            }
                        }

                        label { class: "persona-field persona-field-wide",
                            span { "Summary" }
                            textarea {
                                class: "persona-textarea persona-textarea-sm",
                                value: "{current_form.summary}",
                                oninput: move |evt| update_form(form, move |draft| draft.summary = evt.value()),
                            }
                        }
                        label { class: "persona-field persona-field-wide",
                            span { "Communication Style" }
                            textarea {
                                class: "persona-textarea persona-textarea-sm",
                                value: "{current_form.communication_style}",
                                oninput: move |evt| update_form(form, move |draft| draft.communication_style = evt.value()),
                            }
                        }

                        div { class: "persona-section-label", "Persona Lens" }
                        div { class: "persona-form-grid persona-form-grid-facts",
                            label { class: "persona-field",
                                span { "Jobs To Be Done" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.jobs_to_be_done}",
                                    oninput: move |evt| update_form(form, move |draft| draft.jobs_to_be_done = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Pains" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.pains}",
                                    oninput: move |evt| update_form(form, move |draft| draft.pains = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Adoption Yes If" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.adoption_yes_if}",
                                    oninput: move |evt| update_form(form, move |draft| draft.adoption_yes_if = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Rejection No If" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.rejection_no_if}",
                                    oninput: move |evt| update_form(form, move |draft| draft.rejection_no_if = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Anti Goals" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.anti_goals}",
                                    oninput: move |evt| update_form(form, move |draft| draft.anti_goals = evt.value()),
                                }
                            }
                            label { class: "persona-field",
                                span { "Recognition Cues" }
                                textarea {
                                    class: "persona-textarea",
                                    value: "{current_form.recognition_cues}",
                                    oninput: move |evt| update_form(form, move |draft| draft.recognition_cues = evt.value()),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path) {
        let skill_dir = dir.join("assets/skills/user-personas");
        std::fs::create_dir_all(skill_dir.join("personas")).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Personas\n").unwrap();
    }

    #[test]
    fn generate_persona_from_notes_populates_required_sections() {
        let form = generate_persona_from_notes(
            "Persona for a platform engineer at Acme who needs safer self-serve deploys.",
            1,
        );

        assert_eq!(form.title, "Platform Engineer");
        assert_eq!(form.organization, "Acme");
        assert!(form.jobs_to_be_done.contains("self-serve deploys"));
        assert!(form.pains.contains("Current process"));
        assert!(form.adoption_yes_if.contains("first run"));
        assert!(form.rejection_no_if.contains("replace working tools"));
        assert!(form.anti_goals.contains("generic demo"));
        assert!(form.recognition_cues.contains("platform"));
    }

    #[test]
    fn personas_dir_is_relative_to_root_dot_caretta() {
        // The Persona Studio must always save personas under
        // `<root>/.caretta/personas/`, regardless of where the resolved
        // SKILL.md happens to live (including absolute paths that point at
        // the materialized app-data copy, e.g. macOS
        // `~/Library/Application Support/caretta/skills/...`). This
        // guarantees personas stay inside the project tree alongside the
        // Discovery workspace.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();

        let from_relative = personas_dir(root, "assets/skills/user-personas/SKILL.md");
        let from_absolute = personas_dir(
            root,
            "/Users/example/Library/Application Support/caretta/skills/user-personas/SKILL.md",
        );
        let from_empty = personas_dir(root, "");

        let expected = dir.path().join(".caretta").join("personas");
        assert_eq!(from_relative, expected);
        assert_eq!(from_absolute, expected);
        assert_eq!(from_empty, expected);
    }

    #[test]
    fn save_and_load_persona_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path());
        let root = dir.path().to_str().unwrap();
        let skill = "assets/skills/user-personas/SKILL.md";
        let form = PersonaForm {
            file_name: "research-ops.json".to_string(),
            name: "Riley Chen".to_string(),
            title: "Research Ops Lead".to_string(),
            organization: "Northwind".to_string(),
            summary: "Coordinates research intake and synthesis.".to_string(),
            jobs_to_be_done: "Keep research findings reusable.".to_string(),
            pains: "Persona evidence gets scattered.".to_string(),
            adoption_yes_if: "Personas are durable project assets.".to_string(),
            rejection_no_if: "Requires a separate research repository.".to_string(),
            anti_goals: "Does not want marketing-only personas.".to_string(),
            recognition_cues: "Asks where evidence lives.".to_string(),
            ..PersonaForm::default()
        };

        let saved = save_persona(root, skill, &form).unwrap();
        assert_eq!(saved, "research-ops.json");

        // The file must land in `<root>/.caretta/personas/`, *not* next to the
        // SKILL.md (`assets/skills/user-personas/personas/`).
        let canonical = dir.path().join(".caretta/personas/research-ops.json");
        assert!(
            canonical.is_file(),
            "expected persona at {}",
            canonical.display()
        );
        let legacy = dir
            .path()
            .join("assets/skills/user-personas/personas/research-ops.json");
        assert!(
            !legacy.exists(),
            "persona must not be written to the legacy SKILL-adjacent dir: {}",
            legacy.display()
        );

        let loaded = load_persona_form(root, skill, &saved).unwrap();
        assert_eq!(loaded.name, "Riley Chen");
        assert_eq!(loaded.title, "Research Ops Lead");
        assert_eq!(loaded.jobs_to_be_done, "Keep research findings reusable.");

        let summaries = load_personas(root, skill).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "Riley Chen");
        assert_eq!(
            summaries[0].jobs_to_be_done,
            vec!["Keep research findings reusable.".to_string()]
        );
    }

    #[test]
    fn save_preserves_unrelated_other_facts() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path());
        let root = dir.path().to_str().unwrap();
        let skill = "assets/skills/user-personas/SKILL.md";
        // Seed a persona in the legacy SKILL-adjacent location so we can
        // verify that the next save migrates the document to
        // `<root>/.caretta/personas/` while still preserving unrelated
        // `other_facts` entries.
        let legacy_path = dir
            .path()
            .join("assets/skills/user-personas/personas/operator.json");
        std::fs::write(
            &legacy_path,
            serde_json::to_string_pretty(&json!({
                "type": "TinyPerson",
                "persona": {
                    "name": "Existing",
                    "other_facts": [
                        "current_stack: Kubernetes",
                        "jobs_to_be_done: Old job"
                    ]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let mut form = load_persona_form(root, skill, "operator.json").unwrap();
        form.jobs_to_be_done = "New job".to_string();
        save_persona(root, skill, &form).unwrap();

        let canonical_path = dir.path().join(".caretta/personas/operator.json");
        let saved = std::fs::read_to_string(&canonical_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", canonical_path.display()));
        assert!(saved.contains("current_stack: Kubernetes"));
        assert!(saved.contains("jobs_to_be_done: New job"));
        assert!(!saved.contains("jobs_to_be_done: Old job"));
        // The legacy copy is removed so the persona has a single source of
        // truth after migration.
        assert!(
            !legacy_path.exists(),
            "legacy persona should be removed after migration to .caretta/personas"
        );
    }

    #[test]
    fn delete_persona_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path());
        let root = dir.path().to_str().unwrap();
        let skill = "assets/skills/user-personas/SKILL.md";
        let form = PersonaForm {
            file_name: "delete-me.json".to_string(),
            name: "Delete Me".to_string(),
            ..PersonaForm::default()
        };
        let saved = save_persona(root, skill, &form).unwrap();
        assert!(
            dir.path()
                .join(".caretta/personas/delete-me.json")
                .is_file()
        );
        delete_persona(root, skill, &saved).unwrap();
        assert!(load_personas(root, skill).unwrap().is_empty());
    }

    #[test]
    fn delete_persona_also_clears_legacy_locations() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path());
        let root = dir.path().to_str().unwrap();
        let skill = "assets/skills/user-personas/SKILL.md";

        let legacy = dir
            .path()
            .join("assets/skills/user-personas/personas/legacy.json");
        std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&json!({
                "type": "TinyPerson",
                "persona": { "name": "Legacy" }
            }))
            .unwrap(),
        )
        .unwrap();

        delete_persona(root, skill, "legacy.json").unwrap();
        assert!(
            !legacy.exists(),
            "delete must remove the persona from legacy fallback locations"
        );
        assert!(load_personas(root, skill).unwrap().is_empty());
    }
}
