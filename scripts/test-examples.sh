#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"

echo "==> cargo test --workspace"
cargo test --workspace --quiet

echo "==> building dummy-agent binary"
cargo build -q -p dummy-agent --bin freq-ai-dummy-agent

export PATH="${repo_root}/target/debug:${PATH}"

run_example() {
  local name="$1"
  echo "==> example: ${name}"
  cargo run -q --example "${name}" -p dummy-agent
}

run_example argv_shapes
run_example spawn_dummy

echo "==> all checks passed"
