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

/// Compare `data` against `expected` (a hex SHA-256 string). Returns an error
/// on mismatch so callers can surface a clear diagnostic.
#[cfg(any(feature = "bundle-runtime", test))]
fn check_hash(path: &str, expected: &str, data: &[u8]) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(data));
    anyhow::ensure!(
        actual == expected,
        "asset integrity check failed\n  asset:    {path}\n  expected: {expected}\n  actual:   {actual}\nbinary may be corrupted or tampered — obtain a fresh binary from the official release"
    );
    Ok(())
}

/// Verify that every embedded skill/workflow asset matches its build-time
/// SHA-256 hash recorded in the bundle manifest, and that every embedded
/// asset has a corresponding manifest entry.
///
/// Only compiled for `--features bundle-runtime` builds. Returns an error
/// describing the first integrity failure; callers should treat `Err` as fatal.
#[cfg(feature = "bundle-runtime")]
pub fn verify_asset_hashes() -> anyhow::Result<()> {
    use std::collections::HashSet;

    // Pass 1: verify each manifest entry against the embedded asset.
    for (path, expected_hash) in manifest::ASSET_MANIFEST {
        if let Some(rest) = path.strip_prefix("skills/") {
            let data = SkillAssets::get(rest)
                .ok_or_else(|| {
                    anyhow::anyhow!("asset integrity error — missing skill asset '{rest}'")
                })?
                .data;
            check_hash(path, expected_hash, data.as_ref())?;
        } else if let Some(rest) = path.strip_prefix("workflows/") {
            let data = WorkflowAssets::get(rest)
                .ok_or_else(|| {
                    anyhow::anyhow!("asset integrity error — missing workflow asset '{rest}'")
                })?
                .data;
            check_hash(path, expected_hash, data.as_ref())?;
        } else if *path == "AGENTS.md" {
            check_hash(path, expected_hash, AGENTS_MD.as_bytes())?;
        } else if *path == "labels.yml" {
            check_hash(path, expected_hash, LABELS_YML.as_bytes())?;
        } else if *path == "available-models.json" {
            check_hash(path, expected_hash, AVAILABLE_MODELS_JSON.as_bytes())?;
        } else {
            anyhow::bail!("asset integrity error — unrecognized path prefix: {path}");
        }
    }

    // Pass 2: ensure every embedded asset has a manifest entry, so that a
    // file present in SkillAssets/WorkflowAssets but absent from the manifest
    // does not pass silently.
    let manifest_paths: HashSet<&str> =
        manifest::ASSET_MANIFEST.iter().map(|(p, _)| *p).collect();

    for file in SkillAssets::iter() {
        let key = format!("skills/{}", file.as_ref());
        if !manifest_paths.contains(key.as_str()) {
            anyhow::bail!(
                "asset integrity error — embedded skill '{}' has no manifest entry",
                file.as_ref()
            );
        }
    }

    for file in WorkflowAssets::iter() {
        let key = format!("workflows/{}", file.as_ref());
        if !manifest_paths.contains(key.as_str()) {
            anyhow::bail!(
                "asset integrity error — embedded workflow '{}' has no manifest entry",
                file.as_ref()
            );
        }
    }

    Ok(())
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
mod tests {
    use sha2::{Digest, Sha256};
    use std::path::Path;

    #[test]
    fn check_hash_accepts_correct_hash() {
        let data = b"hello world";
        let hash = format!("{:x}", Sha256::digest(data));
        assert!(super::check_hash("test/asset.md", &hash, data).is_ok());
    }

    #[test]
    fn check_hash_rejects_wrong_hash() {
        let err = super::check_hash("test/asset.md", "deadbeef", b"hello world").unwrap_err();
        assert!(err.to_string().contains("asset integrity check failed"));
    }

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
