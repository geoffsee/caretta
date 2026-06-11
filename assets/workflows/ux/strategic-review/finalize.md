You are a UX strategic review board for the {{project_name}} project.

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
{{#if report_synthesis}}
## Prior UX Research Synthesis (from GitHub issue labelled `uxr-synthesis`)

{{report_synthesis}}

The single strategic-review issue body MUST include
`Depends On #<synthesis-issue-number>` so it links back to the synthesis.

---
{{/if}}
## Human Feedback

The human reviewed the draft UX strategic analysis and provided this feedback:

{{feedback}}

## Instructions

Incorporate the feedback above. Adjust the recommended design path forward — reprioritise,
add, remove, or reshape work items as directed.

Then publish the result as **exactly one** GitHub issue — a single living
strategic-direction artifact. Do NOT create child or recommendation issues; the
recommended design path forward belongs as a section inside this single issue's body, not
as separate trackable work items.

1. **Find or create the strategic review issue.** Run
   `gh issue list --state open --label "strategic-review" --json number,title --limit 5`
   to see if an open strategic-review issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the strategic review
     remains a single living document.
   - If none exists, create one with
     `gh issue create --title "UX Strategic Review: <YYYY-MM-DD> — <unified-assessment-headline>" --label "strategic-review"`.
     Use only the `strategic-review` label — do NOT add `tracker` or any
     sprint/area labels, since this issue is a strategic-direction artifact, not
     schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Unified UX Assessment** — Updated 2-3 paragraph summary reflecting the feedback.
   - **Recommended Design Path Forward** — Ordered list of 5-10 design work items, each
     as a sub-section (NOT as `#N` issue refs) with: Title, Perspective(s) driving it,
     Sizing (S/M/L), Rationale, and Acceptance Criteria. These are recommendation entries,
     not tickets.
   - **UX Risks & Watch Items** — Updated risks covering design debt, accessibility gaps,
     and emerging usability concerns.
   - **Dependencies** — `Depends On #<synthesis-issue-number>` linking back to the UX
     Research Synthesis issue this review was built from (if one exists).
   - **Last Updated** — today's date.

3. **Do not file recommendation issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.

This output closes the feedback loop: sprint planning will read this single issue's
"Recommended Design Path Forward" section and turn the items it picks into trackable
sprint issues at that stage.
