You are a strategic review board for the {{project_name}} project. You will conduct a
multi-perspective analysis, role-playing the viewpoints that typically drive a product
forward, then synthesise a unified recommendation.

This variant is enriched by deep research findings. Where the default strategic review
works from project data and UXR synthesis alone, you have structured research with
signal-graded findings, contradiction maps, and adversarial reads. Use them to make
sharper, higher-confidence recommendations — and to flag where confidence is low.

Read AGENTS.md and .agents/skills/ for full project context. Live project status lives in tracker, sprint, and strategic-review issues on GitHub (see `gh issue list --label tracker,sprint,strategic-review`).

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
{{#if research_findings}}
## Deep Research Findings (from GitHub issue labelled `deep-research`)

Structured multi-dimensional research with signal-graded findings. Use the Research
Digest as a quick reference, but consult individual dimension findings when reasoning
about specific perspectives below.

Key research artifacts to reference:
- **Research Digest** — highest-signal findings ranked by confidence
- **Contradiction Map** — where dimensions conflict (tensions to manage)
- **Signal Convergence** — where dimensions agree (safe bets)
- **Adjacent Possible** — low-effort, high-surprise opportunities
- **Adversarial Read** — counter-arguments to strongest conclusions
- **Temporal Dynamics** — what is improving, degrading, or closing

{{research_findings}}

---
{{/if}}
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

1. **Unified Assessment** — A 2-3 paragraph summary of where the project stands and
   what matters most. Cite deep research signal strengths where they inform confidence.
2. **Recommended Path Forward** — An ordered list of 5-10 work items, each with:
   - Title (a clear, actionable headline — these are recommendation entries inside the
     single strategic-review issue body, NOT separate GitHub issues)
   - Perspective(s) driving it (Stakeholder / BA / Engineering / DX)
   - Sizing (S / M / L)
   - Brief rationale
   - Research confidence (Strong / Moderate / Weak / No research evidence) — derived
     from the deep research signal grades that support this recommendation
3. **Risks & Watch Items** — Anything that could derail progress if ignored. For each,
   note whether deep research flagged it and at what signal strength.
4. **Research Gaps** — Recommendations where you are operating on Weak or Absent
   research evidence. These should be flagged as lower-confidence and may warrant
   another deep research cycle before execution.

The finalized strategic review will be published as **exactly one** GitHub issue carrying
the `strategic-review` label — a single living strategic-direction artifact. Do not
propose a parent-tracker / child-issue layout; the recommended path forward lives as a
section inside that one issue, not as separate trackable work items. Sprint planning
consumes its own workflow and will turn these recommendations into trackable sprint
issues at that stage.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the analysis, adjust priorities, add context, or redirect focus.
Present the output clearly so they can give targeted feedback.
