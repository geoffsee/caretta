#!/usr/bin/env bash
# Copyright (c) 2026 Geoff Seemueller
#
# Licensed under the MIT License or Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# See LICENSE-MIT or LICENSE-APACHE for the full license text.
#
# Additionally, this file is subject to the Revenue Sharing Agreement terms
# as defined in REVENUE-SHARING.md for covered organizations.
# Fail when any tracked Rust source file exceeds the per-file line limit.
# Shared by .github/workflows/ci.yml and the pre-commit hook so the gate is
# enforced identically locally and in CI. Override with RS_LINE_LIMIT=<n>.

set -euo pipefail

LIMIT="${RS_LINE_LIMIT:-2500}"
fail=0

while IFS= read -r f; do
    [ -f "$f" ] || continue
    lines=$(wc -l < "$f")
    if [ "$lines" -gt "$LIMIT" ]; then
        printf '%s: %d lines (limit %d) — split into a directory module\n' \
            "$f" "$lines" "$LIMIT" >&2
        fail=1
    fi
done < <(git ls-files '*.rs')

if [ "$fail" -ne 0 ]; then
    printf '\nOne or more Rust files exceed the %d-line limit.\n' "$LIMIT" >&2
    exit 1
fi
