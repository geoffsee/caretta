use rust_embed::RustEmbed;
use std::path::PathBuf;

mod manifest {
    include!(concat!(env!("OUT_DIR"), "/asset_manifest_generated.rs"));
}

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

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use std::path::Path;

    /// Verifies that every build-time hash in the manifest matches the source
    /// file on disk. CI runs `cargo test --workspace` (no `bundle-runtime`),
    /// which exercises this path to catch stale or missing manifest entries.
    #[test]
    fn asset_manifest_hashes_match_source_files() {
        let assets_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for (path, expected_hash) in super::manifest::ASSET_MANIFEST {
            let abs = assets_root.join(path);
            let data =
                std::fs::read(&abs).unwrap_or_else(|e| panic!("cannot read asset '{path}': {e}"));
            let actual = format!("{:x}", Sha256::digest(&data));
            assert_eq!(
                actual, *expected_hash,
                "stale hash in manifest for '{path}': expected {expected_hash}, got {actual}"
            );
        }
    }

    /// Verifies that the manifest contains at least one skill and one workflow
    /// entry, guarding against a silent empty-manifest regression.
    #[test]
    fn asset_manifest_is_not_empty() {
        let entries = super::manifest::ASSET_MANIFEST;
        assert!(
            entries.iter().any(|(p, _)| p.starts_with("skills/")),
            "manifest contains no skill entries"
        );
        assert!(
            entries.iter().any(|(p, _)| p.starts_with("workflows/")),
            "manifest contains no workflow entries"
        );
    }
}

pub fn assets_dir() -> PathBuf {
    #[cfg(not(target_arch = "wasm32"))]
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));

    #[cfg(target_arch = "wasm32")]
    let base = PathBuf::from(".");

    let dir = base.join("caretta");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Verify that every embedded skill/workflow asset matches its build-time
/// SHA-256 hash recorded in the bundle manifest.
///
/// Only compiled for `--features bundle-runtime` builds. A mismatch aborts
/// the process with a clear error message; dev builds are unaffected.
#[cfg(feature = "bundle-runtime")]
pub fn verify_asset_hashes() {
    use sha2::{Digest, Sha256};

    for (path, expected_hash) in manifest::ASSET_MANIFEST {
        let data = if let Some(rest) = path.strip_prefix("skills/") {
            SkillAssets::get(rest)
                .unwrap_or_else(|| {
                    eprintln!("fatal: asset integrity error — missing skill asset '{rest}'");
                    std::process::exit(1);
                })
                .data
        } else if let Some(rest) = path.strip_prefix("workflows/") {
            WorkflowAssets::get(rest)
                .unwrap_or_else(|| {
                    eprintln!("fatal: asset integrity error — missing workflow asset '{rest}'");
                    std::process::exit(1);
                })
                .data
        } else {
            eprintln!("fatal: asset integrity error — unrecognized path prefix: {path}");
            std::process::exit(1);
        };

        let actual_hash = format!("{:x}", Sha256::digest(data.as_ref()));
        if actual_hash != *expected_hash {
            eprintln!(
                "fatal: asset integrity check failed\n  asset:    {path}\n  expected: {expected_hash}\n  actual:   {actual_hash}\nThis binary may have been tampered with."
            );
            std::process::exit(1);
        }
    }
}

/// Materialize embedded AGENTS.md and skills into the app-data directory.
/// Existing files are refreshed so the bundled guidance stays in sync with
/// the current binary.
/// Returns the app-data root (e.g. `~/.local/share/caretta`).
pub fn materialize_assets() -> PathBuf {
    #[cfg(feature = "bundle-runtime")]
    verify_asset_hashes();

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
