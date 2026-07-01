#!/usr/bin/env bash
# Copyright (c) 2024-2026 Geoff Seemueller
#
# Licensed under the MIT License or Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# See LICENSE-MIT or LICENSE-APACHE for the full license text.
#
# Additionally, this file is subject to the Revenue Sharing Agreement terms
# as defined in REVENUE-SHARING.md for covered organizations.
set -euo pipefail

REPO="geoffsee/caretta"
INSTALL_SCRIPT="https://raw.githubusercontent.com/${REPO}/master/install.sh"

bold="\033[1m"
dim="\033[2m"
green="\033[32m"
red="\033[31m"
reset="\033[0m"

pass=0
fail=0

check() {
  local label="$1"; shift
  if "$@" >/dev/null 2>&1; then
    printf "  ${green}✓${reset} %s\n" "$label"
    ((pass++))
  else
    printf "  ${red}✗${reset} %s\n" "$label"
    ((fail++))
  fi
}

check_output() {
  local label="$1" pattern="$2"; shift 2
  local out
  if out=$("$@" 2>&1) && echo "$out" | grep -qE "$pattern"; then
    printf "  ${green}✓${reset} %s\n" "$label"
    ((pass++))
  else
    printf "  ${red}✗${reset} %s  ${dim}(expected /%s/)${reset}\n" "$label" "$pattern"
    ((fail++))
  fi
}

# ── install via remote script ───────────────────────────────────────
INSTALL_DIR="$(mktemp -d)/caretta-smoke"
trap 'rm -rf "$INSTALL_DIR"' EXIT
export CARETTA_INSTALL_DIR="$INSTALL_DIR"

BIN="$INSTALL_DIR/caretta"

printf "\n${bold}caretta smoke test${reset}\n\n"
printf "${dim}install script: %s${reset}\n" "$INSTALL_SCRIPT"
printf "${dim}install dir:    %s${reset}\n\n" "$INSTALL_DIR"

check "remote install script runs successfully" \
  bash -c "curl -fsSL '$INSTALL_SCRIPT' | bash"

# ── post-install checks ────────────────────────────────────────────
check "binary exists and is executable"           test -x "$BIN"
check_output "--help exits 0 and shows usage"     "USAGE|Usage|usage|caretta" "$BIN" --help
check_output "--version prints version"           "caretta [0-9]" "$BIN" --version

for cmd in gui ideation code-review security-review sprint-planning retrospective roadmapper housekeeping refresh-agents refresh-docs; do
  check "$cmd --help exits 0" "$BIN" "$cmd" --help
done

# ── summary ─────────────────────────────────────────────────────────
total=$((pass + fail))
printf "\n${bold}%d/%d passed${reset}" "$pass" "$total"
if [ "$fail" -gt 0 ]; then
  printf "  ${red}(%d failed)${reset}\n\n" "$fail"
  exit 1
else
  printf "  ${green}all good${reset}\n\n"
fi