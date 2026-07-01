// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::cmd::{count_tokens, log};
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use toak_rs::{MarkdownGenerator, MarkdownGeneratorOptions};

/// Maximum tokens to include from the codebase snapshot in a prompt.
pub const MAX_SNAPSHOT_TOKENS: usize = 100_000;

#[cfg(not(target_arch = "wasm32"))]
pub fn generate_codebase_snapshot(root: &str) -> String {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(generate_codebase_snapshot_async(root)))
        }
        Ok(_) => {
            let root = root.to_string();
            std::thread::spawn(move || generate_codebase_snapshot_on_new_runtime(&root))
                .join()
                .unwrap_or_else(|_| {
                    log("WARNING: toak-rs snapshot worker thread panicked");
                    String::new()
                })
        }
        Err(_) => generate_codebase_snapshot_on_new_runtime(root),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn generate_codebase_snapshot_on_new_runtime(root: &str) -> String {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(generate_codebase_snapshot_async(root)),
        Err(e) => {
            log(&format!(
                "WARNING: failed to create Tokio runtime for toak-rs snapshot: {e}"
            ));
            String::new()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn generate_codebase_snapshot_async(root: &str) -> String {
    log("Generating codebase snapshot with toak-rs...");

    let snapshot_path = PathBuf::from(root).join("prompt.md");
    let opts = MarkdownGeneratorOptions {
        dir: PathBuf::from(root),
        output_file_path: snapshot_path.clone(),
        verbose: false,
        ..Default::default()
    };

    let mut generator = MarkdownGenerator::new(opts);

    let result = generator.create_markdown_document().await;

    let snapshot = match result {
        Ok(res) if res.success => std::fs::read_to_string(&snapshot_path).unwrap_or_default(),
        Ok(_) => {
            log(
                "WARNING: toak-rs markdown generation reported failure, continuing without snapshot",
            );
            String::new()
        }
        Err(e) => {
            log(&format!(
                "WARNING: toak-rs snapshot failed: {e}, continuing without snapshot"
            ));
            String::new()
        }
    };

    // Clean up the temp file.
    let _ = std::fs::remove_file(&snapshot_path);

    // Truncate if over budget.
    let tokens = count_tokens(&snapshot);
    if tokens > MAX_SNAPSHOT_TOKENS {
        log(&format!(
            "Snapshot is {tokens} tokens, truncating to {MAX_SNAPSHOT_TOKENS}"
        ));
        truncate_snapshot(snapshot, MAX_SNAPSHOT_TOKENS)
    } else {
        log(&format!("Snapshot ready ({tokens} tokens)"));
        snapshot
    }
}

#[cfg(target_arch = "wasm32")]
pub fn generate_codebase_snapshot(_root: &str) -> String {
    log("Skipping codebase snapshot on Wasm target.");
    String::new()
}

#[cfg(target_arch = "wasm32")]
pub async fn generate_codebase_snapshot_async(_root: &str) -> String {
    log("Skipping codebase snapshot on Wasm target.");
    String::new()
}

/// Truncate a snapshot string to fit within a token budget.
///
/// Uses a conservative 3-bytes-per-token estimate so the result never exceeds
/// `max_tokens` when re-tokenized.
pub fn truncate_snapshot(snapshot: String, max_tokens: usize) -> String {
    let max_bytes = max_tokens * 3;
    let truncated = if snapshot.len() > max_bytes {
        let mut end = max_bytes;
        while !snapshot.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &snapshot[..end]
    } else {
        &snapshot
    };
    format!(
        "{truncated}\n\n[... snapshot truncated at {max_tokens} tokens — use `toak` CLI for full exploration ...]"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn generate_codebase_snapshot_works_without_tokio_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        fs::write(root.join("main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "main.rs"])
            .current_dir(root)
            .output()
            .unwrap();

        let root = root.to_string_lossy().into_owned();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| generate_codebase_snapshot(&root));
        std::env::set_current_dir(original_dir).unwrap();

        let snapshot =
            result.expect("snapshot generation should not panic during synchronous dispatch");
        assert!(
            snapshot.contains("main.rs") || snapshot.contains("main"),
            "snapshot should contain the tracked source file"
        );
    }

    /// Exercises the sync wrapper inside a tokio runtime, matching GUI dispatch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_codebase_snapshot_works_inside_tokio_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // toak-rs requires a git repo — initialise one with a tracked file.
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        fs::write(root.join("main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "main.rs"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let root = root.to_string_lossy().into_owned();
        let content = generate_codebase_snapshot(&root);
        assert!(
            content.contains("main"),
            "snapshot should contain our source"
        );

        let tokens = count_tokens(&content);
        assert!(
            tokens > 0,
            "count_tokens should return >0 for non-empty input"
        );
    }

    #[test]
    fn truncate_snapshot_under_budget_still_appends_marker() {
        let input = "short".to_string();
        let result = truncate_snapshot(input.clone(), 100);
        assert!(result.starts_with("short"));
        assert!(result.contains("snapshot truncated"));
    }

    #[test]
    fn truncate_snapshot_over_budget_cuts_to_byte_limit() {
        // 10 tokens × 3 bytes/token = 30 bytes max
        let input = "a".repeat(100);
        let result = truncate_snapshot(input, 10);
        let body = result.split("\n\n[...").next().unwrap();
        assert_eq!(body.len(), 30);
    }

    #[test]
    fn truncate_snapshot_respects_char_boundaries() {
        // 'é' is 2 bytes in UTF-8. With max_tokens=5, max_bytes=15.
        // 7 × 'é' = 14 bytes, plus 'a' = 15 bytes exactly. Should not split mid-char.
        let input = "ééééééé".to_string(); // 14 bytes
        let result = truncate_snapshot(input.clone(), 5);
        // max_bytes = 15, input is 14 bytes, so it fits — no truncation of content
        let body = result.split("\n\n[...").next().unwrap();
        assert_eq!(body, "ééééééé");

        // Now force a mid-char split: 3 tokens × 3 = 9 bytes, 'ééééé' = 10 bytes
        let input2 = "ééééé".to_string(); // 10 bytes
        let result2 = truncate_snapshot(input2, 3);
        let body2 = result2.split("\n\n[...").next().unwrap();
        // Should back up to 8 bytes = 4 'é' chars
        assert_eq!(body2, "éééé");
        assert!(body2.len() <= 9);
    }

    #[test]
    fn truncate_snapshot_result_is_within_budget() {
        let input = "fn main() { println!(\"hello world\"); }\n".repeat(10_000);
        let max_tokens = 1_000;
        let result = truncate_snapshot(input, max_tokens);
        let tokens = count_tokens(&result);
        assert!(tokens <= max_tokens + 50); // allow some buffer for the footer
    }
}
