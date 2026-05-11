# Issue Tracker — Sprint #76

## Goal

Deliver the foundational audit/provenance layer that all three personas (Aanya Kapoor, Liis Kask, Hannah Sørensen) identify as non-negotiable prerequisites for production adoption. No capability-expansion features ship until this foundation is validated.

## Task Dependency Hierarchy

| Issue | Depends On | Depended On By | Layer | Status |
|-------|-----------|----------------|-------|--------|
| #70 feat: Structured Agent Event Log | — | #74, #75 | 0 | 🔴 Not Started |
| #71 feat: Workflow Checkpoint and Resume | — | — | 0 | 🔴 Not Started |
| #72 feat: Adapter Capability Negotiation | — | — | 0 | 🔴 Not Started |
| #73 feat: Deterministic Asset Hash Pinning | — | — | 0 | ✅ Done |
| #74 feat: Workflow Preset Versioning | #70 | — | 1 | 🔴 Not Started |
| #75 feat: Path-Constraint Capability | #70 | — | 1 | 🔴 Not Started |

## Checklist

- [ ] #70 feat: Structured Agent Event Log
- [ ] #71 feat: Workflow Checkpoint and Resume
- [ ] #72 feat: Adapter Capability Negotiation
- [x] #73 feat: Deterministic Asset Hash Pinning
- [ ] #74 feat: Workflow Preset Versioning (blocked by #70)
- [ ] #75 feat: Path-Constraint Capability (blocked by #70)

## References

Strategic Review: #69
UXR Synthesis: #68
Ideation: #67
