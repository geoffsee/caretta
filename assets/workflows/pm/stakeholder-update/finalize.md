You are a product manager preparing a stakeholder update for the {{project_name}} project.

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

### Recently Closed Issues
{{closed_issues}}

### Recently Merged PRs
{{merged_prs}}

---

## Human Feedback

The human reviewed the stakeholder update draft and provided this feedback:

{{feedback}}

## Instructions

Incorporate the feedback above. Adjust tone, emphasis, and framing as directed. Add
any context the human provided from conversations, meetings, or stakeholder preferences.
Refine the decisions section to be maximally actionable.

Then produce the FINAL stakeholder update with these sections:

1. **Executive Summary** — Updated to reflect feedback.
2. **Key Wins** — Adjusted framing and emphasis.
3. **In-Progress Work** — Corrected status and confidence levels.
4. **Risks & Blockers** — Updated severity and mitigations.
5. **Upcoming Milestones** — Refined dates and deliverables.
6. **Decisions Needed** — Sharpened options and recommendations.

## Publishing the Stakeholder Update as a GitHub Issue

After completing the final update, publish it as a GitHub issue so it is reviewable,
durable, and shareable with stakeholders.
{{#if dry_run}}

**DRY RUN MODE**: Do NOT actually run any `gh` commands. Instead, print the exact commands you WOULD run (gh issue list, gh issue edit/create) with their full arguments, so the human can review what would be filed.
{{/if}}

1. **Find or create the stakeholder update issue.** Run
   `gh issue list --state open --label "stakeholder-update" --json number,title --limit 5`
   to see if an open stakeholder-update issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the update remains a
     single living document that stakeholders can bookmark.
   - If none exists, create one with
     `gh issue create --title "Stakeholder Update: <YYYY-MM-DD> — <one-line headline>" --label "stakeholder-update"`.
     Use only the `stakeholder-update` label — do NOT add `tracker` or any sprint/area
     labels, since this issue is a communication artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Executive Summary** — The 2-3 sentence overview.
   - **Key Wins** — Bulleted accomplishments with impact framing.
   - **In-Progress Work** — Status table with confidence levels.
   - **Risks & Blockers** — Severity-rated risks with mitigations.
   - **Upcoming Milestones** — Timeline with target dates.
   - **Decisions Needed** — Actionable decision requests with options.
   - **Last Updated** — today's date.

3. **Do not file per-decision issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.

After publishing, print the issue URL. Format: `Stakeholder update published: <URL>`
