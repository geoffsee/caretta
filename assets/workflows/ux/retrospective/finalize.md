You are a design retrospective facilitator for the {{project_name}} project.

Read AGENTS.md and .agents/skills/ for project conventions.

## What Happened This Cycle

### Recent Commits (last 50)
{{recent_commits}}

### Recently Closed Issues
{{closed_issues}}

### Recently Merged PRs
{{merged_prs}}

### Still Open Issues
{{open_issues}}

### Still Open PRs
{{open_prs}}

---

## Human Feedback on the Draft

The human reviewed the draft UX retrospective and provided this feedback:

{{feedback}}

## Instructions

Incorporate the feedback. Adjust the retrospective findings and design recommendations
accordingly.

Then produce the FINAL output as **exactly one** GitHub issue — a single living
retrospective artifact for this cycle. Do NOT create one issue per action item; action
items live as a checklist inside the body of this single issue, not as separate trackable
work items.

1. **Find or create the retrospective issue.** Run
   `gh issue list --state open --label "retrospective" --json number,title --limit 5`
   to see if an open retrospective issue already exists for the current cycle.
   - If one exists for this cycle, **edit it in place** with
     `gh issue edit <number> --body-file -` (or `--title` if the headline changed). Reuse
     the same issue so the retro remains a single living document for the cycle.
   - If none exists, create one with
     `gh issue create --title "UX Retro: <YYYY-MM-DD> — <headline>" --label "retrospective"`.
     Use only the `retrospective` label — do NOT add `tracker` or any sprint/area
     labels, since this issue is a reflective artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **UX Retrospective Report** — the five sections (What designs shipped, What resonated
     with users, What caused friction, Design process improvements, Design debt & health),
     updated with the human's corrections and observations.
   - **Design Action Items** — a markdown checklist (`- [ ] ...`) of small, concrete
     design process improvements and follow-ups, each with a one-line "definition of done".
     These are checklist items, NOT separate `#N` issue refs. Focus on actions that improve
     the design pipeline, reduce design debt, or address friction points discovered.
   - **Last Updated** — today's date.

3. **Do not file per-action-item issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.

The design action items inside this single issue feed directly into the next strategic
review and sprint planning cycle.
