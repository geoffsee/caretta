# GitHub Playground v0 Fast

This directory is a safe sandbox for rapidly testing an end-to-end software
factory flow before copying anything into `.github/workflows/`.

These examples assume a `freq-ai` action invocation pattern:

```yaml
- uses: geoffsee/freq-ai-action@v0.0.1
  with:
    task: housekeeping
    agent: claude
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

## Workflows included

- `workflows/nightly-housekeeping.yml`
- `workflows/weekly-backlog-curation.yml`
- `workflows/autonomous-sprint.yml`
- `workflows/weekly-ci-governance.yml`
- `workflows/monthly-factory-retrospective.yml`
- `workflows/factory-cycle-dispatch.yml` (chained end-to-end runner)

## Fast-mode behavior

- All workflows are `workflow_dispatch` only (no schedules, no issue triggers).
- Every workflow exposes one optional string input: `context`.
- The orchestrator `factory-cycle-dispatch.yml` chains the full loop:
  housekeeping -> backlog-curation -> autonomous-sprint -> ci-governance ->
  factory-retrospective.
- The same `context` input is passed into each action invocation so users can
  steer outcomes with natural language for a single run.

## Starter context example

Paste something like this into the `context` input when dispatching
`factory-cycle-dispatch.yml`:

```text
Focus this run on reliability and CI speed.

Priorities:
1) Reduce flaky tests and improve deterministic test setup.
2) Cut median CI time by at least 20 percent.
3) Avoid schema or API breaking changes in this run.

Constraints:
- No force-pushes.
- Keep PRs small and reviewable.
- Require green tests and lint before merge.

Deliverables:
- Open/update issues with clear acceptance criteria.
- Link all work to a tracker.
- End with a concise retrospective and next-run recommendations.
```
