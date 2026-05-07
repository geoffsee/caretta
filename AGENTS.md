# Repository Guidelines

## Project Structure & Module Organization
The project is organized into a Cargo workspace under `crates/`.
- `crates/cli/` (package `freq-ai`) contains the main Rust application code.
- `crates/agent-common` defines the shared `AgentCliAdapter` trait; `crates/cli/src/agent/adapter_dispatch.rs` maps `cli_common::Agent` to each provider implementation.
- `crates/claude/`, `crates/cline/`, etc. are adapters for their respective CLIs.
Main application logic is in `crates/cli/src/`. `crates/cli/src/main.rs` wires the CLI binary, `crates/cli/src/lib.rs` exposes shared library code, `crates/cli/src/agent/` holds agent execution and workflow logic, and `crates/cli/src/ui/` contains the Dioxus desktop UI. Bundled prompts, workflows, and skill files live under `assets/`. Utility scripts such as [scripts/smoke-test.sh](/Users/williamseemueller/workspace/freq-ai/scripts/smoke-test.sh) and [scripts/setup-hooks.sh](/Users/williamseemueller/workspace/freq-ai/scripts/setup-hooks.sh) support release and local verification.

## Build, Test, and Development Commands
Use `cargo run -- gui` to launch the desktop app and `cargo run -- --help` to inspect CLI entry points. Run `cargo build` for a debug build and `cargo build --release --features bundle-runtime` for a self-contained release binary (the `bundle-runtime` feature embeds the agent CLI runtime archive into the binary; it is intentionally off by default to keep dev builds fast — without it, the agent runtime is loaded from `crates/agent-runtime/node_modules` at runtime). Use `cargo test --workspace` for the full test suite and `cargo test agent::tracker::tests::` for targeted iteration. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` before opening a PR. `./scripts/setup-hooks.sh` installs the same checks as local git hooks.

## Coding Style & Naming Conventions
This repository uses Rust 2024 edition defaults: 4-space indentation, `snake_case` for functions/modules, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Keep modules focused by domain (`agent`, `ui`, `custom_themes`) and prefer small helper functions over large mixed-responsibility blocks. Format with `cargo fmt`; treat clippy warnings as errors.

## Testing Guidelines
Tests are inline unit tests placed next to the code they verify under `#[cfg(test)]`. Follow existing descriptive snake_case names such as `refresh_docs_prompt_limits_scope_and_requires_summary_block`. Add tests for parser changes, workflow prompt generation, and command construction paths. For release smoke coverage, run `./scripts/smoke-test.sh` when touching install or CLI behavior.

## Commit & Pull Request Guidelines
Recent commits use short imperative subjects like `add support for xAI agent` and `refactor asset paths`. Keep commits focused and use the body when behavior or migration context is not obvious. PRs should state the user-visible change, list verification commands run, and link the relevant issue or tracker item. Include screenshots or short recordings for UI changes in `src/ui/`.

## Security & Configuration Tips
Development assumes `gh auth login` has been completed and at least one supported agent CLI is available on `PATH`. Keep secrets out of committed config; prefer environment variables and the OS keyring-backed paths already used by the app.
