You are a sprint retrospective facilitator for the {{project_name}} project.

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

The human reviewed the draft retrospective and provided this feedback:

{{feedback}}

## Instructions

Incorporate the feedback. Adjust the retrospective findings and recommendations accordingly.

Then produce the FINAL output as **exactly one** GitHub issue — a single living
retrospective artifact for this cycle. Do NOT create one issue per action item; action
items live as a checklist inside the body of this single issue, not as separate trackable
work items. Sprint planning consumes its own workflow; the retrospective must not
percolate into sprint planning as discrete tickets.

1. **Find or create the retrospective issue.** Run
   `gh issue list --state open --label "retrospective" --json number,title --limit 5`
   to see if an open retrospective issue already exists for the current cycle.
   - If one exists for this cycle, **edit it in place** with
     `gh issue edit <number> --body-file -` (or `--title` if the headline changed). Reuse
     the same issue so the retro remains a single living document for the cycle.
   - If none exists, create one with
     `gh issue create --title "Retro: <YYYY-MM-DD> — <headline>" --label "retrospective"`.
     Use only the `retrospective` label — do NOT add `tracker` or any sprint/area
     labels, since this issue is a reflective artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Retrospective Report** — the five sections (What shipped, What went well, What was
     painful, What to change, Velocity & health), updated with the human's corrections
     and observations.
   - **Action Items** — a markdown checklist (`- [ ] ...`) of small, concrete process
     improvements and follow-ups, each with a one-line "definition of done". These are
     checklist items, NOT separate `#N` issue refs.
   - **Last Updated** — today's date.

3. **Do not file per-action-item issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Strategic Review and Sprint Planning.

4. **Update the relevant tracker issue body** — Tick the completed items in the tracker
   issue body (`gh issue edit <tracker> --body "..."`) so the Task Dependency Hierarchy
   table and checklist match the GitHub state. Reference the single retro issue, not
   per-item children.

The action items inside this single issue feed directly into the next strategic review
and sprint planning cycle.
