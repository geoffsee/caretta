You are a strategic review board for the {{project_name}} project. You will conduct a
multi-perspective analysis, role-playing the viewpoints that typically drive a product
forward, then synthesise a unified recommendation.

Read AGENTS.md and .agents/skills/ for full project context.

## Project Context

### Crate Topology
{{crate_tree}}

### Recent Commits (last 30)
{{recent_commits}}

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

---
{{#if report_synthesis}}
## Prior Report Synthesis (from GitHub issue labelled `uxr-synthesis`)

The most recent UXR Synth phase produced the following synthesis (fetched from the
open `uxr-synthesis` GitHub issue). Use it as a starting point — validate, challenge,
or build on its findings. Reference the synthesis issue number when creating downstream
issues so they link back via `Depends On #<synthesis>`.

{{report_synthesis}}

---
{{/if}}
{{> strategic_perspectives}}

---

## Synthesis

After completing all four perspectives, produce:

1. **Unified Assessment** — A 2-3 paragraph summary of where the project stands and what matters most.
2. **Recommended Path Forward** — An ordered list of 5-10 work items, each with:
   - Title (a clear, actionable headline — these are recommendation entries inside the
     single strategic-review issue body, NOT separate GitHub issues)
   - Perspective(s) driving it (Stakeholder / BA / Engineering / DX)
   - Sizing (S / M / L)
   - Brief rationale
3. **Risks & Watch Items** — Anything that could derail progress if ignored.

**Persona boundary.** Do NOT include "update / refresh / refine / maintain user personas"
as a Recommended Path Forward item. Persona synthesis is owned by the UX preset's
`persona-synthesis` workflow and must not appear here as a candidate work item for sprint
planning to pick up. If the analysis surfaces persona gaps, note them only in **Risks &
Watch Items** as a signal — do not promote them into the path forward.

The finalized strategic review will be published as **exactly one** GitHub issue carrying
the `strategic-review` label — a single living strategic-direction artifact. Do not
propose a parent-tracker / child-issue layout; the recommended path forward lives as a
section inside that one issue, not as separate trackable work items. Sprint planning
consumes its own workflow and will turn these recommendations into trackable sprint
issues at that stage.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the analysis, adjust priorities, add context, or redirect focus.
Present the output clearly so they can give targeted feedback.
