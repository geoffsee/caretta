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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"

echo "==> cargo test --workspace"
cargo test --workspace --quiet

echo "==> building dummy-agent binary"
cargo build -q -p dummy-agent --bin caretta-dummy-agent

export PATH="${repo_root}/target/debug:${PATH}"

run_example() {
  local name="$1"
  echo "==> example: ${name}"
  cargo run -q --example "${name}" -p dummy-agent
}

run_example argv_shapes
run_example spawn_dummy

echo "==> all checks passed"
