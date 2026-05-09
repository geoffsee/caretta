# freq-ai
Workflow-driven agents

- Desktop
- Web
- CLI
- GitHub Actions

<img src="freq-ai.png" alt="freq-ai.png" style="max-width: 33%;" />

## Origins
It was upon an evening not unlike any other that the toils of my labour grew so weighty in the chambers of my mind as to brook no further dismissal. I had set myself to the construction of a cloud of mine own devising, a work, I confess, of no small ambition, and found that its foundations demanded a most particular order of laying: first the accounts of users, then the permissions that govern them, then the documentation by which they are made intelligible, then the compliance by which they are made lawful, and at the last the security by which the whole is preserved from ruin. Each stone, you see, rested upon the one before it, and to misplace a single course was to invite the slow and silent decay of the edifice entire. The labour was not, in truth, beyond the compass of a single mind; yet it lay manifestly beyond the reach of a single pair of hands. And so, after the manner of those who, finding themselves outnumbered by their own undertakings, resolve to multiply their instruments rather than their hours, I set about the fashioning of further hands, and of a cycle by which they might turn in concert.

## Quickstart
```shell
$ cargo binstall freq-ai
$ freq-ai --help
```

## CLI examples

```shell
# Launch the desktop UI (default subcommand)
$ freq-ai

# Review every open PR in the current repo
$ freq-ai code-review

# Work a single issue end-to-end (drafts a branch + PR)
$ freq-ai issue 42

# Address review threads on a PR
$ freq-ai fix-pr 1337

# Continuously work issues from a tracker issue
$ freq-ai loop 7

# Sweep open issues, PRs, local branches, tracker bodies, STATUS.md, and ISSUES.md
$ freq-ai housekeeping

# Refresh root-level project docs against the current state of the code
$ freq-ai refresh-docs

# Serve the web UI on http://localhost:8080 (override with --port)
$ freq-ai serve
$ freq-ai serve --port 3030

# Pick a different agent CLI on the fly
$ freq-ai --agent codex code-review
$ freq-ai --agent gemini issue 42

# List available workflow presets, or peek inside one
$ freq-ai presets
$ freq-ai presets xp

# Run a workflow under a different preset (overrides freq-ai.toml)
$ freq-ai --preset xp ideation
```

`--agent` accepts `claude`, `cline`, `codex`, `copilot`, `gemini`, `grok`, `junie`, `xai`, `cursor` (default: `claude`). The matching CLI must be installed and authenticated. `--auto` passes adapter-specific flags that reduce permission prompts and, for two-phase workflows (ideation, housekeeping, sprint-planning, retrospective, etc.), synthesizes stand-in feedback so the draft chains straight into finalize without a human in the loop — without `--auto` the CLI stops after the draft so you can review before any side effects fire. `--dry-run` prints planned prompts and actions without making supported changes. `--preset <name>` swaps the workflow preset for a single invocation (use `freq-ai presets` to see what's available; `freq-ai presets <name>` lists the workflows that preset ships with).

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
configured `user_personas` skill file. With the default config, that is
`assets/skills/user-personas/personas/`. If you override `[skills].user_personas`,
the Personas tab reads and writes next to that custom skill path so UXR workflows
and the studio share the same persona set.

## Configuration (`freq-ai.toml`)

freq-ai reads `freq-ai.toml` from the repo root on every launch (the legacy filename `dev.toml` is still honored as a fallback). Every field is optional — drop in only what you want to change. The full surface looks like this:

```toml
# ── Top-level ─────────────────────────────────────────────────────────────
project_name           = "my-project"   # default: inferred from the repo dir
workflow_preset        = "default"      # default: "default"  (run `freq-ai presets`)
bootstrap_agent_files  = true           # default: true   — legacy agent-file bootstrap flag
bootstrap_snapshot     = false          # default: false  — opt-in toak-rs codebase snapshot on launch
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
# api_key stored via `freq-ai`'s OS keychain; do not commit it.

# ── Skill files (override bundled paths) ──────────────────────────────────
[skills]
user_personas  = "skills/user-personas/SKILL.md"
issue_tracking = "skills/issue-tracking/SKILL.md"

# ── Bot identity for code review / approvals ──────────────────────────────
# mode = "disabled" | "token" | "github_app". Tokens / private keys are
# stored in the OS keychain via the GUI, not in this file.
[bot]
mode            = "github_app"
app_id          = "1234567"
installation_id = "12345678"

# ── Security scan target paths ────────────────────────────────────────────
# Defaults assume the original freq-cloud crate layout; override if your
# project doesn't have those crates.
[security_scan]
edge           = "crates/edge-node/src/lib.rs"
network        = "crates/network-node/src/lib.rs"
network_kem    = "crates/network-node/src/kem.rs"
network_crypto = "crates/network-node/src/crypto.rs"
service        = "crates/service-node/src/lib.rs"
gateway        = "crates/gateway-node/src/lib.rs"
gateway_users  = "crates/gateway-node/src/users.rs"
gateway_kms    = "crates/gateway-node/src/kms.rs"
cli_build      = "crates/freq-cli/src/build.rs"
compute        = "crates/compute-node/src/lib.rs"
```

`user_personas` also controls the Personas Studio storage location: persona JSON
documents live in a `personas/` directory beside that `SKILL.md`.

CLI flags (`--agent`, `--auto`, `--dry-run`, `--preset`) override matching `freq-ai.toml` values for that single invocation. Secrets — agent API keys, GitHub bot tokens, GitHub App private keys — are not written to `freq-ai.toml`; they're stored in the OS keychain by the GUI's settings panel or supplied via env vars (see the [GitHub Actions example](#github-actions) below).

## GitHub Actions
Every CLI subcommand above is also available as a GitHub Action — [**geoffsee/freq-ai-action**](https://github.com/geoffsee/freq-ai-action). Wire it to `pull_request`, `issues`, or `schedule` and your repo starts maintaining itself: issues become PRs, PRs get reviewed, review threads get addressed, weekly housekeeping happens on its own.

A working end-to-end demo lives at [**geoffsee/freq-ai-hello-world**](https://github.com/geoffsee/freq-ai-hello-world) — a tiny Node project where labeling an issue `agent:work` is enough to land a merged PR with no further input.

```yaml
- uses: geoffsee/freq-ai-action@main   # pin to a SHA or tag for production
  with:
    task: code-review
    agent: claude
  env:
    # ── Agent auth (pick the ones that match your `agent:` choice) ──
    CLAUDE_CODE_OAUTH_TOKEN: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}   # claude (preferred)
    # ANTHROPIC_API_KEY:     ${{ secrets.ANTHROPIC_API_KEY }}         # claude (alternative)
    # OPENAI_API_KEY:        ${{ secrets.OPENAI_API_KEY }}            # codex
    # GEMINI_API_KEY:        ${{ secrets.GEMINI_API_KEY }}            # gemini
    # XAI_API_KEY:           ${{ secrets.XAI_API_KEY }}               # xai / grok
    # (cline, copilot, junie, cursor authenticate via their own CLI login flow)

    # ── GitHub auth for the `gh` CLI freq-ai shells out to ──
    GH_TOKEN: ${{ secrets.FREQ_AI_PAT || github.token }}              # PAT preferred so PRs trigger downstream workflows

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

    # ── freq-ai knobs ──
    # DEV_PROJECT_NAME: my-project   # override project name (otherwise inferred from the repo)
    # DISABLE_TOAK: "1"              # skip the toak-rs bootstrap snapshot (faster, less context)

    # ── Diagnostics ──
    # RUST_LOG: info                 # the action defaults to info; bump to debug/trace if you need more
```

The full hands-off setup (PAT, OAuth token, GitHub App credentials, branch protection) is documented step-by-step in the [freq-ai-hello-world README](https://github.com/geoffsee/freq-ai-hello-world#setup).

## Status: Unstable (Active Development)
Expect unexpected breaking changes.

## Docs
- [Getting Started](docs/getting_started.md) — Installation, prerequisites, Desktop vs Web App, Personas Studio, and CLI usage.
- [Workflow & Lifecycle](docs/workflow.md) — How an AI dev agent cycle works (Ideation to Retrospective), including Documentation Refresh actions.
- [Configuration & Setup](docs/configuration.md) — CLI options, bot account setup for Code Review, supported agents, and general tips.
- [Architecture](docs/architecture.md) — Project structure and internals.

## Contributing
- Open Issue -> Fork Repo -> Create Pull Request

## Contact
- File an issue for any questions or feedback.
