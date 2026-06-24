# GitHub Actions: Disable/Adjust Fork PR Approval for a Repository

This document explains how to change the fork pull request workflow approval policy used by GitHub Actions.

## What this setting is

Repository setting API endpoint:
- `GET/PUT repos/{owner}/{repo}/actions/permissions/fork-pr-contributor-approval`
- Controls which fork PRs require approval before their workflows can run.

Returned value (`approval_policy`):
- `first_time_contributors_new_to_github`
  - New behavior (most strict): only contributors who are new to GitHub need approval.
- `first_time_contributors`
  - Less strict: first-time contributors to the repository need approval.
- `all_external_contributors`
  - Most permissive: all external contributors need approval.

## Prerequisites

- `gh` CLI installed and authenticated.
- Authenticated account must have repo admin access.
- Token must include the `repo` scope.

## 1) Check current policy

```bash
gh api repos/<OWNER>/<REPO>/actions/permissions/fork-pr-contributor-approval --jq '.approval_policy'
```

Example:
```bash
gh api repos/geoffsee/tx-monitor/actions/permissions/fork-pr-contributor-approval --jq '.approval_policy'
```

## 2) Set a new policy

Use one of the values above for `approval_policy`.

### Example: stop requiring approval for first-time contributors that are new to GitHub

```bash
gh api --method PUT repos/geoffsee/tx-monitor/actions/permissions/fork-pr-contributor-approval \
  -H "Accept: application/vnd.github+json" \
  -f approval_policy=first_time_contributors
```

## 3) Verify the change

```bash
gh api repos/geoffsee/tx-monitor/actions/permissions/fork-pr-contributor-approval --jq '.approval_policy'
```

Expected output should match the value you set.

## Optional: using GitHub UI

You can also set this in the repository UI:
- `Settings` → `Actions` → `General` → section for fork PR workflow behavior
- Update the corresponding approval option.

