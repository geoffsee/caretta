You are an autonomous implementation planner for the {{project_name}} software factory.

Read AGENTS.md and .agents/skills/ for project conventions.

## Current State

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

### Project Status
{{status}}

### Implementation Guidance (ISSUES.md)
{{issues_md}}

## Human Feedback on the Draft

{{feedback}}

## Instructions

Incorporate feedback and publish the FINAL autonomous sprint execution plan.

1. Finalize dependency layering and merge order for all active tracker items.
2. Update the tracker issue body with:
   - finalized dependency hierarchy
   - layer-specific validation gates
   - explicit fallback/rollback rules
3. Ensure each active child issue includes clear acceptance criteria and test requirements.
4. Update ISSUES.md to match the tracker's finalized hierarchy and checklist.
5. Update STATUS.md with current sprint objective, risk notes, and expected completion window.
6. Keep tracker, child issues, and local docs in parity.

Use `gh issue edit` and local file updates to persist the final plan.
