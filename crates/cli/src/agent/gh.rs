//! Shared wrapper around the `gh` GitHub CLI.
//!
//! All caretta code that shells out to `gh` should go through [`Gh`] so the
//! program name, default behaviour, and any cross-cutting concerns (logging,
//! redaction, future auth wiring) live in one place.

use crate::agent::cmd::{
    cmd_capture, cmd_run, cmd_run_env, cmd_stdout, cmd_stdout_or_die, die, has_command,
};

const GH: &str = "gh";

/// Namespace handle for `gh` CLI invocations.
pub struct Gh;

impl Gh {
    /// Whether the `gh` binary is reachable on `PATH`.
    pub fn is_installed() -> bool {
        has_command(GH)
    }

    /// Abort the process with `message` when `gh` is not on `PATH`.
    pub fn require_installed_or_die(message: &str) -> ! {
        die(message)
    }

    /// Run `gh <args>` and return trimmed stdout, or `None` on failure.
    pub fn stdout(args: &[&str]) -> Option<String> {
        cmd_stdout(GH, args)
    }

    /// Run `gh <args>` and return trimmed stdout, dying with `context` on
    /// failure.
    pub fn stdout_or_die(args: &[&str], context: &str) -> String {
        cmd_stdout_or_die(GH, args, context)
    }

    /// Run `gh <args>` and return `(success, combined_stdout_stderr)`.
    pub fn capture(args: &[&str]) -> (bool, String) {
        cmd_capture(GH, args)
    }

    /// Run `gh <args>` inheriting stdio. Returns success.
    pub fn run(args: &[&str]) -> bool {
        cmd_run(GH, args)
    }

    /// Run `gh <args>` with additional env vars, inheriting stdio. Returns
    /// success.
    pub fn run_env(args: &[&str], env: &[(String, String)]) -> bool {
        cmd_run_env(GH, args, env)
    }
}
