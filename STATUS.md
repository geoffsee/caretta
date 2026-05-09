# Project Status

Live tracker for freq-ai capability and feature health. Each row points to
its primary tracking issue; housekeeping audits this file for drift against
GitHub issue state.

## Status Legend

- 🔴 Broken / regressed — block release
- 🟡 In Progress — feature being built or repaired
- 🟢 Stable — covered by tests, working in production
- ✅ Complete — capability fully delivered and stable
- ⚠️ Degraded — usable but with known caveats

## Capabilities

| Area | Capability | Tracking | Status | Notes |
|------|-----------|----------|--------|-------|
| Agent adapters | Cross-adapter launch-path smoke coverage | #49 | ✅ | All 8 adapters (claude, cline, codex, copilot, gemini, grok, junie, xai) plus Cursor have inline `launch_path_propagates_not_found_for_absent_binary` smoke tests covering `freqai_native_run_argv` + `launch_*` composition and the `Command::spawn` path under an environment-absent guard |
| Snapshot generation | Tokio runtime-context handling | #48 | 🟡 | In progress — runtime-context audit underway |
| Factory tracking | ISSUES.md / STATUS.md live documents | #50 | 🟡 | Initial scaffolding seeded via #49; #50 to finalize structure |
