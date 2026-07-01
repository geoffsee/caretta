// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Embedded GitHub-App installer.
//!
//! Runs the
//! [`caretta-github-app-installer`](https://github.com/geoffsee/caretta-github-app-installer)
//! npm package using the Bun runtime shipped by [`caretta-agent-runtime`].
//! The installer is declared in `crates/agent-runtime/package.json` and lives
//! under `node_modules/caretta-github-app-installer/` after the runtime's
//! `bun install` step — the same mechanism used for the bundled agent CLIs —
//! so no sources are vendored or extracted from the binary.
//!
//! Flow:
//! 1. Resolve the embedded Bun executable via [`AgentRuntime::bun_path`].
//! 2. Resolve the installer entrypoint at
//!    `<runtime>/node_modules/caretta-github-app-installer/src/cli.ts`.
//! 3. Spawn `bun <entrypoint>` with `PORT`, `APP_NAME`, `GITHUB_ORG` and
//!    `WEBHOOK_URL` set from caller-supplied options.
//! 4. The installer opens GitHub's App-Manifest registration page, exchanges
//!    the returned code for credentials, and writes:
//!    - `~/.config/caretta/dev-ui-bot.pem` (private key, `chmod 600`)
//!    - `<working_dir>/.env.github-app` (App ID, secrets, PEM path).
//!
//! Only the option/command-construction layer is unit tested; the spawn
//! helper performs real I/O and is exercised through the desktop UI.

use agent_runtime::AgentRuntime;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// Default port used by the installer's local callback server when the caller
/// does not override `PORT`.
pub const DEFAULT_PORT: u16 = 3000;

/// Default GitHub App name (editable in the browser once registration starts).
pub const DEFAULT_APP_NAME: &str = "caretta-dev-bot";

/// Default webhook target embedded in the manifest. GitHub requires a public
/// URL even when webhook delivery is inactive.
pub const DEFAULT_WEBHOOK_URL: &str = "https://example.com/caretta-webhook";

/// npm package name declared in `crates/agent-runtime/package.json`.
pub const INSTALLER_PACKAGE: &str = "caretta-github-app-installer";

/// Entrypoint inside the installed package, relative to its `node_modules`
/// directory. The package's `bin` points at this same file.
const INSTALLER_ENTRYPOINT: &str = "node_modules/caretta-github-app-installer/src/cli.ts";

/// Caller-tunable knobs forwarded to the installer as environment variables.
/// Field-level `None` means *use the installer's own default*.
#[derive(Debug, Clone, Default)]
pub struct InstallerOptions {
    /// Port for the local callback server (`PORT`).
    pub port: Option<u16>,
    /// Default name presented on the GitHub registration form (`APP_NAME`).
    pub app_name: Option<String>,
    /// GitHub organization to register the app under (`GITHUB_ORG`).
    /// `None` registers it on the personal account performing the manifest flow.
    pub owner: Option<String>,
    /// Public webhook target embedded in the manifest (`WEBHOOK_URL`).
    pub webhook_url: Option<String>,
    /// Working directory used as `process.cwd()` by the installer; controls
    /// where the generated `.env.github-app` lands. Defaults to the runtime
    /// root when unset.
    pub working_dir: Option<PathBuf>,
}

impl InstallerOptions {
    /// Build the env-var list the installer expects. Empty/whitespace values
    /// are skipped so the installer falls back to its own defaults.
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = Vec::new();
        if let Some(port) = self.port {
            env.push(("PORT".to_string(), port.to_string()));
        }
        push_env(&mut env, "APP_NAME", self.app_name.as_deref());
        push_env(&mut env, "GITHUB_ORG", self.owner.as_deref());
        push_env(&mut env, "WEBHOOK_URL", self.webhook_url.as_deref());
        env
    }
}

fn push_env(into: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            into.push((key.to_string(), trimmed.to_string()));
        }
    }
}

/// Result of a single installer invocation.
#[derive(Debug, Clone)]
pub struct InstallerOutcome {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Directory the installer wrote `.env.github-app` to (same as the
    /// effective `working_dir`).
    pub working_dir: PathBuf,
}

/// Resolve the installer's entrypoint inside an embedded `AgentRuntime`. The
/// path is *not* checked for existence — callers that need a hard failure
/// when the package is missing should use [`require_installer_script`].
pub fn installer_script_path(runtime: &AgentRuntime) -> PathBuf {
    runtime.root().join(INSTALLER_ENTRYPOINT)
}

/// Like [`installer_script_path`] but errors when the file is missing,
/// catching the case where the runtime was built without the
/// `caretta-github-app-installer` npm dependency installed.
pub fn require_installer_script(runtime: &AgentRuntime) -> Result<PathBuf, String> {
    let path = installer_script_path(runtime);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Embedded installer script not found at {}. \
             Reinstall the agent runtime (`bun install` in crates/agent-runtime) \
             so the `{INSTALLER_PACKAGE}` npm package is materialized.",
            path.display()
        ))
    }
}

/// Build the `bun <cli.ts>` command without spawning it. Useful for tests and
/// for the UI to display "what would be run" before launching.
pub fn build_install_command(opts: &InstallerOptions) -> Result<Command, String> {
    let runtime = AgentRuntime::prepare()
        .map_err(|e| format!("Failed to prepare embedded agent runtime: {e}"))?;
    let script = require_installer_script(&runtime)?;

    // Re-use the runtime's command builder so we inherit the PATH overlay
    // (which puts the embedded Bun first, then the agent-CLI `.bin/` shims,
    // then the system PATH).
    let mut cmd = runtime.command_for_binary("bun");
    cmd.arg(script);

    let cwd = opts
        .working_dir
        .clone()
        .unwrap_or_else(|| runtime.root().to_path_buf());
    cmd.current_dir(cwd);

    for (k, v) in opts.env_pairs() {
        cmd.env(k, v);
    }

    Ok(cmd)
}

/// Spawn the installer and wait for completion, returning the captured
/// output. Blocks the calling thread; the desktop UI invokes this through
/// [`tokio::task::spawn_blocking`].
pub fn run_installer_blocking(opts: &InstallerOptions) -> Result<InstallerOutcome, String> {
    let mut cmd = build_install_command(opts)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let working_dir = effective_working_dir(opts);

    let Output {
        status,
        stdout,
        stderr,
    } = cmd
        .output()
        .map_err(|e| format!("Failed to spawn embedded Bun runtime: {e}"))?;

    Ok(InstallerOutcome {
        success: status.success(),
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        working_dir,
    })
}

fn effective_working_dir(opts: &InstallerOptions) -> PathBuf {
    if let Some(dir) = opts.working_dir.as_ref() {
        return dir.clone();
    }
    match AgentRuntime::prepare() {
        Ok(rt) => rt.root().to_path_buf(),
        Err(_) => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn env_pairs_skips_unset_and_blank_fields() {
        let opts = InstallerOptions {
            port: None,
            app_name: Some("   ".to_string()),
            owner: Some("".to_string()),
            webhook_url: None,
            working_dir: None,
        };
        assert!(opts.env_pairs().is_empty());
    }

    #[test]
    fn env_pairs_emits_port_app_name_owner_and_webhook() {
        let opts = InstallerOptions {
            port: Some(4123),
            app_name: Some(" my-bot ".to_string()),
            owner: Some("my-org".to_string()),
            webhook_url: Some("https://example.com/hook".to_string()),
            working_dir: None,
        };

        let env: std::collections::HashMap<_, _> = opts.env_pairs().into_iter().collect();
        assert_eq!(env.get("PORT").map(String::as_str), Some("4123"));
        assert_eq!(env.get("APP_NAME").map(String::as_str), Some("my-bot"));
        assert_eq!(env.get("GITHUB_ORG").map(String::as_str), Some("my-org"));
        assert_eq!(
            env.get("WEBHOOK_URL").map(String::as_str),
            Some("https://example.com/hook")
        );
    }

    #[test]
    fn installer_entrypoint_lives_under_node_modules() {
        // Catches accidental renames of the entrypoint constant.
        assert!(INSTALLER_ENTRYPOINT.starts_with("node_modules/"));
        assert!(INSTALLER_ENTRYPOINT.ends_with(".ts"));
        assert!(INSTALLER_ENTRYPOINT.contains(INSTALLER_PACKAGE));
    }

    #[test]
    fn installer_script_path_joins_under_runtime_root() {
        let runtime = AgentRuntime::prepare().expect("agent runtime should prepare");
        let script = installer_script_path(&runtime);
        assert!(
            script.starts_with(runtime.root()),
            "script {} should sit under runtime root {}",
            script.display(),
            runtime.root().display()
        );
        assert!(
            script.ends_with(INSTALLER_ENTRYPOINT),
            "unexpected script suffix: {}",
            script.display()
        );
    }

    #[test]
    fn require_installer_script_succeeds_for_bundled_runtime() {
        // Sanity check that the npm dependency was actually installed; if
        // this fails, run `bun install` in crates/agent-runtime.
        let runtime = AgentRuntime::prepare().expect("agent runtime should prepare");
        let script = require_installer_script(&runtime).expect("entrypoint present");
        assert!(script.is_file());
    }

    #[test]
    fn build_install_command_targets_bun_and_passes_script() {
        let opts = InstallerOptions {
            port: Some(4500),
            ..InstallerOptions::default()
        };
        let cmd = build_install_command(&opts).expect("command");
        let program = Path::new(cmd.get_program()).to_path_buf();
        let program_str = program.to_string_lossy().to_lowercase();
        assert!(
            program_str.contains("bun") || program == Path::new("bun"),
            "expected bun-like program, got {}",
            program.display()
        );

        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args.len(),
            1,
            "expected exactly one positional arg: {args:?}"
        );
        assert!(
            args[0].ends_with("/caretta-github-app-installer/src/cli.ts"),
            "unexpected script arg: {}",
            args[0]
        );

        let envs: std::collections::HashMap<String, String> = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().into_owned(),
                    v?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert_eq!(envs.get("PORT").map(String::as_str), Some("4500"));
    }
}
