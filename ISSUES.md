# Issues

Live tracker for the freq-ai factory cycle. The Task Dependency Hierarchy
table is the source of truth for sibling layering and status during a sprint.
Housekeeping (`assets/workflows/default/housekeeping/draft.md`) audits this
table for drift against GitHub issue state.

## Active Cycle: Tracker #51 — foundation hardening

**Cycle goal:** Establish a reliable foundation: harden snapshot generation
runtime paths, add cross-adapter smoke tests, and initialize the factory's
live tracking documents.

### Task Dependency Hierarchy

| Issue | Title | Depends On | Depended On By | Layer | Status |
|-------|-------|-----------|----------------|-------|--------|
| #48 | Audit and extend Tokio runtime-context handling in snapshot generation | — | — | 0 | 🔴 Not Started |
| #49 | Add launch-path smoke tests for each agent adapter | — | — | 0 | ✅ Done |
| #50 | Initialize ISSUES.md and STATUS.md as live factory tracking documents | — | — | 0 | 🔴 Not Started |

All three items are independent (Layer 0) and execute in parallel.

### Layer 0 — Validation Gate

- `cargo test --workspace` green (covers #48 and #49)
- `cargo clippy --workspace --all-targets -- -D warnings` clean (covers #48 and #49)
- `ls ISSUES.md STATUS.md` succeeds and grep assertions pass (covers #50)
- No new test failures relative to `master` at sprint start

## Status Legend

- 🔴 Not Started — issue open, no work begun
- 🟡 In Progress — branch open or PR draft against the issue
- 🟢 Review — PR open and awaiting review
- ✅ Done — PR merged or issue closed
- ⚠️ Blocked — see issue body for blocker
