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

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK_DIR="$REPO_ROOT/.git/hooks"

echo "Installing pre-commit hook..."

cat > "$HOOK_DIR/pre-commit" << 'HOOK'
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all
git add -A

echo "==> Checking per-file Rust line limit..."
if ! ./scripts/check-rs-line-limit.sh; then
    echo ""
    echo "Split the offending file(s) into directory modules."
    exit 1
fi

echo "==> Running cargo fmt --check (all crates)..."
if ! cargo fmt --all -- --check; then
    echo ""
    echo "Formatting errors found. Run 'cargo fmt --all' to fix."
    exit 1
fi

echo "==> Running cargo clippy (all crates)..."
if ! cargo clippy --workspace --all-targets -- -D warnings; then
    echo ""
    echo "Clippy warnings found. Fix them before committing."
    exit 1
fi

echo "==> All checks passed."
HOOK

chmod +x "$HOOK_DIR/pre-commit"
echo "Pre-commit hook installed at $HOOK_DIR/pre-commit"

echo "Installing pre-push hook..."

cat > "$HOOK_DIR/pre-push" << 'HOOK'
#!/usr/bin/env bash
set -euo pipefail

echo "==> Running cargo test (all crates)..."
if ! cargo test --workspace; then
    echo ""
    echo "Tests failed. Fix them before pushing."
    exit 1
fi

echo "==> All tests passed."
HOOK

chmod +x "$HOOK_DIR/pre-push"
echo "Pre-push hook installed at $HOOK_DIR/pre-push"