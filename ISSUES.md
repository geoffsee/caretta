# ISSUES.md — Factory Backlog Tracker

This file is a live factory tracking document. It is read and updated by the
`housekeeping`, `tracker-loop-dispatch`, and `autopilot` workflows.

---

## Active Sprint — Tracker #51

**Cycle Goal**: Establish a reliable foundation: harden the snapshot generation
runtime paths, add cross-adapter smoke tests, and initialize the factory's live
tracking documents.

### Task Dependency Hierarchy

| Issue | Title | Depends On | Depended On By | Layer | Status |
|-------|-------|-----------|----------------|-------|--------|
| #48 | Audit and extend Tokio runtime-context handling in snapshot generation | — | — | 0 | 🔴 Not Started |
| #49 | Add launch-path smoke tests for each agent adapter | — | — | 0 | 🔴 Not Started |
| #50 | Initialize ISSUES.md and STATUS.md as live factory tracking documents | — | — | 0 | ✅ Done |

### Checklist

- [ ] #48 Audit and extend Tokio runtime-context handling in snapshot generation
- [ ] #49 Add launch-path smoke tests for each agent adapter
- [x] #50 Initialize ISSUES.md and STATUS.md as live factory tracking documents

### Layer 0 — Validation Gate

All three items must satisfy their own acceptance criteria **and** the following
before the layer is considered complete:

- `cargo test --workspace` green (covers #48 and #49)
- `cargo clippy --workspace --all-targets -- -D warnings` clean (covers #48 and #49)
- `ls ISSUES.md STATUS.md` succeeds and grep assertions pass (covers #50)
- No new test failures relative to `master` at sprint start

---

## Fallback / Rollback Rules

| Scenario | Action |
|----------|--------|
| #48 runtime fix causes Clippy regression | Revert the offending change in a follow-up commit; keep test scaffolding |
| #49 adapter tests introduce flaky failures | Mark flaky test `#[ignore]` with a comment explaining the condition; open a follow-up issue |
| #50 file format incompatible with housekeeping parser | Adjust format to match parser expectation; re-verify with `grep` assertions |
| CI fails after merge of any item | Revert the merge commit; investigate and re-open the child issue |
