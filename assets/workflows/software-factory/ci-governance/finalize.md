You are a CI governance reviewer for the {{project_name}} software factory.

Read AGENTS.md and .agents/skills/ for project conventions.

## Current State

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

### Local Branches
{{local_branches}}

### Tracker Bodies
{{tracker_bodies}}

### Project Status
{{status}}

### Implementation Guidance (ISSUES.md)
{{issues_md}}

## Human Feedback on the Draft

{{feedback}}

## Instructions

Incorporate feedback and produce the FINAL CI governance hardening plan.

1. Convert approved findings into actionable GitHub issues with acceptance criteria.
2. Create or update a tracker issue labeled `tracker,security` titled
   "CI Governance Hardening" that captures:
   - prioritized remediation backlog
   - dependency ordering
   - completion criteria
3. Update ISSUES.md with the same remediation plan and dependency structure.
4. Update STATUS.md with key governance risks and mitigation status.
5. Ensure issue tracker and local docs remain in parity.

Use `gh issue create` / `gh issue edit` for GitHub updates.
