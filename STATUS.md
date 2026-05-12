# Feature Status

## Audit / Provenance Layer (Tracker #76)

| Feature | Status | Notes |
|---------|--------|-------|
| Structured Agent Event Log (#70) | ✅ Done | SQLite append-only log; schema versioned; `--dry-run` preview; configurable via `CARETTA_EVENT_LOG` or `event_log_path` in `caretta.toml` |
| Workflow Checkpoint and Resume (#71) | 🔴 Not Started | |
| Adapter Capability Negotiation (#72) | 🔴 Not Started | |
| Deterministic Asset Hash Pinning (#73) | 🔴 Not Started | |
| Workflow Preset Versioning (#74) | ✅ Done | `preset.yaml` manifests with semver version; `name@req` resolution; v2 DB schema records preset name+version per run |
| Path-Constraint Capability (#75) | ✅ Done | `[path_constraints]` in `caretta.toml`; per-workflow YAML override; system-prompt injection; post-run violation detection logged to SQLite event log (schema v3) |
