# Getting Started

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/geoffsee/caretta/master/install.sh | bash
```

This detects your OS and architecture, downloads the latest release binary, and installs it to `~/.local/bin`. Override the install directory with `CARETTA_INSTALL_DIR`:

```sh
CARETTA_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/geoffsee/caretta/master/install.sh | bash
```

Pre-built binaries are available for:
- Linux x86_64 / aarch64
- macOS aarch64 (Apple Silicon)
- Windows x86_64

### Prerequisites

- `gh` CLI authenticated (`gh auth login`)
- An AI agent on PATH (`claude`, `cline`, `codex`, `copilot`, `cursor`, `gemini`, `grok`, `junie`, or `xai`)

### Code review and a bot GitHub App

**Code Review** submits PR reviews through the GitHub API (`POST /repos/{owner}/{repo}/pulls/{n}/reviews`). GitHub will not let you approve your own pull requests, so reviews need to come from a **separate bot identity**. In CI, the default `GITHUB_TOKEN` is also a poor fit for posting reviews as a distinct bot. caretta addresses this by minting **installation tokens** for a **GitHub App** you create (recommended), or by using a **second account / PAT** as an alternative.

Set **`DEV_BOT_APP_ID`**, **`DEV_BOT_INSTALLATION_ID`**, and **`DEV_BOT_PRIVATE_KEY`** (or the PAT variables) before running Code Review, or configure the same credentials in the desktop app’s **Configuration** panel (secrets go to the OS keychain, not `caretta.toml`). Step-by-step instructions, permissions, and troubleshooting are in **[Configuration & Setup — Bot account setup](configuration.md#bot-account-setup-code-review)**. For GitHub Actions, see [caretta-action setup](../packages/action/docs/SETUP.md) or [autopilot-action setup](../packages/autopilot-action/docs/SETUP.md).

## Quick Start

You can choose between a native Desktop App, a Web Server App, or use the Command Line Interface.

### Desktop App

To launch the native desktop GUI:

```sh
caretta gui
```

*(Note: `caretta` defaults to `gui` if no subcommand is provided).*

The desktop UI has a sidebar for configuration, workflow actions, tracker
issues, open issues, and PRs. The editor panel includes tabs for agent output,
file browsing, security findings, interviews, chat, and **Personas**. Use the
Personas tab to create, edit, delete, or generate persistent user personas from
natural-language notes. Persona JSON files are saved beside the configured
`user_personas` skill file, so UXR workflows consume the same persona set you
manage in the UI.

### Web App

If you've installed `caretta` locally, you can serve the embedded Dioxus web app over HTTP. The required web assets are bundled inside the native binary.

```sh
caretta serve
```

By default, it will be available at `http://127.0.0.1:8080`. You can specify a custom port:

```sh
caretta serve --port 3000
```

### Development Scripts (Web vs Desktop Mode)

If you're working directly from the repository source code, `scripts/agent.sh` provides a convenient launch wrapper:

- **Web Mode (Default):** Run the script without any arguments to compile and serve the web app dynamically via the `dx` CLI.
  ```bash
  ./scripts/agent.sh
  ```

- **Desktop Mode:** Pass the `--desktop` flag to compile and launch the native desktop GUI via Cargo.
  ```bash
  ./scripts/agent.sh --desktop
  ```

### CLI Usage

Everything runs from the sidebar in the GUI, but you can also invoke specific workflows or actions from the CLI:

```sh
# Run sprint planning draft
caretta sprint-planning

# Fix review comments on a PR
caretta fix-pr 123

# Run a specific issue
caretta issue 45

# List presets or inspect one preset's workflows
caretta presets
caretta presets ux
```
