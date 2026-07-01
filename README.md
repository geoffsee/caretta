# caretta
Workflow-driven agents

- Desktop
- Web
- CLI
- GitHub Actions

<div style="text-align: center;">
  <img src="caretta.png" alt="caretta.png" style="max-width: 33%;" />
</div>

## Origins
Caretta takes its name from *Caretta caretta*, the loggerhead sea turtle: a global wanderer of the Atlantic, Pacific, Indian Ocean, and Mediterranean, not the creature of a single sea. A hatchling leaves its natal beach with no chart in any ordinary sense, yet the open ocean is not featureless to it. Loggerheads can use the Earth's magnetic field as both compass and map, reading regional signatures shaped by field intensity and inclination, and recent work suggests juveniles can learn the magnetic character of particular places.

Birds share a related gift, though their compass appears to be bound to a different sensorium: light-dependent chemistry in retinal cryptochromes, an eye-borne way of sensing magnetic direction. Same hidden coordinate system, different biological interface.

That was the image behind this project. A growing codebase is less a straight road than a shifting field of signatures: accounts, permissions, documentation, compliance, security, issues, and release pressure all bend the local field. caretta is built to help agents read those signals, orient themselves, and move work forward in the right sequence when no single pair of hands can carry the whole migration alone.

## Quickstart
```shell
$ cargo binstall caretta
$ caretta --help
```

## CLI examples

```shell
# Launch the desktop UI (default subcommand)
$ caretta

# Review every open PR in the current repo
$ caretta code-review

# Work a single issue end-to-end (drafts a branch + PR)
$ caretta issue 42

# Address review threads on a PR
$ caretta fix-pr 1337

# Continuously work issues from a tracker issue
$ caretta loop 7

# Sweep open issues, PRs, local branches, and tracker bodies
$ caretta housekeeping

# Refresh root-level project docs against the current state of the code
$ caretta refresh-docs

# Serve the web UI on http://localhost:8080 (override with --port)
$ caretta serve
$ caretta serve --port 3030

# Pick a different agent CLI on the fly
$ caretta --agent codex code-review
$ caretta --agent gemini issue 42

# List available workflow presets, or peek inside one
$ caretta presets
$ caretta presets xp

# Run a workflow under a different preset (overrides caretta.toml)
$ caretta --preset xp ideation
```

`--agent` accepts `claude`, `cline`, `codex`, `copilot`, `gemini`, `junie`, `xai`, `cursor` (default: `claude`). The matching CLI must be installed and authenticated. `--auto` passes adapter-specific flags that reduce permission prompts and, for two-phase workflows (ideation, housekeeping, sprint-planning, retrospective, etc.), synthesizes stand-in feedback so the draft chains straight into finalize without a human in the loop — without `--auto` the CLI stops after the draft so you can review before any side effects fire. `--dry-run` prints planned prompts and actions without making supported changes. `--preset <name>` swaps the workflow preset for a single invocation (use `caretta presets` to see what's available; `caretta presets <name>` lists the workflows that preset ships with).

## Desktop UI

The desktop app is split into a workflow sidebar and an editor panel. The sidebar
handles agent/model selection, local inference settings, workflow presets,
workflow actions, tracker issues, open issues, and open PRs. The editor panel has
tabs for:

- **Agent Output** — streamed assistant, tool, and log events.
- **Files** — files read or changed by the agent, plus a repository file browser.
- **Personas** — a User Personas Studio for creating, editing, deleting, and
  natural-language drafting persistent personas.
- **Security** — local security scan findings and JSON export.
- **Interview** — multi-round structured interview flows.
- **Chat** — free-form project chat with the selected agent.

Personas are stored as JSON files in the `personas/` directory beside the
resolved `user_personas` skill file. With automatic defaults, that directory is
**`.caretta/skills/user-personas/personas/`** when you keep forked skills there,
else **`assets/skills/user-personas/personas/`** when present (upstream Caretta’s
bundled layout), else beside the materialized `SKILL.md` under your OS app-data
directory — see **Skill paths** under Configuration below. If you override
`[skills].user_personas`, the Personas tab reads and writes next to that custom skill
path so UXR workflows and the studio share the same persona set.

## Configuration (`caretta.toml`)

caretta reads `caretta.toml` from the repo root on every launch (the legacy filename `dev.toml` is still honored as a fallback). Every field is optional — drop in only what you want to change. The full surface looks like this:

**Skill paths (defaults):** if you omit `[skills]`, caretta searches in order — repo-relative **`.caretta/skills/.../SKILL.md`** when present (**recommended overrides** for forks and application repos); else **`assets/skills/.../SKILL.md`** when present (**upstream Caretta’s embedded source tree**, unchanged here); otherwise **absolute** paths to the bundled copy materialized under the OS app-data directory — typically **`$HOME/Library/Application Support/caretta/skills`** on macOS, **`$XDG_DATA_HOME/caretta/skills`** (or **`~/.local/share/caretta/skills`**) on Linux — so workflows work with no copied skills at all. If both layouts exist in the checkout, **`.caretta/skills/` wins**. Explicit `[skills]` entries override everything.

```toml
# ── Top-level ─────────────────────────────────────────────────────────────
project_name           = "my-project"   # default: inferred from the repo dir
geodynamo_url          = "https://geoffsee.github.io/geodynamo/"
workflow_preset        = "default"      # default: "default"  (run `caretta presets`)
bootstrap_agent_files  = true           # default: true   — legacy agent-file bootstrap flag
bootstrap_snapshot     = false          # default: false  — opt-in to toak-rs codebase snapshot on launch
use_subscription       = false          # default: false  — billing hint for adapters that support it

# ── Per-agent default model ───────────────────────────────────────────────
# Keys match `--agent` values. Empty / missing = adapter default.
[agent_models]
claude  = "claude-opus-4-7"
codex   = "gpt-5-codex"
gemini  = "gemini-2.5-pro"
grok    = "grok-4"

# ── Local inference (OpenAI-compatible endpoint) ──────────────────────────
[local_inference]
advanced = false                          # show advanced fields in the GUI
preset   = "vllm"                         # vllm | lm_studio | ollama | custom
base_url = "http://localhost:8000/v1"     # filled from preset unless preset = "custom"
model    = "qwen2.5-coder-32b-instruct"
# api_key stored via `caretta`'s OS keychain; do not commit it.

# ── Skill files (optional — overrides automatic `.caretta/` then `assets/` resolution) ─
# [skills]
# user_personas  = ".caretta/skills/user-personas/SKILL.md"
# issue_tracking = ".caretta/skills/issue-tracking/SKILL.md"

# ── Bot identity for code review / approvals ──────────────────────────────
# mode = "disabled" | "token" | "github_app". Tokens / private keys are
# stored in the OS keychain via the GUI, not in this file.
[bot]
mode            = "github_app"
app_id          = "1234567"
installation_id = "12345678"

# ── Security scan target paths ────────────────────────────────────────────
# List the files this project considers security-relevant. The local scanner
# runs its checks (hardcoded-secret detection, weak-crypto markers, plaintext
# http:// URLs) against each declared path; repository-wide hygiene checks
# (.gitignore coverage, SECURITY.md presence) run regardless. Leaving `paths`
# empty surfaces a configuration warning rather than guessing.
[security_scan]
paths = [
    # "src/auth.rs",
    # "src/api/handlers.rs",
]

# ── Visual regression command ────────────────────────────────────────────
# `command` is run exactly as configured from the repo root. `base_url` and
# `screenshots_dir` are kept as Caretta context fields for continuity across
# workflows; they are not injected into the child process.
[visual_regression]
command = ["bun", "x", "playwright", "test", "tests/visual"]
base_url = "http://localhost:5173"
screenshots_dir = "tests/visual/screenshots"

# ── Deploy command ───────────────────────────────────────────────────────
# `command` is run exactly as configured from the repo root. `environment`
# and `url` are kept as Caretta context fields for continuity across workflows;
# they are not injected into the child process.
[deploy]
command = ["./scripts/deploy.sh"]
environment = "staging"
url = "https://staging.example.com"
```

`user_personas` also controls the Personas Studio storage location: persona JSON
documents live in a `personas/` directory beside the resolved `SKILL.md` (whether
that path is repo-relative, materialized under app data, or set explicitly in `[skills]`).

`geodynamo_url` is optional. It should point at the geodynamo GitHub Pages project
root; the autopilot action falls back to this value when its `geodynamo-url`
input is not set.

CLI flags (`--agent`, `--auto`, `--dry-run`, `--preset`) override matching `caretta.toml` values for that single invocation. Secrets — agent API keys, GitHub bot tokens, GitHub App private keys — are not written to `caretta.toml`; they're stored in the OS keychain by the GUI's settings panel or supplied via env vars (see the [GitHub Actions example](#github-actions) below).

## GitHub Actions
Every CLI subcommand above is also available as a GitHub Action — [**geoffsee/caretta-action**](https://github.com/geoffsee/caretta-action). Wire it to `pull_request`, `issues`, or `schedule` and your repo starts maintaining itself: issues become PRs, PRs get reviewed, review threads get addressed, weekly housekeeping happens on its own.

A working end-to-end demo lives at [**geoffsee/caretta-hello-world**](https://github.com/geoffsee/caretta-hello-world) — a tiny Node project where labeling an issue `agent:work` is enough to land a merged PR with no further input.

```yaml
- uses: geoffsee/caretta-action@main   # pin to a SHA or tag for production
  with:
    task: code-review
    agent: claude
  env:
    # ── Agent auth (pick the ones that match your `agent:` choice) ──
    CLAUDE_CODE_OAUTH_TOKEN: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}   # claude (preferred)
    # ANTHROPIC_API_KEY:     ${{ secrets.ANTHROPIC_API_KEY }}         # claude (alternative)
    # OPENAI_API_KEY:        ${{ secrets.OPENAI_API_KEY }}            # codex
    # GEMINI_API_KEY:        ${{ secrets.GEMINI_API_KEY }}            # gemini
    # XAI_API_KEY:           ${{ secrets.XAI_API_KEY }}               # xai
    # (cline, copilot, junie, cursor authenticate via their own CLI login flow)

    # ── GitHub auth for the `gh` CLI caretta shells out to ──
    GH_TOKEN: ${{ secrets.CARETTA_PAT || github.token }}              # PAT preferred so PRs trigger downstream workflows

    # ── Bot identity (so reviews/approvals don't run as the PR author) ──
    # Pick ONE of the three styles below.
    #
    # 1. Direct token:
    # DEV_BOT_TOKEN:           ${{ secrets.DEV_BOT_TOKEN }}
    #
    # 2. Token from a file:
    # DEV_BOT_TOKEN_PATH:      /path/to/token-file
    #
    # 3. GitHub App (mints installation tokens at runtime):
    DEV_BOT_APP_ID:          ${{ secrets.DEV_BOT_APP_ID }}
    DEV_BOT_INSTALLATION_ID: ${{ secrets.DEV_BOT_INSTALLATION_ID }}
    # DEV_BOT_PRIVATE_KEY is the *path* to a PEM. A prior step base64-decodes
    # secrets.DEV_BOT_PRIVATE_KEY_B64 into $RUNNER_TEMP/dev-bot.pem and exports it.

    # ── caretta knobs ──
    # DEV_PROJECT_NAME: my-project   # override project name (otherwise inferred from the repo)
    # DISABLE_TOAK: "1"              # skip the toak-rs bootstrap snapshot (faster, less context)

    # ── Diagnostics ──
    # RUST_LOG: info                 # the action defaults to info; bump to debug/trace if you need more
```

The full hands-off setup (PAT, OAuth token, GitHub App credentials, branch protection) is documented step-by-step in the [caretta-hello-world README](https://github.com/geoffsee/caretta-hello-world#setup).

## Status: Unstable (Active Development)
Expect unexpected breaking changes.

## Docs
- [Getting Started](docs/getting_started.md) — Installation, prerequisites, Desktop vs Web App, Personas Studio, and CLI usage.
- [Workflow & Lifecycle](docs/workflow.md) — How an AI dev agent cycle works (Ideation to Retrospective), including Documentation Refresh actions.
- [Configuration & Setup](docs/configuration.md) — CLI options, bot account setup for Code Review, supported agents, and general tips.
- [Architecture](docs/architecture.md) — Project structure and internals.

## License & Revenue Sharing
caretta is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE); individuals, small teams, and organizations under the revenue threshold use it under those terms with no further obligation. Organizations (government or non-government) with **US $500,000+** annual revenue that put a significant portion of the code or product into production are asked to make contact first and share fairly — see [`REVENUE-SHARING.md`](REVENUE-SHARING.md).

## Contributing
- Open Issue -> Fork Repo -> Create Pull Request

## Contact
- File an issue for any questions or feedback.
