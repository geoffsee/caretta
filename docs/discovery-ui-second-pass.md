# Discovery & Framing UI (Second Pass, Discovery Tab)

## Scope

This slice adds first-class Discovery/Framing UI controls directly on the existing Discovery tab (no dedicated tabs), using the same workspace JSON model used by import/export.

## Implemented component priority

1. Assumption table
1. Frame comparison matrix
1. Decision log
1. Risk dashboard
1. Constraint/dependency graph (compact, cheap representation)

## Data model additions

The workspace now stores these sections in-memory and as JSON:

- `assumptions`: rows with `status`, `confidence`, `evidence`, `owner`, `validation_next_step`
- `frame_comparisons`: rows with competing framing records
- `decisions`: rows with `gate`, `rationale`, `rejected_alternatives`, `reversibility`
- `risks`: rows with `likelihood`, `impact`, `trigger`, `mitigation`
- `constraint_links`: rows with `from`, `to`, `reason`

Everything remains part of `DiscoveryWorkspace` and is persisted together with the existing discovery context payload.

## UI behavior on Discovery tab

- Added table-based editors for each section:
  - assumption rows with add/remove and inline field edit
  - frame comparison rows
  - decision records
  - risk entries
  - compact constraint rows as a minimal dependency list (`From`, `To`, `Reason`) to keep visualization overhead low
- The implementation keeps the same tab-level structure as current Discovery screens, while giving users form-like structured workflows.
- No separate read-only views were introduced in this pass.

## Persistence and import/export

- Workspace JSON I/O continues to use the saved Discovery workspace file path.
- New fields are included in:
  - load
  - save
  - export markdown
- Export now emits dedicated sections for assumptions, frame comparisons, decisions, risks, and constraints so the data can be moved between sessions.

## Test/fixture path

Added representative fixture:

- `crates/cli/tests/fixtures/discovery-workspace.json`

Added regression coverage for:

- fixture load/save roundtrip
- markdown export including new sections
- workflow template rendering behavior using discovery fixture context

## Workflow/template integration

- Discovery framing workflow assets are present under:
  - `assets/workflows/default/discovery-framing/workflow.yaml`
  - `assets/workflows/default/discovery-framing/draft.md`
  - `assets/workflows/default/discovery-framing/finalize.md`
- Tests assert that template generation still works with discovery context from fixture data.

## Future note

This pass intentionally avoids richer graph visualization chrome. Constraint/dependency work is currently structured data first, with UI graphing deferred until the model and decision contract are stable.
