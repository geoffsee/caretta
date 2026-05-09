// Rust equivalent of the provided Bash/Python model scanner. Source: :contentReference[oaicite:0]{index=0}

use crate::bundled_agents::{SUPPORTED_AGENTS, iter_bundled_cli_ids};
use crate::utilities::ModelRegex;
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;

fn model_regex() -> &'static ModelRegex {
    static REGEX: OnceLock<ModelRegex> = OnceLock::new();
    REGEX.get_or_init(|| ModelRegex::new().expect("model regex should compile"))
}

#[derive(Debug, Clone, Serialize)]
pub struct CliScan {
    pub installed: bool,
    pub executable: Option<PathBuf>,
    pub resolved: Option<PathBuf>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CuratedModel(pub String, pub String);

pub type RawScanResult = BTreeMap<String, CliScan>;
pub type CuratedModels = BTreeMap<String, Vec<CuratedModel>>;

/// Writes `assets/available-models.json` under `repo_root` using model strings
/// harvested from each provider's **bundled** `node_modules` entrypoint under
/// `runtime_root` (`freq-ai-agent-runtime`'s crate directory).
pub fn scan_available_models(
    repo_root: impl AsRef<Path>,
    runtime_root: impl AsRef<Path>,
) -> io::Result<(RawScanResult, CuratedModels)> {
    let raw = scan_all_clis(runtime_root.as_ref())?;
    let curated = curate_all(&raw);

    let assets_dir = repo_root.as_ref().join("assets");
    fs::create_dir_all(&assets_dir)?;

    let out_path = assets_dir.join("available-models.json");
    let json = serde_json::to_string_pretty(&curated)?;
    fs::write(out_path, format!("{json}\n"))?;

    Ok((raw, curated))
}

pub fn scan_all_clis(runtime_root: &Path) -> io::Result<RawScanResult> {
    let mut result = BTreeMap::new();

    for id in iter_bundled_cli_ids() {
        result.insert(id.to_string(), scan_cli(runtime_root, id)?);
    }

    Ok(result)
}

/// Scan one agent's bundled JavaScript entrypoint under `runtime_root` (the
/// `freq-ai-agent-runtime` crate directory that contains `node_modules`).
pub fn scan_cli(runtime_root: &Path, cli: &str) -> io::Result<CliScan> {
    let Some(entrypoint_path) = embedded_entrypoint_path(runtime_root, cli) else {
        return Ok(CliScan {
            installed: false,
            executable: None,
            resolved: None,
            models: Vec::new(),
        });
    };

    let resolved = fs::canonicalize(&entrypoint_path).unwrap_or_else(|_| entrypoint_path.clone());
    let root = resolved
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut models = BTreeSet::new();

    for model in scan_one_file(&resolved) {
        models.insert(model);
    }

    for entry in WalkDir::new(&root)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();

        if !path.is_file() || !is_scannable_file(path) {
            continue;
        }

        for model in scan_one_file(path) {
            models.insert(model);
        }
    }

    Ok(CliScan {
        installed: true,
        executable: Some(entrypoint_path),
        resolved: Some(resolved),
        models: models.into_iter().collect(),
    })
}

fn embedded_entrypoint_path(runtime_root: &Path, cli_id: &str) -> Option<PathBuf> {
    let agent = SUPPORTED_AGENTS.iter().find(|a| a.id == cli_id)?;
    if agent.external {
        return None;
    }

    let rel = agent.entrypoint?;
    let path = runtime_root.join(rel);
    if path.is_file() {
        return Some(path);
    }
    None
}

fn is_scannable_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js" | "cjs" | "mjs" | "json")
    )
}

fn scan_one_file(path: &Path) -> Vec<String> {
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };

    let content = extract_printable_strings(&bytes);
    extract_models(&content)
}

fn extract_printable_strings(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut current = Vec::new();

    for &byte in bytes {
        if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' {
            current.push(byte);
        } else {
            if current.len() >= 4 {
                out.push_str(&String::from_utf8_lossy(&current));
                out.push('\n');
            }

            current.clear();
        }
    }

    if current.len() >= 4 {
        out.push_str(&String::from_utf8_lossy(&current));
        out.push('\n');
    }

    out
}

fn extract_models(input: &str) -> Vec<String> {
    let lower = input.to_lowercase();

    let reject_suffix_re = Regex::new(
        r"(cannot|sqlite|example|failed|account|desktop|review|settings|context|user|voice|staging|folder|hiring|actions|hidden|http|local|native|proactive|prompt|socks|swift|allowed)$",
    )
        .expect("reject suffix regex should compile");

    let mut models = BTreeSet::new();

    for model_match in model_regex().explain_matches(&lower) {
        let mut model = model_match.matched_text.trim().to_string();

        while model.ends_with('.') || model.ends_with('-') {
            model.pop();
        }

        if model.len() <= 2 {
            continue;
        }

        if reject_suffix_re.is_match(&model) {
            continue;
        }

        if model.ends_with("e.g") || model.ends_with("eg") {
            continue;
        }

        models.insert(model);
    }

    models.into_iter().collect()
}

pub fn curate_all(raw: &RawScanResult) -> CuratedModels {
    let agents = [
        AgentSpec {
            agent: "claude",
            sources: &["claude"],
            prefixes: &["claude-"],
            primary_prefix: "claude-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "cline",
            sources: &["cline"],
            prefixes: &["claude-", "gemini-", "gpt-", "grok-"],
            primary_prefix: "gpt-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "codex",
            sources: &["codex"],
            prefixes: &["gpt-"],
            primary_prefix: "gpt-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "copilot",
            sources: &["copilot"],
            prefixes: &["gpt-", "claude-"],
            primary_prefix: "gpt-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "gemini",
            sources: &["gemini"],
            prefixes: &["gemini-"],
            primary_prefix: "gemini-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "grok",
            sources: &["grok"],
            prefixes: &["grok-"],
            primary_prefix: "grok-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "junie",
            sources: &["junie", "claude"],
            prefixes: &["claude-"],
            primary_prefix: "claude-",
            require_agent_installed: true,
        },
        AgentSpec {
            agent: "xai",
            sources: &["xai"],
            prefixes: &["grok-"],
            primary_prefix: "grok-",
            require_agent_installed: true,
        },
    ];

    let mut result = BTreeMap::new();

    for spec in agents {
        let agent_installed = raw
            .get(spec.agent)
            .map(|scan| scan.installed)
            .unwrap_or(false);

        if spec.require_agent_installed && !agent_installed {
            result.insert(spec.agent.to_string(), Vec::new());
            continue;
        }

        let mut pool = BTreeSet::new();

        for source in spec.sources {
            if let Some(scan) = raw.get(*source) {
                pool.extend(scan.models.iter().cloned());
            }
        }

        let mut models = curate_models(&pool, spec.prefixes);
        sort_models(&mut models, spec.primary_prefix);

        let labelled = models
            .into_iter()
            .map(|model| {
                let label = make_label(&model);
                CuratedModel(model, label)
            })
            .collect();

        result.insert(spec.agent.to_string(), labelled);
    }

    result
}

#[derive(Debug, Clone, Copy)]
struct AgentSpec {
    agent: &'static str,
    sources: &'static [&'static str],
    prefixes: &'static [&'static str],
    primary_prefix: &'static str,
    require_agent_installed: bool,
}

fn curate_models(models: &BTreeSet<String>, prefixes: &[&str]) -> Vec<String> {
    if prefixes.is_empty() {
        return Vec::new();
    }

    let skip_bare = ["gpt", "grok", "sonnet", "opus", "haiku"];

    let skip_re = Regex::new(r"-(beta|preview|exp|live|base)(-|$)|-(specific|customtools)$|-v\d+$")
        .expect("skip regex should compile");

    let old_claude_re =
        Regex::new(r"^claude-(3|2|instant|\d+-)").expect("old claude regex should compile");

    let mut keep = Vec::new();

    for model in models {
        if skip_bare.contains(&model.as_str()) {
            continue;
        }

        if !prefixes.iter().any(|prefix| model.starts_with(prefix)) {
            continue;
        }

        if old_claude_re.is_match(model) {
            continue;
        }

        if is_old_new_style_claude(model) {
            continue;
        }

        if should_prefer_gpt_dot_form(model, models) {
            continue;
        }

        if skip_re.is_match(model) {
            continue;
        }

        if is_redundant(model, models) {
            continue;
        }

        keep.push(model.clone());
    }

    keep
}

fn is_old_new_style_claude(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("claude-") else {
        return false;
    };

    let parts = rest.split('-').collect::<Vec<_>>();

    if parts.len() < 2 {
        return false;
    }

    if !matches!(parts[0], "haiku" | "sonnet" | "opus") {
        return false;
    }

    let Ok(major) = parts[1].parse::<u64>() else {
        return false;
    };

    major < 4
}

fn should_prefer_gpt_dot_form(model: &str, models: &BTreeSet<String>) -> bool {
    if !model.starts_with("gpt-") {
        return false;
    }

    let re = Regex::new(r"^(gpt-\d+)-(\d+)").expect("gpt dot regex should compile");
    let dot = re.replace(model, "$1.$2").to_string();

    dot != model && models.contains(&dot)
}

fn is_redundant(model: &str, pool: &BTreeSet<String>) -> bool {
    let dated_re = Regex::new(r"-\d{8}(-v\d+)?$").expect("dated regex should compile");
    let short_numeric_re = Regex::new(r"-\d{3}$").expect("short numeric regex should compile");

    let base = dated_re.replace(model, "").to_string();
    if base != model && pool.contains(&base) {
        return true;
    }

    let base = short_numeric_re.replace(model, "").to_string();
    if base != model && pool.contains(&base) {
        return true;
    }

    if let Some(base) = model.strip_suffix("-0")
        && pool.contains(base)
    {
        return true;
    }

    if let Some(base) = model.strip_suffix("-latest")
        && pool.contains(base)
    {
        return true;
    }

    false
}

fn sort_models(models: &mut [String], primary_prefix: &str) {
    match primary_prefix {
        "claude-" => models.sort_by_key(|model| claude_sort_key(model)),
        "gpt-" => models.sort_by_key(|model| provider_sort_key(model)),
        "gemini-" => models.sort_by_key(|model| provider_sort_key(model)),
        "grok-" => models.sort_by_key(|model| provider_sort_key(model)),
        _ => models.sort_by_key(|model| provider_sort_key(model)),
    }
}

fn claude_sort_key(model: &str) -> (u8, Vec<i64>, String) {
    let tier_order = HashMap::from([("opus", 0_u8), ("sonnet", 1_u8), ("haiku", 2_u8)]);

    let rest = model.strip_prefix("claude-").unwrap_or(model);

    let dated_re = Regex::new(r"-\d{8}$").expect("dated sort regex should compile");
    let rest = dated_re.replace(rest, "");
    let parts = rest.split('-').collect::<Vec<_>>();

    let tier = tier_order
        .get(parts.first().copied().unwrap_or(""))
        .copied()
        .unwrap_or(99);

    let mut nums = parts
        .iter()
        .skip(1)
        .filter_map(|part| part.parse::<i64>().ok())
        .map(|n| -n)
        .collect::<Vec<_>>();

    if nums.is_empty() {
        nums.push(1);
    }

    nums.push(0);

    (tier, nums, model.to_string())
}

fn provider_sort_key(model: &str) -> ProviderSortKey {
    let family = if model.starts_with("gpt-") {
        0
    } else if model.starts_with("claude-") {
        1
    } else if model.starts_with("gemini-") {
        2
    } else if model.starts_with("grok-") {
        3
    } else {
        99
    };

    let nums = Regex::new(r"\d+(?:\.\d+)?")
        .expect("number regex should compile")
        .find_iter(model)
        .filter_map(|m| m.as_str().parse::<f64>().ok())
        .map(|n| (n * 1000.0) as i64)
        .map(|n| -n)
        .collect::<Vec<_>>();

    ProviderSortKey {
        family,
        nums,
        model: model.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProviderSortKey {
    family: u8,
    nums: Vec<i64>,
    model: String,
}

fn make_label(model: &str) -> String {
    if let Some(model_match) = model_regex().explain_match(model) {
        return match model_match.part.name {
            "claude_instant" | "claude_numbered_family" | "claude_family_first" => {
                label_claude(model)
            }
            "gemini" => label_gemini(model),
            "grok" => label_grok(model),
            "gpt" => label_gpt(model),
            "codex" => {
                if model.starts_with("gpt-") {
                    label_gpt(model)
                } else {
                    label_codex_cli(model)
                }
            }
            "bare_alias" => model.to_string(),
            _ => label_fallback_by_prefix(model),
        };
    }

    label_fallback_by_prefix(model)
}

fn label_fallback_by_prefix(model: &str) -> String {
    if model.starts_with("claude-") {
        label_claude(model)
    } else if model.starts_with("gpt-") {
        label_gpt(model)
    } else if model.starts_with("gemini-") {
        label_gemini(model)
    } else if model.starts_with("grok-") {
        label_grok(model)
    } else {
        model.to_string()
    }
}

fn label_codex_cli(model: &str) -> String {
    let rest = model.strip_prefix("codex-").unwrap_or(model);
    let words = rest
        .split('-')
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" ");
    format!("Codex {words}")
}

fn label_claude(model: &str) -> String {
    let rest = model.strip_prefix("claude-").unwrap_or(model);
    let dated_re = Regex::new(r"-\d{8}$").expect("claude date regex should compile");
    let rest = dated_re.replace(rest, "");
    let parts = rest.split('-').collect::<Vec<_>>();

    let tier = parts
        .first()
        .map(|part| title_case(part))
        .unwrap_or_default();

    let version = if parts.len() > 1 {
        parts[1..].join(".")
    } else {
        String::new()
    };

    if version.is_empty() {
        tier
    } else {
        format!("{tier} {version}")
    }
}

fn label_gpt(model: &str) -> String {
    let rest = model.strip_prefix("gpt-").unwrap_or(model);
    let parts = rest.split('-').collect::<Vec<_>>();

    let version = parts.first().copied().unwrap_or_default();
    let qualifier = parts
        .iter()
        .skip(1)
        .map(|part| title_case(part))
        .collect::<Vec<_>>()
        .join(" ");

    if qualifier.is_empty() {
        format!("GPT-{version}")
    } else {
        format!("GPT-{version} {qualifier}")
    }
}

fn label_gemini(model: &str) -> String {
    let rest = model.strip_prefix("gemini-").unwrap_or(model);
    let parts = rest.split('-').collect::<Vec<_>>();

    let version = parts.first().copied().unwrap_or_default();
    let qualifier = parts
        .iter()
        .skip(1)
        .map(|part| title_case(part))
        .collect::<Vec<_>>()
        .join(" ");

    if qualifier.is_empty() {
        format!("Gemini {version}")
    } else {
        format!("Gemini {version} {qualifier}")
    }
}

fn label_grok(model: &str) -> String {
    let rest = model.strip_prefix("grok-").unwrap_or(model);
    let parts = rest.split('-').collect::<Vec<_>>();

    let mut version_parts = Vec::new();
    let mut qualifier_parts = Vec::new();
    let mut in_qualifier = false;

    for part in parts {
        if !in_qualifier && part.chars().all(|c| c.is_ascii_digit()) {
            version_parts.push(part);
        } else {
            in_qualifier = true;
            qualifier_parts.push(part);
        }
    }

    let version = version_parts.join(".");
    let qualifier = qualifier_parts
        .into_iter()
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" ");

    if qualifier.is_empty() {
        format!("Grok {version}")
    } else {
        format!("Grok {version} {qualifier}")
    }
}

fn title_case(input: &str) -> String {
    let mut chars = input.chars();

    match chars.next() {
        Some(first) => {
            let first = first.to_uppercase().collect::<String>();
            let rest = chars.as_str().to_lowercase();
            format!("{first}{rest}")
        }
        None => String::new(),
    }
}

/// Serializes a raw scan for debugging; re-exported from the library and used
/// from unit tests (not referenced from the `build.rs` scan path).
#[allow(dead_code)]
pub fn raw_scan_to_json(raw: &RawScanResult) -> serde_json::Value {
    let mut object = serde_json::Map::new();

    for (cli, scan) in raw {
        object.insert(
            cli.clone(),
            json!({
                "installed": scan.installed,
                "executable": scan.executable,
                "resolved": scan.resolved,
                "models": scan.models,
            }),
        );
    }

    serde_json::Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn extracts_models_from_text() {
        let text = r#"
            claude-sonnet-4
            claude-opus-4-20250514
            gpt-5
            gpt-4.1-mini
            gemini-2.5-pro
            grok-4-fast
            example
        "#;

        let models = extract_models(text);

        assert!(models.contains(&"claude-sonnet-4".to_string()));
        assert!(models.contains(&"claude-opus-4-20250514".to_string()));
        assert!(models.contains(&"gpt-5".to_string()));
        assert!(models.contains(&"gpt-4.1-mini".to_string()));
        assert!(models.contains(&"gemini-2.5-pro".to_string()));
        assert!(models.contains(&"grok-4-fast".to_string()));
    }

    #[test]
    fn curates_redundant_aliases() {
        let models = BTreeSet::from([
            "gpt-5".to_string(),
            "gpt-5-latest".to_string(),
            "gpt-5-2025-08-07".to_string(),
            "gpt".to_string(),
        ]);

        let curated = curate_models(&models, &["gpt-"]);

        assert!(curated.contains(&"gpt-5".to_string()));
        assert!(!curated.contains(&"gpt-5-latest".to_string()));
        assert!(!curated.contains(&"gpt".to_string()));
    }

    #[test]
    fn labels_models() {
        assert_eq!(make_label("claude-sonnet-4"), "Sonnet 4");
        assert_eq!(make_label("gpt-4.1-mini"), "GPT-4.1 Mini");
        assert_eq!(make_label("gemini-2.5-pro"), "Gemini 2.5 Pro");
        assert_eq!(make_label("grok-4-fast"), "Grok 4 Fast");
        assert_eq!(
            make_label("grok-4-fast-non-reasoning"),
            "Grok 4 Fast Non Reasoning"
        );
    }

    #[test]
    fn cline_curates_own_detected_models() {
        let mut raw = RawScanResult::new();

        raw.insert(
            "cline".to_string(),
            CliScan {
                installed: true,
                executable: Some(PathBuf::from("/tmp/cline")),
                resolved: Some(PathBuf::from("/tmp/cline")),
                models: vec![
                    "claude-sonnet-4".to_string(),
                    "gemini-2.5-pro".to_string(),
                    "gpt-5".to_string(),
                    "grok-4".to_string(),
                    "gpt".to_string(),
                ],
            },
        );

        for id in iter_bundled_cli_ids() {
            raw.entry(id.to_string()).or_insert(CliScan {
                installed: false,
                executable: None,
                resolved: None,
                models: Vec::new(),
            });
        }

        let curated = curate_all(&raw);
        let cline = curated.get("cline").expect("cline should exist");

        assert!(cline.iter().any(|model| model.0 == "gpt-5"));
        assert!(cline.iter().any(|model| model.0 == "claude-sonnet-4"));
        assert!(cline.iter().any(|model| model.0 == "gemini-2.5-pro"));
        assert!(cline.iter().any(|model| model.0 == "grok-4"));
        assert!(!cline.iter().any(|model| model.0 == "gpt"));
    }

    #[test]
    fn xai_is_empty_when_xai_is_not_installed() {
        let mut raw = RawScanResult::new();

        raw.insert(
            "grok".to_string(),
            CliScan {
                installed: true,
                executable: Some(PathBuf::from("/tmp/grok")),
                resolved: Some(PathBuf::from("/tmp/grok")),
                models: vec!["grok-4".to_string()],
            },
        );

        raw.insert(
            "xai".to_string(),
            CliScan {
                installed: false,
                executable: None,
                resolved: None,
                models: Vec::new(),
            },
        );

        for id in iter_bundled_cli_ids() {
            raw.entry(id.to_string()).or_insert(CliScan {
                installed: false,
                executable: None,
                resolved: None,
                models: Vec::new(),
            });
        }

        let curated = curate_all(&raw);
        assert!(curated.get("xai").unwrap().is_empty());
    }

    #[test]
    fn scans_real_clis_without_failing() {
        let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let raw = scan_all_clis(runtime_root).expect("scan should not fail");

        for id in iter_bundled_cli_ids() {
            assert!(raw.contains_key(id));
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&raw_scan_to_json(&raw)).unwrap()
        );

        let curated = curate_all(&raw);
        println!("{}", serde_json::to_string_pretty(&curated).unwrap());
    }
}
