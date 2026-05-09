# Factory Issues — Live Tracking

This document mirrors the Task Dependency Hierarchy from the active sprint
tracker (parent: #51) and is kept in sync as child issues land. Statuses
follow the legend: 🔴 Not Started · 🟡 In Progress · ✅ Done.

## Cycle Goal

Establish a reliable foundation: harden the snapshot generation runtime paths,
add cross-adapter smoke tests, and initialize the factory's live tracking
documents.

## Task Dependency Hierarchy

| Issue | Title                                                                       | Depends On | Depended On By | Layer | Status |
|-------|-----------------------------------------------------------------------------|------------|----------------|-------|--------|
| #48   | Audit and extend Tokio runtime-context handling in snapshot generation      | —          | —              | 0     | ✅ Done |
| #49   | Add launch-path smoke tests for each agent adapter                          | —          | —              | 0     | 🔴 Not Started |
| #50   | Initialize ISSUES.md and STATUS.md as live factory tracking documents       | —          | —              | 0     | 🔴 Not Started |

## Layer 0 — Validation Gate

- [x] `cargo test --workspace` green (covers #48 and #49)
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean (covers #48 and #49)
- [ ] `ls ISSUES.md STATUS.md` succeeds and grep assertions pass (covers #50)
- [x] No new test failures relative to `master` at sprint start
