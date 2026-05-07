# Covenant

This Covenant is the standing agreement between human maintainers and any
autonomous agent that operates on this repository. It is intentionally
**liberal**: it is short, it grants the factory cycle room to do its job, and
it draws hard lines only where blast radius is large or recovery is costly.

The factory cycle described in `.github/CI.md` — `autopilot`,
`tracker-loop-dispatch`, `weekly-backlog-curation`, `autonomous-sprint`,
`factory-cycle-dispatch`, `release-mediator`, `release-tag-publisher`,
`release`, `nightly-housekeeping` — operates per that contract. This Covenant
does not throttle it. Workflow-specific restraint lives in `CHARTER.md`.

## 1. Universal posture

1. **Trust the existing system.** Default to the smallest change that
   satisfies the workflow's prompt.
2. **Report honestly.** "No action required this cycle" is a complete report.
   Do not pad findings to justify a run.
3. **Uncertainty is reported as uncertainty**, not converted into a
   recommendation.

## 2. Security findings are private

This rule applies to **every** autonomous run, regardless of workflow.

1. Do **not** open public issues, public PR comments, or public tracker
   entries describing security weaknesses. This includes token-scope
   concerns, workflow-permission gaps, supply-chain risk, secrets handling,
   injection surfaces, and CI privilege paths.
2. Prepare a short, factual **private maintainer note** instead. No exploit
   detail, no reproduction steps, no attacker-useful specifics. Deliver it
   through the lowest-exposure channel available (a private security
   advisory if one exists; otherwise hold it in the run's draft for the
   human reviewer).
3. Public artifacts from a run with a security finding say only:
   _"A private security note has been prepared for maintainers."_
4. The agent does **not** auto-remediate security-flavored findings. It
   does not tighten workflow permissions, rotate tokens, alter secret
   handling, or modify branch protection without explicit human direction
   in the run's `context` input.
5. When uncertain whether something is sensitive, treat it as sensitive.

## 3. Workflows are load-bearing

The files under `.github/workflows/` are interdependent and easy to
destabilize. **Review/audit-class** runs (CI Governance, retrospective,
charter) recommend changes to these files; they do not make them. Execution
workflows that need to evolve their own behavior do so through ordinary
human-reviewed PRs, not via governance output.

## 4. Precedence

Where this Covenant conflicts with a workflow prompt, this Covenant wins.
Where it conflicts with `CHARTER.md`, the stricter of the two applies for
the workflows `CHARTER.md` covers; elsewhere, this Covenant alone applies.

Human maintainers may relax any rule for a single run by stating the
relaxation in that run's `context` input.
