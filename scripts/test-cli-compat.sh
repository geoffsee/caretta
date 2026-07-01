#!/usr/bin/env bash
# Copyright (c) 2026 Geoff Seemueller
#
# Licensed under the MIT License or Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# See LICENSE-MIT or LICENSE-APACHE for the full license text.
#
# Additionally, this file is subject to the Revenue Sharing Agreement terms
# as defined in REVENUE-SHARING.md for covered organizations.
set -euo pipefail

# Runs live compatibility checks for each wrapper crate against provider CLIs
# installed on PATH.
#
# Usage:
#   ./scripts/test-cli-compat.sh
#
# Optional:
#   ALLOW_MISSING_BINARIES=1 ./scripts/test-cli-compat.sh

ALLOW_MISSING_BINARIES="${ALLOW_MISSING_BINARIES:-0}"
export CARETTA_LIVE_CLI_TESTS=1

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
dummy_bin="${repo_root}/target/debug/caretta-dummy-agent"
if [[ ! -x "${dummy_bin}" ]]; then
  (cd "${repo_root}" && cargo build -q -p dummy-agent --bin caretta-dummy-agent)
fi
if [[ -x "${dummy_bin}" ]]; then
  export PATH="${repo_root}/target/debug:${PATH}"
fi

if command -v npm >/dev/null 2>&1; then
  npm install -g "${repo_root}/packages/cli-compat-fixture" >/dev/null 2>&1 || true
fi

agents=(
  "dummy-agent:caretta-dummy-agent"
  "caretta-claude:claude"
  "caretta-cline:cline"
  "caretta-codex:codex"
  "caretta-copilot:copilot"
  "caretta-gemini:gemini"
  "caretta-junie:junie"
  "caretta-xai:grok"
)

missing=()
for entry in "${agents[@]}"; do
  crate="${entry%%:*}"
  binary="${entry#*:}"
  if ! command -v "$binary" >/dev/null 2>&1; then
    missing+=("$crate:$binary")
  fi
done

if [[ "${#missing[@]}" -gt 0 ]]; then
  echo "Missing provider binaries:"
  for entry in "${missing[@]}"; do
    echo "  - ${entry#*:} (crate: ${entry%%:*})"
  done
  if [[ "$ALLOW_MISSING_BINARIES" != "1" ]]; then
    echo
    echo "Set ALLOW_MISSING_BINARIES=1 to run only wrappers with installed binaries."
    exit 1
  fi
fi

echo "==> agent-common (shared trait + argv helpers)"
cargo test -p agent-common

echo "==> caretta (CLI ↔ adapter wiring)"
cargo test -p caretta --lib adapter_dispatch

# Live probes: exercise argv shapes the app uses (model, prompt/native argv, resume,
# project, output-format, yolo/bypass) where the installed CLI accepts them. These
# are best-effort: we prefer non-interactive --help/--version-style checks.
# Exit status is ignored (|| true): older CLIs may reject a flag even when newer
# ones accept it — failures are informational only.
live_probe() {
  local name="$1"
  shift
  if ! command -v "$1" >/dev/null 2>&1; then
    return 0
  fi
  # One line per probe so scheduled CI logs stay grep-friendly.
  echo "    probe: $name -> $*"
  # shellcheck disable=SC2068
  "$@" </dev/null >/dev/null 2>&1 || true
}

echo "==> live CLI argv probes (best-effort)"
if command -v caretta-dummy-agent >/dev/null 2>&1; then
  live_probe "dummy-agent help" caretta-dummy-agent --help
  live_probe "dummy-agent version" caretta-dummy-agent --version
  live_probe "dummy-agent exec" caretta-dummy-agent exec --json "probe"
fi
if command -v caretta-cli-compat-fixture >/dev/null 2>&1; then
  live_probe "cli-compat-fixture help" caretta-cli-compat-fixture --help
  live_probe "cli-compat-fixture version flag" caretta-cli-compat-fixture --version
  live_probe "cli-compat-fixture version cmd" caretta-cli-compat-fixture version
fi
if command -v claude >/dev/null 2>&1; then
  live_probe "claude help" claude --help
  live_probe "claude native-style" claude -p "probe" --output-format stream-json --verbose --help
  live_probe "claude model" claude --model opus --help
fi
if command -v codex >/dev/null 2>&1; then
  live_probe "codex help" codex --help
  live_probe "codex exec" codex exec --help
  live_probe "codex resume" codex resume --help
  live_probe "codex project" codex --cd /tmp --help
  live_probe "codex yolo/bypass" codex exec --help
fi
if command -v copilot >/dev/null 2>&1; then
  live_probe "copilot help" copilot --help
  live_probe "copilot prompt" copilot -p "probe" --help
  live_probe "copilot model" copilot --model gpt-5 --help
  live_probe "copilot output-format" copilot --output-format json --help
  live_probe "copilot yolo" copilot --yolo --help
fi
if command -v gemini >/dev/null 2>&1; then
  live_probe "gemini help" gemini --help
  live_probe "gemini native -p" gemini -p "probe" --help
  live_probe "gemini model -m" gemini -m gemini-2.5-pro --help
  live_probe "gemini output-format" gemini --output-format json --help
  live_probe "gemini yolo" gemini --yolo --help
fi
if command -v grok >/dev/null 2>&1; then
  live_probe "grok help" grok --help
  live_probe "grok native -p" grok -p "probe" --help
  live_probe "grok model -m" grok -m grok-4 --help
  live_probe "grok project" grok --directory /tmp --help
fi
if command -v cline >/dev/null 2>&1; then
  live_probe "cline help" cline --help
  live_probe "cline chat" cline chat --help
  live_probe "cline yolo" cline --yolo --help
fi
if command -v junie >/dev/null 2>&1; then
  live_probe "junie help" junie --help
  live_probe "junie native-style" junie -p "probe" --output-format stream-json --verbose --help
  live_probe "junie model" junie --model junie-pro --help
  live_probe "junie brave" junie --brave --help
fi

for entry in "${agents[@]}"; do
  crate="${entry%%:*}"
  binary="${entry#*:}"
  if ! command -v "$binary" >/dev/null 2>&1; then
    continue
  fi
  echo "==> testing crate '$crate' against binary '$binary'"
  cargo test -p "$crate"
done
