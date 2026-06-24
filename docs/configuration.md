# Configuration & Setup

## Options

```sh
caretta [OPTIONS] [COMMAND]
```

| Flag | Description | Default |
|---|---|---|
| `--agent <name>` | AI agent (`claude`, `cline`, `codex`, `copilot`, `cursor`, `gemini`, `junie`, `xai`) | `claude` |
| `--auto` | Unattended mode (skip permission prompts) | off |
| `--dry-run` | Show what would happen without executing | off |
| `--preset <name>` | Use a workflow preset for this invocation | `caretta.toml` / `default` |
| `--create-labels` | Write the bundled label taxonomy to `.github/labels.yml` and exit | — |

## Tips

- Use **dry-run** first to preview what any action will do before committing to it.
- The **Follow** checkbox in the editor tab auto-scrolls as events stream in. Uncheck it to scroll back through history.
- **Expand All** opens collapsed thinking and tool-result blocks in the event stream.
- Use **Stop** in the Actions panel to request cancellation of the current run. Active agent subprocesses are terminated, and the loop exits cleanly.
- Use the **Personas** tab to maintain persistent UXR personas. It reads and writes JSON files in the `personas/` directory beside the configured `[skills].user_personas` path.
- Switch themes from the title bar dropdown. 10 built-in: Tokyo Night, Catppuccin Mocha, Dracula, Nord, Gruvbox Dark, Solarized Dark, One Dark Pro, Rose Pine, Synthwave '84, GitHub Dark.

## Bot Account Setup (Code Review)

The **Code Review** action posts reviews through `gh api` against
`POST /repos/{owner}/{repo}/pulls/{n}/reviews` so verdicts and line-anchored
comments are submitted atomically. GitHub forbids approving your own PRs, so a
separate bot identity is required. Without it, the Code Review button is
disabled.

### Option A — GitHub App (recommended)

#### Quick start — `scripts/create-github-app.ts`

A helper script automates the [GitHub App Manifest Flow](https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest): it serves a local form, redirects to GitHub for app registration, then exchanges the returned code for credentials and writes the PEM to `~/.config/caretta/dev-ui-bot.pem` plus a `.env.github-app` containing `DEV_BOT_APP_ID` and `DEV_BOT_PRIVATE_KEY`.

```sh
# Personal account
bun scripts/create-github-app.ts

# Org-owned app
GITHUB_ORG=my-org APP_NAME=my-caretta-bot bun scripts/create-github-app.ts
```

After the app is created, install it on your repo (`<html_url>/installations/new`), copy the installation ID from the resulting URL into `DEV_BOT_INSTALLATION_ID` in `.env.github-app`, then `source .env.github-app && caretta`.

#### Manual setup

1. **Create a private GitHub App** in your user/org settings:
   - **Repository permissions**: Contents (read), Pull requests (read & write), Issues (read & write), Metadata (read).
   - No webhook URL or events required.
2. **Install the app** on the target repository.
3. Note the **App ID** and **Installation ID** (visible in the app's settings page under "Installations").
4. **Generate a private key** (PEM) from the app settings and save it:
   ```sh
   mkdir -p ~/.config/caretta
   mv ~/Downloads/<app-name>.pem ~/.config/caretta/dev-ui-bot.pem
   chmod 600 ~/.config/caretta/dev-ui-bot.pem
   ```
5. **Set environment variables** before launching the dev agent:
   ```sh
   export DEV_BOT_APP_ID="123456"
   export DEV_BOT_INSTALLATION_ID="78901234"
   export DEV_BOT_PRIVATE_KEY="$HOME/.config/caretta/dev-ui-bot.pem"
   caretta
   ```

caretta mints short-lived installation tokens on demand (cached for 50 minutes)
and injects `GH_TOKEN` into the review subprocess. Audit logs show the GitHub App
bot identity, such as `dev-ui-bot[bot]`.

You can also configure review-bot access in the GUI under `Configuration` and
click `Save Configuration`. Non-secret settings are written to `caretta.toml`
(legacy `dev.toml` is still read on startup as a fallback); stored GitHub
tokens, GitHub App PEM keys, and local inference API keys go into the OS
credential vault instead of plaintext project files.

### Option B — Personal access token (second user)

1. Create a second GitHub user (e.g. `<owner>-bot`), grant write access to the repo.
2. Generate a **fine-grained PAT** with Pull requests (read & write) and Issues (read & write) scopes.
3. Set the token directly:
   ```sh
   export DEV_BOT_TOKEN="github_pat_..."
   caretta
   ```
   Or store it in a file and point to it:
   ```sh
   echo "github_pat_..." > ~/.config/caretta/bot-token
   chmod 600 ~/.config/caretta/bot-token
   export DEV_BOT_TOKEN_PATH="$HOME/.config/caretta/bot-token"
   caretta
   ```

### Environment Variables

| Variable | Description | Required |
|---|---|---|
| `DEV_BOT_TOKEN` | Direct token (PAT or pre-minted installation token) | One of these |
| `DEV_BOT_TOKEN_PATH` | Path to a file containing the token | must be set |
| `DEV_BOT_APP_ID` | GitHub App ID | Required for |
| `DEV_BOT_INSTALLATION_ID` | Installation ID for the app on this repo | GitHub App mode |
| `DEV_BOT_PRIVATE_KEY` | Path to the App's PEM private key (default: `~/.config/caretta/dev-ui-bot.pem`) | Optional |

## Supported Agents

| Agent | Binary | Auto flag | Event streaming | Notes |
|---|---|---|---|---|
| Claude | `claude` | `--dangerously-skip-permissions` | stream-json | Default. Full structured event streaming to the UI. |
| Cline | `cline` | `--no-interactive` | plain | Multi-provider agent. Configure provider with `cline auth`. |
| Gemini | `agy` | `--yolo` | stream-json | Antigravity CLI (replaces deprecated `gemini` npm CLI). |
| xAI | `grok` | `--always-approve` | streaming-json | Official Grok Build CLI. Uses `XAI_API_KEY`. |
| Junie | `junie` | `--brave` | json-stream | JetBrains Junie CLI. BYOK via `--provider` + API key flags. |
| Codex | `codex` | `--dangerously-bypass-approvals-and-sandbox` | JSONL (`exec --json`) | Streams assistant/tool/result events into the same UI timeline. |
| Copilot | `copilot` | `--yolo` | unknown | GitHub Copilot CLI (standalone binary, not `gh copilot`). |
| Cursor | `cursor` | `--yolo` | stream-json | Cursor Agent CLI. Supports `--model` and non-interactive `-p`. |
