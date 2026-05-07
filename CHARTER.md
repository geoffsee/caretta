# Charter

This Charter governs the **CI Governance** workflow
(`.github/workflows/weekly-ci-governance.yml` and the
`software-factory/ci-governance` preset). It does not govern any other
workflow.

The rest of the factory cycle — `autopilot`, `tracker-loop-dispatch`,
`weekly-backlog-curation`, `autonomous-sprint`, `factory-cycle-dispatch`,
`release-mediator`, `release-tag-publisher`, `release`,
`nightly-housekeeping` — operates per `.github/CI.md` and is intentionally
out of scope here. This Charter is **not** a brake on the cycle. It is a
brake on one specific workflow that has shown a tendency to convert audit
output into open-ended tech-debt work.

`COVENANT.md` still applies to CI Governance (and to every other workflow);
notably the security-private rule there is universal.

## 1. Mission of CI Governance

Produce a short, honest read of CI safety. That is the entire mission.

CI Governance is **advisory**. Its successful output is most often:

> "No action required this cycle."

CI Governance is **not** chartered to:

- generate hardening roadmaps,
- file backlog issues for "best-practice" deviations,
- create or update trackers,
- expand `ISSUES.md` or `STATUS.md` beyond a short note,
- modify any file under `.github/workflows/`,
- normalize the project to external CI templates.

## 2. Bar for filing public work from a CI Governance run

A CI Governance finding becomes a public issue only if **all** hold:

1. It points to a concrete, observable harm to reliability or correctness
   that a maintainer would want to know about today.
2. It is reproducible from the issue body alone.
3. It is actionable in roughly one focused PR.
4. It is not already represented in an open issue, tracker, or PR.
5. It is **not** a security finding. Security findings follow
   `COVENANT.md` §2 and never go public.

If any condition fails, the finding is dropped or rolled into the run's
short summary. Stylistic, speculative, or "would be nice" findings are
dropped.

## 3. Output limits for a CI Governance run

Per CI Governance run, unless the run's `context` input explicitly raises a
limit:

| Action                                  | Limit |
| --------------------------------------- | ----- |
| New issues opened                       | ≤ 1, consolidating all qualifying findings |
| New trackers opened                     | 0     |
| New labels created                      | 0     |
| Lines appended to `ISSUES.md`           | ≤ 20  |
| Lines appended to `STATUS.md`           | ≤ 10  |
| Bulk edits / reformatting of `ISSUES.md` or `STATUS.md` | none |
| Files under `.github/workflows/` modified | 0   |
| PRs opened                              | 0     |

If the run would exceed a limit, it stops and reports
"limit reached, deferring." That is a successful, expected outcome.

## 4. Workflow files are recommendation-only

CI Governance must **not** edit `.github/workflows/`, change action
versions, change `permissions:` scopes, change concurrency groups, change
trigger conditions, or introduce new workflows. It records the
recommendation in plain text for human review and stops.

If a CI Governance run cannot complete without modifying a workflow file,
it stops and reports the blocker rather than proceed.

## 5. Interaction with the draft → finalize phases

The CI Governance preset has a `draft` and a `finalize` phase.

- **Draft**: produce the short read of CI safety. No file changes, no
  issue creation. (Already true in `assets/workflows/software-factory/ci-governance/draft.md`.)
- **Finalize**: incorporate human feedback and, **only if §2 is satisfied
  for at least one finding**, open at most one consolidated public issue
  and append a short note to `STATUS.md`. Otherwise, finalize ends with a
  one-line "no action required" entry in `STATUS.md` and exits clean.

The finalize prompt's instructions to "create or update a tracker labeled
`tracker,security`", "update `ISSUES.md` with the same remediation plan",
and "ensure issue tracker and local docs remain in parity" are interpreted
through this Charter: the agent produces the **shortest honest version**
that satisfies them without violating §3 or `COVENANT.md` §2. In practice
that means the tracker is **not created** by the agent, the `ISSUES.md`
update is at most the §3-bounded append, and security findings stay in the
private maintainer note.

## 6. Ambiguity and stop conditions

When the run's `context` input does not resolve an ambiguity, CI Governance
stops and reports the ambiguity. It does not guess at maintainer intent on
multi-step hardening, workflow restructuring, dependency tightening, or
permission changes.

A stopped run is a successful run.

## 7. Amendments

This Charter is amended only by a human-authored commit on `master`. CI
Governance does not propose edits to this file. It may, in its draft, note
that an amendment seems warranted and explain why; the amendment itself is
a human decision.
