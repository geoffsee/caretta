# GitHub Software Factory

This directory defines the GitHub Actions control plane for running `caretta`
against issues, trackers, pull requests, and release checkpoints.

## Action contract

Most agent workflows call the published action:

```yaml
- uses: geoffsee/caretta-action@v0.0.6
  with:
    task: loop
    args: 123
    agent: claude
  env:
    CLAUDE_CODE_OAUTH_TOKEN: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
```

For preset workflows that are not native top-level `caretta` commands, use:

```yaml
with:
  task: run
  args: backlog-curation
  preset: software-factory
```

Native commands such as `housekeeping`, `loop`, `code-review`, and `fix-pr`
should use their native `task` value directly.

## Workflows

- `nightly-housekeeping.yml`: manual/callable housekeeping pass.
- `weekly-backlog-curation.yml`: manual/callable software-factory backlog curation.
- `autonomous-sprint.yml`: manual/callable autonomous sprint planning/execution.
- `weekly-ci-governance.yml`: manual/callable CI governance review.
- `monthly-factory-retrospective.yml`: manual/callable retrospective.
- `factory-cycle-dispatch.yml`: chained factory cycle.
- `tracker-loop-dispatch.yml`: manual tracker execution, followed by code review and review-fix follow-up.
- `autopilot.yml`: scheduled/manual controller that evaluates issues/PRs and dispatches tracker or backlog work.
- `release-mediator.yml`: scheduled/manual neutral release checkpoint generator that promotes checkpoints to tags.
- `release-tag-publisher.yml`: reusable/manual tag publisher for checkpoint issues.
- `release.yml`: builds release artifacts and publishes the `caretta` crate when a `v*` tag is pushed.

## Operating Flow

1. `Autopilot` runs every 6 hours and evaluates open issues and PRs.
2. If an open `tracker` issue exists, Autopilot dispatches `tracker-loop-dispatch.yml`.
3. If work exists but no tracker exists, Autopilot dispatches backlog curation.
4. Tracker loop work creates or updates PRs.
5. After tracker work succeeds, `tracker-loop-dispatch.yml` runs `caretta code-review`.
6. After code review succeeds, it runs `caretta fix-pr <PR>` for each open PR.
7. `Release Mediator` creates a neutral, time-bounded checkpoint issue each Friday.
8. `Release Mediator` calls `Release Tag Publisher` with the checkpoint issue number.
9. `Release Tag Publisher` creates and pushes the next annotated `v*` tag.
10. The pushed `v*` tag triggers `release.yml`, which builds release artifacts and publishes the matching Cargo version to crates.io.

## Release Checkpoints

Release checkpoints are intentionally unemotional. They are factual records of
feature groups between two points in time, based on merged PRs, closed issues,
and current open PR state. They are not release announcements, quality claims,
or launch narratives.

`release-mediator.yml` and `release-tag-publisher.yml` are implicitly bound:
each published checkpoint is promoted to the next patch tag by default. The tag
publisher validates the issue has the `release-checkpoint` label, computes the
next semver tag from the requested bump, confirms it matches the workspace
`Cargo.toml` version, creates an annotated tag, and pushes it.

Release automation is constrained to `master`. `release-mediator.yml` skips
checkpoint creation and tag publishing unless the workflow ref is
`refs/heads/master`, and `release-tag-publisher.yml` checks out `master`
explicitly before creating any tag.

## Manual Context

Most manual workflows accept a `context` input. Use it to steer the run without
changing workflow logic.

Example:

```text
Focus this run on reliability and CI speed.

Priorities:
1. Reduce flaky tests and improve deterministic setup.
2. Cut median CI time by at least 20 percent.
3. Avoid schema or API breaking changes.

Constraints:
- Keep PRs small and reviewable.
- Require green tests and lint before merge.
- Surface ambiguous scope rather than guessing.
```
