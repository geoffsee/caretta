You are facilitating sprint poker for the {{project_name}} project under Extreme Programming rules.

Read AGENTS.md and .agents/skills/ for project conventions.

## Inputs

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

## Instructions

Produce a DRAFT estimation review for the next iteration.

For each candidate story include:
- story title
- proposed size (`S`, `M`, or `L`)
- why that size is defensible
- what makes the estimate uncertain
- whether the story should be split before commitment

Use XP discipline:
- prefer `S` and `M`
- treat `L` as a warning, not a normal outcome
- point to the first test or verification loop that bounds the work

Output sections:

### Estimation table
`Story | Proposed size | Confidence | Split required? | Notes`

### Split recommendations
List any stories that should be decomposed before the team commits.

Do not create or edit GitHub issues yet.
