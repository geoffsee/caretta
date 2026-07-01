// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use std::path::{Path, PathBuf};
use std::process::Command;

fn resolve_web_assets_dir() -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir);

    if let Ok(custom_path) = std::env::var("CARETTA_WEB_ASSETS_DIR") {
        let custom = Path::new(&custom_path).to_path_buf();
        if custom.exists() {
            return custom.canonicalize().map_err(|e| {
                format!("failed to canonicalize CARETTA_WEB_ASSETS_DIR={custom_path}: {e}")
            });
        }
        println!(
            "cargo::warning=CARETTA_WEB_ASSETS_DIR is set but path does not exist: {custom_path}"
        );
    }

    let candidates = [
        manifest_dir.join("dist"),
        workspace_dir.join("dist"),
        workspace_dir.join("target/dx/caretta/debug/web/public"),
        workspace_dir.join("target/dx/caretta/release/web/public"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            let abs = candidate
                .canonicalize()
                .map_err(|e| format!("failed to canonicalize {}: {e}", candidate.display()))?;
            if abs.join("index.html").exists() {
                return Ok(abs);
            }
            if abs.join("index.htm").exists() {
                return Ok(abs);
            }
        }
    }

    Err("unable to resolve web assets directory containing index.html".to_string())
}

fn main() {
    let web_assets_dir = match resolve_web_assets_dir() {
        Ok(dir) => {
            println!("cargo::rerun-if-changed={}", dir.display());
            dir
        }
        Err(reason) => {
            println!(
                "cargo::warning=web assets not found: {reason}; expected crates/cli/dist, dist, or target/dx/caretta/<mode>/web/public"
            );
            let out_dir = std::env::var("OUT_DIR").unwrap();
            let stub = Path::new(&out_dir).join("dist-stub");
            std::fs::create_dir_all(&stub).expect("create dist-stub");
            stub
        }
    };

    // Tell server.rs where to find the web assets folder. If no bundle
    // is available, we fallback to an empty stub so RustEmbed can still
    // compile and emit a clear warning.
    println!(
        "cargo::rustc-env=WEB_ASSETS_DIR={}",
        web_assets_dir.display()
    );

    let wasm_path = web_assets_dir.join("wasm/caretta_bg.wasm");

    println!("cargo::rerun-if-changed={}", wasm_path.display());

    if !wasm_path.exists() {
        return;
    }

    let meta = std::fs::metadata(&wasm_path).expect("failed to read wasm metadata");
    // Only optimize if larger than 10MB (i.e. not already optimized)
    if meta.len() <= 10 * 1024 * 1024 {
        return;
    }

    let wasm_opt = option_env!("WASM_OPT").unwrap_or("wasm-opt");
    let status = Command::new(wasm_opt)
        .args(["-Oz", "--strip-debug"])
        .arg(&wasm_path)
        .arg("-o")
        .arg(&wasm_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            let new_size = std::fs::metadata(&wasm_path)
                .map(|m| m.len() / (1024 * 1024))
                .unwrap_or(0);
            println!(
                "cargo::warning=wasm-opt: optimized caretta_bg.wasm to {}MB",
                new_size
            );
        }
        Ok(s) => println!("cargo::warning=wasm-opt exited with {s}, skipping optimization"),
        Err(_) => println!("cargo::warning=wasm-opt not found, skipping wasm optimization"),
    }
}
