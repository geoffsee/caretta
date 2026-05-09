use agent_common::{AgentCliAdapter, AgentCliCommand, AgentInvocation};
use cli_common::Agent;
#[cfg(feature = "bundle-runtime")]
use flate2::read::GzDecoder;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
#[cfg(feature = "bundle-runtime")]
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(feature = "bundle-runtime")]
use tar::Archive;

mod available_models;
mod bundled_agents;
mod utilities;

pub use available_models::{
    CliScan, CuratedModel, CuratedModels, RawScanResult, curate_all, raw_scan_to_json,
    scan_all_clis, scan_available_models, scan_cli,
};
pub use bundled_agents::{BundledAgent, SUPPORTED_AGENTS};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/agent_runtime_generated.rs"));
}

pub use generated::{ARCHIVE_NAME, ARCHIVE_SHA256, ARCHIVE_SHORT_SHA256, TARGET_ARCH, TARGET_OS};

#[derive(Debug, Clone)]
pub struct AgentRuntime {
    root: PathBuf,
}

impl AgentRuntime {
    pub fn prepare() -> io::Result<Self> {
        Self::prepare_at(default_runtime_root())
    }

    #[cfg(feature = "bundle-runtime")]
    pub fn prepare_at(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let marker = root.join(".freq-ai-agent-runtime");
        if marker_contains_current_archive(&marker) {
            return Ok(Self { root });
        }

        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;

        let decoder = GzDecoder::new(Cursor::new(generated::ARCHIVE_BYTES));
        Archive::new(decoder).unpack(&root)?;
        ensure_entrypoints_executable(&root);
        fs::write(marker, ARCHIVE_SHA256)?;

        Ok(Self { root })
    }

    /// Without the `bundle-runtime` feature there is no embedded archive to
    /// unpack. We mount the runtime directly on the crate's source tree where
    /// `node_modules` already lives, and resolve `bun`/`node` from the
    /// build-time-resolved Bun binary. The `_root` argument is intentionally
    /// ignored so callers (including tests using a tempdir) keep working.
    #[cfg(not(feature = "bundle-runtime"))]
    pub fn prepare_at(_root: impl AsRef<Path>) -> io::Result<Self> {
        let root = PathBuf::from(generated::MANIFEST_DIR);
        ensure_entrypoints_executable(&root);
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("node_modules").join(".bin")
    }

    pub fn runtime_bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn bun_path(&self) -> Option<PathBuf> {
        self.runtime_binary_path("bun")
    }

    pub fn node_path(&self) -> Option<PathBuf> {
        self.runtime_binary_path("node")
    }

    pub fn binary_path_for_agent(&self, agent: Agent) -> Option<PathBuf> {
        self.binary_path(agent.binary())
    }

    pub fn binary_path_for_adapter(&self, adapter: &impl AgentCliAdapter) -> Option<PathBuf> {
        self.binary_path(adapter.binary())
    }

    pub fn binary_path(&self, binary: &str) -> Option<PathBuf> {
        if let Some(agent) = bundled_agent_by_binary(binary)
            && let Some(entrypoint) = agent.entrypoint
        {
            let path = self.root.join(entrypoint);
            if path.is_file() {
                return Some(path);
            }
        }

        executable_candidates(binary)
            .into_iter()
            .map(|candidate| self.bin_dir().join(candidate))
            .find(|path| path.is_file())
            .map(resolve_executable_path)
    }

    pub fn runtime_binary_path(&self, binary: &str) -> Option<PathBuf> {
        if let Some(found) = executable_candidates(binary)
            .into_iter()
            .map(|candidate| self.runtime_bin_dir().join(candidate))
            .find(|path| path.is_file())
            .map(resolve_executable_path)
        {
            return Some(found);
        }

        // In non-bundled (dev) builds there is no `bin/` overlay because we
        // never unpacked an archive. Fall back to the Bun binary that the
        // build script resolved at compile time; both `bun` and `node` are
        // served by the same executable (Bun ships a `node`-compatible mode).
        #[cfg(not(feature = "bundle-runtime"))]
        if matches!(binary, "bun" | "node") {
            let bun = PathBuf::from(generated::BUN_PATH);
            if bun.is_file() {
                return Some(resolve_executable_path(bun));
            }
        }

        None
    }

    pub fn command_for_agent(&self, agent: Agent) -> Command {
        self.command_for_binary(agent.binary())
    }

    pub fn command_for_adapter(&self, adapter: &impl AgentCliAdapter) -> Command {
        self.command_for_binary(adapter.binary())
    }

    pub fn command_for_adapter_invocation(
        &self,
        adapter: &impl AgentCliAdapter,
        invocation: AgentInvocation,
    ) -> Option<Command> {
        let cli_command = adapter.command_for(invocation)?;
        Some(self.command_for_cli_command(&cli_command))
    }

    pub fn command_for_cli_command(&self, command: &AgentCliCommand) -> Command {
        let mut process = self.command_for_binary(&command.binary);
        process.args(&command.args);
        process
    }

    pub fn command_for_binary(&self, binary: &str) -> Command {
        let mut command = if binary == "claude" {
            self.command_for_claude_wrapper().unwrap_or_else(|| {
                Command::new(
                    self.binary_path(binary)
                        .unwrap_or_else(|| PathBuf::from(binary)),
                )
            })
        } else {
            Command::new(
                self.binary_path(binary)
                    .unwrap_or_else(|| PathBuf::from(binary)),
            )
        };
        command.env(
            "PATH",
            runtime_path([self.runtime_bin_dir(), self.bin_dir()]),
        );
        command
    }

    fn command_for_claude_wrapper(&self) -> Option<Command> {
        let wrapper = self
            .root
            .join("node_modules/@anthropic-ai/claude-code/cli-wrapper.cjs");
        if !wrapper.is_file() {
            return None;
        }

        let mut command = Command::new(self.node_path().unwrap_or_else(|| PathBuf::from("node")));
        command.arg(wrapper);
        Some(command)
    }
}

pub fn default_runtime_root() -> PathBuf {
    let override_dir = env::var_os("FREQ_AI_AGENT_RUNTIME_DIR").map(PathBuf::from);
    override_dir.unwrap_or_else(|| {
        env::temp_dir()
            .join("freq-ai")
            .join("agent-runtime")
            .join(format!(
                "{}-{}-{}",
                TARGET_OS, TARGET_ARCH, ARCHIVE_SHORT_SHA256
            ))
    })
}

pub fn agent_metadata(agent: Agent) -> Option<BundledAgent> {
    let id = agent.to_string();
    SUPPORTED_AGENTS.iter().copied().find(|a| a.id == id)
}

pub fn bundled_agent_by_binary(binary: &str) -> Option<BundledAgent> {
    SUPPORTED_AGENTS
        .iter()
        .copied()
        .find(|a| !a.external && a.binary == binary)
}

#[cfg(feature = "bundle-runtime")]
fn marker_contains_current_archive(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|contents| contents.trim() == ARCHIVE_SHA256)
        .unwrap_or(false)
}

fn resolve_executable_path(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

/// Ensure each known agent entrypoint under `root` has the executable bit set.
///
/// Some packages (notably `@anthropic-ai/claude-code`) only `chmod 0o755` the
/// entrypoint as part of a postinstall script that may not run under Bun in
/// every CI environment, leaving the file present but non-executable. This
/// guard restores +x on Unix so spawning the binary doesn't fail with EACCES.
fn ensure_entrypoints_executable(root: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for agent in SUPPORTED_AGENTS {
            let Some(entrypoint) = agent.entrypoint else {
                continue;
            };
            let path = root.join(entrypoint);
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            let mut perms = metadata.permissions();
            let mode = perms.mode();
            if mode & 0o111 != 0o111 {
                perms.set_mode(mode | 0o755);
                let _ = fs::set_permissions(&path, perms);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = root;
    }
}

fn runtime_path(paths: impl IntoIterator<Item = PathBuf>) -> OsString {
    let existing = env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = paths.into_iter().collect();
    parts.extend(env::split_paths(&existing));
    env::join_paths(parts).unwrap_or(existing)
}

#[cfg(windows)]
fn executable_candidates(binary: &str) -> Vec<String> {
    vec![
        binary.to_string(),
        format!("{binary}.exe"),
        format!("{binary}.cmd"),
        format!("{binary}.ps1"),
    ]
}

#[cfg(not(windows))]
fn executable_candidates(binary: &str) -> Vec<String> {
    vec![binary.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_common::{AgentCliAdapter, AgentInvocation};
    use claude::ClaudeWrapper;
    use cline::ClineWrapper;
    use codex::CodexWrapper;
    use copilot::CopilotWrapper;
    use gemini::GeminiWrapper;
    use grok::GrokWrapper;
    use junie::JunieWrapper;
    use std::process::Stdio;
    use xai::XaiWrapper;

    #[test]
    fn metadata_matches_xai_copilot_proxy() {
        let xai = agent_metadata(Agent::Xai).expect("xai metadata");
        assert_eq!(xai.binary, "copilot");
        assert_eq!(Agent::Xai.binary(), "copilot");
    }

    #[test]
    fn default_root_is_scoped_by_platform_and_archive_hash() {
        let root = default_runtime_root();
        let root = root.to_string_lossy();
        assert!(root.contains(TARGET_OS));
        assert!(root.contains(TARGET_ARCH));
        assert!(root.contains(ARCHIVE_SHORT_SHA256));
    }

    #[test]
    fn command_path_prefers_runtime_and_agent_bins() {
        let runtime = AgentRuntime {
            root: PathBuf::from("/tmp/freq-ai-agent-runtime-test"),
        };
        let path = runtime_path([runtime.runtime_bin_dir(), runtime.bin_dir()]);
        let paths: Vec<PathBuf> = env::split_paths(&path).collect();
        assert_eq!(paths[0], runtime.runtime_bin_dir());
        assert_eq!(paths[1], runtime.bin_dir());
    }

    #[test]
    fn claude_command_uses_wrapper_instead_of_direct_native_binary() {
        let tempdir = tempfile::tempdir().expect("create temp runtime dir");
        let root = tempdir.path();
        let wrapper = root.join("node_modules/@anthropic-ai/claude-code/cli-wrapper.cjs");
        fs::create_dir_all(wrapper.parent().unwrap()).expect("create wrapper parent");
        fs::write(&wrapper, "").expect("write wrapper");
        let node = root.join("bin/node");
        fs::create_dir_all(node.parent().unwrap()).expect("create runtime bin");
        fs::write(&node, "").expect("write node");

        let runtime = AgentRuntime {
            root: root.to_path_buf(),
        };
        let command = runtime.command_for_binary("claude");

        let expected_node = fs::canonicalize(&node).unwrap_or(node);
        assert_eq!(command.get_program(), expected_node.as_os_str());
        let args = command
            .get_args()
            .map(PathBuf::from)
            .collect::<Vec<PathBuf>>();
        assert_eq!(args, vec![wrapper]);
    }

    #[test]
    fn bundled_runtime_runs_provider_version_commands() {
        let tempdir = tempfile::tempdir().expect("create temp runtime dir");
        let runtime = AgentRuntime::prepare_at(tempdir.path()).expect("prepare runtime");

        assert_version_command(&runtime, ClaudeWrapper);
        assert_version_command(&runtime, ClineWrapper);
        assert_version_command(&runtime, CodexWrapper);
        assert_version_command(&runtime, CopilotWrapper);
        assert_version_command(&runtime, GeminiWrapper);
        assert_version_command(&runtime, GrokWrapper);
        assert_version_command(&runtime, JunieWrapper);
        assert_version_command(&runtime, XaiWrapper);
    }

    fn assert_version_command(runtime: &AgentRuntime, adapter: impl AgentCliAdapter) {
        let mut command = runtime
            .command_for_adapter_invocation(&adapter, AgentInvocation::Version)
            .expect("adapter should support version invocation");
        command.current_dir(runtime.root());
        let version_output = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|err| panic!("failed to spawn `{}`: {err}", adapter.binary()));

        if version_output.status.success() {
            return;
        }

        let mut help_command = runtime
            .command_for_adapter_invocation(&adapter, AgentInvocation::Help)
            .expect("adapter should support help invocation");
        help_command.current_dir(runtime.root());
        let help_output = help_command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|err| panic!("failed to spawn `{}` help: {err}", adapter.binary()));

        if is_env_dependent_cli_failure(&version_output.stderr)
            || is_env_dependent_cli_failure(&help_output.stderr)
        {
            return;
        }

        assert!(
            help_output.status.success(),
            "`{}` version and help commands failed\nversion stdout:\n{}\nversion stderr:\n{}\nhelp stdout:\n{}\nhelp stderr:\n{}",
            adapter.binary(),
            String::from_utf8_lossy(&version_output.stdout),
            String::from_utf8_lossy(&version_output.stderr),
            String::from_utf8_lossy(&help_output.stdout),
            String::from_utf8_lossy(&help_output.stderr),
        );
    }

    fn is_env_dependent_cli_failure(stderr: &[u8]) -> bool {
        let stderr = String::from_utf8_lossy(stderr);
        let stderr = stderr.to_ascii_lowercase();
        stderr.contains("api key required")
            || stderr.contains("set grok_api_key")
            || stderr.contains("secitemcopymatching failed")
    }
}
