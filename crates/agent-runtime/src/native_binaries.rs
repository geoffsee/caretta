// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Native agent CLI binaries fetched at build time (Antigravity `agy`, xAI `grok`).
//!
//! These replace deprecated npm-distributed CLIs (`@google/gemini-cli`,
//! `@kazuki-ookura/grok-cli`) and the former xAI Copilot proxy bundle.
//!
//! Most helpers here are consumed by `build_native_binaries.rs`; the runtime
//! crate re-exports the install table for metadata/tests.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub const GROK_VERSION_LOCK: &str = "native-binaries.lock.json";

#[derive(Debug, Clone, Copy)]
pub struct NativeBinaryInstall {
    pub agent_id: &'static str,
    pub runtime_path: &'static str,
}

pub const NATIVE_BINARIES: &[NativeBinaryInstall] = &[
    NativeBinaryInstall {
        agent_id: "gemini",
        runtime_path: "bin/agy",
    },
    NativeBinaryInstall {
        agent_id: "xai",
        runtime_path: "bin/grok",
    },
];

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NativeBinaryLock {
    pub xai: String,
    pub antigravity: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AntigravityManifest {
    pub version: String,
    pub url: String,
    pub sha512: String,
}

#[allow(dead_code)]
pub fn native_binary_by_agent(agent_id: &str) -> Option<&'static NativeBinaryInstall> {
    NATIVE_BINARIES
        .iter()
        .find(|spec| spec.agent_id == agent_id)
}

#[allow(dead_code)]
pub fn runtime_binary_path(runtime_root: &Path, spec: &NativeBinaryInstall) -> PathBuf {
    runtime_root.join(spec.runtime_path)
}

#[allow(dead_code)]
pub fn native_binaries_supported(
    target_os: &str,
    target_arch: &str,
    target_env: Option<&str>,
) -> bool {
    antigravity_platform(target_os, target_arch, target_env).is_some()
        && grok_platform(target_os, target_arch).is_some()
}

#[allow(dead_code)]
pub fn antigravity_platform(
    target_os: &str,
    target_arch: &str,
    target_env: Option<&str>,
) -> Option<String> {
    let os = match target_os {
        "macos" => "darwin",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match target_arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    if os == "linux" && target_env == Some("musl") {
        Some(format!("linux_{arch}_musl"))
    } else {
        Some(format!("{os}_{arch}"))
    }
}

#[allow(dead_code)]
pub fn grok_platform(target_os: &str, target_arch: &str) -> Option<String> {
    let os = match target_os {
        "macos" => "macos",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match target_arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

#[allow(dead_code)]
pub fn antigravity_manifest_url(platform: &str) -> String {
    format!(
        "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/{platform}.json"
    )
}

#[allow(dead_code)]
pub fn grok_artifact_url(version: &str, platform: &str) -> String {
    format!("https://x.ai/cli/grok-{version}-{platform}")
}

#[allow(dead_code)]
pub fn grok_artifact_fallback_url(version: &str, platform: &str) -> String {
    format!(
        "https://storage.googleapis.com/grok-build-public-artifacts/cli/grok-{version}-{platform}"
    )
}

#[allow(dead_code)]
pub fn parse_antigravity_manifest(json: &str) -> Result<AntigravityManifest, String> {
    serde_json::from_str(json).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_platform_maps_musl_linux() {
        assert_eq!(
            antigravity_platform("linux", "x86_64", Some("musl")),
            Some("linux_amd64_musl".to_string())
        );
        assert_eq!(
            antigravity_platform("macos", "aarch64", None),
            Some("darwin_arm64".to_string())
        );
    }

    #[test]
    fn native_binaries_are_not_supported_on_windows() {
        assert!(!native_binaries_supported(
            "windows",
            "x86_64",
            Some("msvc")
        ));
        assert_eq!(
            antigravity_platform("windows", "x86_64", Some("msvc")),
            None
        );
        assert_eq!(grok_platform("windows", "x86_64"), None);
    }

    #[test]
    fn grok_platform_uses_macos_prefix() {
        assert_eq!(
            grok_platform("macos", "aarch64"),
            Some("macos-aarch64".to_string())
        );
    }
}
