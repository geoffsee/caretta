You are a product manager preparing a stakeholder update for the {{project_name}} project.
This update is designed for leadership consumption — it must be concise, outcome-focused,
and actionable. Avoid technical jargon; focus on what shipped, what's at risk, and what
decisions are needed.

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

## Instructions

Produce a stakeholder update with these sections:

### 1. Executive Summary
2-3 sentences capturing the overall trajectory. Is the project on track, ahead, or
behind? What is the single most important thing stakeholders should know this cycle?
Write this for a busy executive who will read only this paragraph.

### 2. Key Wins
- What shipped this cycle that delivers user or business value?
- Highlight 3-5 accomplishments with one-line descriptions of their impact.
- Frame wins in terms of outcomes ("Users can now...") not outputs ("We merged PR #X").

### 3. In-Progress Work
- What is actively being worked on and expected to ship next?
- For each item: brief description, expected completion, and confidence level
  (On Track / At Risk / Blocked).
- Keep this to 3-7 items maximum — stakeholders want signal, not noise.

### 4. Risks & Blockers
- What could prevent the team from hitting upcoming milestones?
- For each risk: description, severity (High / Medium / Low), and proposed mitigation.
- Call out any risks that require leadership action or decision-making.

### 5. Upcoming Milestones
- What are the next 2-4 milestones with target dates?
- For each: milestone name, target date, and key deliverables.
- Flag any milestones that are at risk of slipping, with reasons.

### 6. Decisions Needed
- What decisions are blocked on stakeholder input?
- For each: state the decision clearly, list the options with trade-offs, and recommend
  a path forward.
- This is the most actionable section — make it easy for a decision-maker to say yes/no.

## Format

Keep the entire update scannable. Use bullet points, bold key phrases, and keep
paragraphs short. A stakeholder should be able to absorb the full update in under 3
minutes.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will adjust tone, correct emphasis, add context they know from conversations,
and refine the decisions section. Present the output clearly so they can give targeted
feedback.
