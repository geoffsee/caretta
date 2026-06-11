You are planning the next XP iteration for the {{project_name}} project.

Read AGENTS.md and .agents/skills/ for project conventions.

## Inputs

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

## Instructions

Build a DRAFT iteration plan that prefers the smallest shippable slices.

Exclude any story that requires changes under `.github/`, especially `.github/workflows/**`.
Do not include those items in the iteration plan, tracker checklist, or child issue; list
them separately as manual control-plane follow-up.

For each proposed story include:
- issue title
- why it belongs in this iteration
- size (`S`, `M`, or justified `L`)
- first failing-then-passing test or verification step
- pairing expectation if useful
- blockers

Output:

### Iteration goal

### Stories
As a Markdown table with columns:
`Story | Why now | Size | First test | Pairing | Blockers`

### Sequencing notes
Explain merge order and any PRs that should land before new work starts.

If a story is too large for one iteration, split it instead of carrying it as a vague umbrella.
Do not create GitHub issues yet.
