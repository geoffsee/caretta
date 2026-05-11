use super::*;

/// Build the prompt for the Phase 2 Fix Comments agent run (#144).
///
/// The agent is launched with `cwd` set to a fresh git worktree on the PR's
/// head branch, so all file paths in the prompt and in the agent's edits are
/// relative to that worktree (not the user's main checkout). The dev process
/// commits and pushes the worktree after the agent run completes.
pub fn build_pr_review_fix_prompt(
    project_name: &str,
    pr_num: u32,
    pr_title: &str,
    branch: &str,
    diff: &str,
    threads: &[ReviewThread],
) -> String {
    let mut threads_section = String::new();
    for (i, t) in threads.iter().enumerate() {
        threads_section.push_str(&format!(
            "### Thread {i_num} — `{path}:{line}` (by @{author})\n\n{body}\n\n",
            i_num = i + 1,
            path = t.path,
            line = t.line,
            author = t.author,
            body = t.body,
        ));
    }
    let thread_count = threads.len();

    format!(
        r#"You are addressing review comments on pull request #{pr_num} for the {project_name} project.

Read AGENTS.md and skills/ for project conventions and coding standards.

## Working directory

Your current working directory is a freshly-created git worktree on branch `{branch}`. All file paths below are relative to this worktree. Do NOT `cd` elsewhere and do NOT run `git checkout` — the calling script handles branching and cleanup.

## Pull Request #{pr_num}: {pr_title}

### Diff
```diff
{diff}
```

## Unresolved Review Threads ({thread_count})

Address each thread below. The author of each thread is the project's review bot, so these are findings from an earlier automated code review pass.

{threads_section}
## Instructions

- For each thread, edit the file at the indicated path to address the finding. The line numbers refer to the **new** version of the file (the RIGHT side of the diff above).
- Stay focused: only fix what the threads call out. Do NOT refactor neighbouring code or rename unrelated symbols. The smaller the diff, the easier the next review.
- Do NOT run any workspace-wide validation (tests, lints, builds, formatters) inside this worktree. The worktree is throwaway, builds inside it are slow, and CI will validate the push. If you want to sanity-check your edit, re-`Read` the file to confirm the change applied — that is enough.
- Do NOT commit. Do NOT push. The calling script handles commit and push so it can clean up the worktree atomically.
- Do NOT post comments or reviews back to GitHub. The calling script handles that.

If a thread is ambiguous or you cannot determine the right fix without a human, leave the file unchanged for that thread and explain in your final summary which thread(s) you skipped and why."#
    )
}

/// Build a verification-pass prompt: given the original review threads and the
/// post-fix diff, the agent decides per-thread whether the new code addresses
/// the original concern, and writes its verdict to `output_path` as JSON.
///
/// Schema written by the agent:
/// ```json
/// {
///   "verified": ["<thread_id>", ...],
///   "unverified": [{"id": "<thread_id>", "reason": "..."}, ...]
/// }
/// ```
pub fn build_pr_review_verification_prompt(
    project_name: &str,
    pr_num: u32,
    diff: &str,
    threads: &[ReviewThread],
    output_path: &str,
) -> String {
    let mut threads_section = String::new();
    for t in threads {
        threads_section.push_str(&format!(
            "### Thread `{id}`\nFile: `{path}:{line}` (by @{author})\n\n{body}\n\n---\n\n",
            id = t.id,
            path = t.path,
            line = t.line,
            author = t.author,
            body = t.body,
        ));
    }
    let thread_count = threads.len();

    format!(
        r#"You are a caretta code-review verifier on pull request #{pr_num} for the {project_name} project.

The Fix Comments agent has just pushed changes intended to address each review thread below.
Your job: for each thread, decide whether the change actually addresses the original concern.

## Working directory

Your current working directory is a freshly-created git worktree containing the post-fix code.
Use Read/Grep/Bash (read-only) to inspect the relevant files at HEAD.

## Post-fix diff (for orientation only — verify against the actual files)

```diff
{diff}
```

## Review Threads to Verify ({thread_count})

{threads_section}

## Instructions

- For each thread, decide if the new code addresses the original concern. Be strict:
  - If the fix is superficial, partial, off-target, or absent → **unverified**.
  - A suggestion-only thread is verified iff the new code reflects the spirit of the suggestion.
- Cover EVERY thread ID exactly once across the two output lists.
- Do NOT edit any files. Do NOT commit, push, or post comments. Read-only verification.

## Output

When you have decided on every thread, write a JSON file to this exact path (overwrite if it exists):

    {output_path}

The file MUST contain ONLY a JSON object matching this schema:

```json
{{
  "verified": ["<thread_id>", "..."],
  "unverified": [
    {{"id": "<thread_id>", "reason": "<short why-not>"}}
  ]
}}
```

After writing the file, your final response can be a single line summary. The calling script reads the file, not your response."#
    )
}

/// Verdict for one thread parsed from the verification agent's JSON output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationVerdict {
    pub verified: Vec<String>,
    pub unverified: Vec<UnverifiedThread>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnverifiedThread {
    pub id: String,
    pub reason: String,
}

/// Parse the JSON file the verification agent writes. Returns `None` if the
/// file is missing or malformed — callers should treat that as "no verdicts"
/// (i.e. nothing approved automatically).
pub fn parse_verification_verdict(json: &str) -> Option<VerificationVerdict> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let verified = v
        .get("verified")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let unverified = v
        .get("unverified")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(serde_json::Value::as_str)?;
                    let reason = item
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    Some(UnverifiedThread {
                        id: id.to_string(),
                        reason: reason.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(VerificationVerdict {
        verified,
        unverified,
    })
}

pub fn build_sprint_planning_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"You are a sprint planning assistant for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## Current State

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

## Instructions

Produce a DRAFT sprint plan for the next development cycle:

0. **Read upstream recommendations.** The Strategic Review workflow publishes a single
   living issue labelled `strategic-review` whose body contains the **Recommended Path
   Forward** — the canonical list of candidate work items for sprint planning. Run
   `gh issue list --state open --label strategic-review --json number,title --limit 5` to
   find it, then `gh issue view <number>` to read its body. Treat the items in
   "Recommended Path Forward" as the primary input pool for this sprint plan; the open
   issues list below is supplementary context (in-flight work, leftover items, PRs).
1. **Analyse** — Review the strategic-review recommendations, open issues, open PRs, and completed work. Identify what is ready, what is blocked, and what has open review work.
2. **Prioritise** — Rank work items by impact and urgency. Consider dependencies.
3. **Dependencies** — Identify dependencies between work items. Assign each item a Layer number (0 = no dependencies, 1 = depends on layer-0 items, etc.). Items in the same layer can run in parallel.
4. **Group** — Organise items into a coherent sprint with clear goals.
5. **Estimate** — Provide rough sizing (S/M/L) for each item.
6. **Output** — Present the draft sprint plan with a Task Dependency Hierarchy table:

   | Issue | Depends On | Depended On By | Layer | Status |
   |-------|-----------|----------------|-------|--------|

   followed by a Markdown checklist with `- [ ] #N Title (blocked by #X, #Y)` entries.

If there are open PRs that should be merged before new work begins, call that out.

This is a DRAFT for human review. Do NOT create or modify any GitHub issues.
The human will provide feedback before the plan is finalised."#
    )
}

pub fn build_sprint_planning_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    format!(
        r#"You are a sprint planning assistant for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## Current State

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

## Human Feedback on the Draft

The human reviewed the draft sprint plan and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above and produce the FINAL sprint plan:

0. **Re-read upstream recommendations.** Sprint planning's primary input pool is the
   single open `strategic-review` issue's **Recommended Path Forward** section. Fetch it
   with `gh issue list --state open --label strategic-review --json number --limit 5`
   followed by `gh issue view <number>`. Pick from those recommendations; treat the open
   issues list above as supplementary context for in-flight work.
1. Adjust priorities, grouping, and scope based on the feedback.
2. Create GitHub issues for each work item using `gh issue create --title "..." --body "..."`.
   Do NOT include `Tracked by #<tracker>` yet — the tracker doesn't exist until step 3.
   The back-reference will be added by `gh issue edit` in step 4.
   **Ordering**: create all child issues first, collect their `#N` numbers, then create the tracker.
3. Create a GitHub tracker issue using:
   `gh issue create --title "Sprint: <goal>" --body "..." --label "sprint,tracker"`
   The tracker body must contain:
   - A Task Dependency Hierarchy table:

     | Issue | Depends On | Depended On By | Layer | Status |
     |-------|-----------|----------------|-------|--------|
     | #N Title | #X | #Y | 0 | 🔴 Not Started |

   - A checklist with `- [ ] #N Title (blocked by #X, #Y)` entries for each item.
4. Edit each child issue to add `Tracked by #<tracker>` in the body using
   `gh issue edit <child> --body "..."`.
5. Update ISSUES.md to add the new sprint's Task Dependency Hierarchy section. Keep existing completed sections intact.
6. Update STATUS.md if the sprint scope changes the status of any tracked feature.
7. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW."#
    )
}

fn strategic_review_context(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    format!(
        r#"## Project Context

### Crate Topology
{crate_tree}

### Recent Commits (last 30)
{recent_commits}

### Open Issues
{open_issues}

### Open Pull Requests
{open_prs}

### Project Status (STATUS.md)
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}"#
    )
}

const STRATEGIC_PERSPECTIVES: &str = r#"## Conduct the review from each perspective in turn:

### 1. Product Stakeholder
- What business value has been delivered so far?
- Where are the gaps between what exists and what users/operators need?
- What capabilities would unlock the most adoption or differentiation?
- Are there external pressures (compliance, market, ecosystem) to account for?

### 2. Business Analyst
- Are there missing user stories or acceptance criteria in open issues?
- Which requirements are implicit in the architecture but not tracked?
- What cross-cutting concerns (observability, documentation, onboarding) are under-specified?
- Draft 3-5 concrete user stories for the highest-priority gap.

### 3. Lead Engineer
- What technical debt is accumulating? Where are the architectural risks?
- Are there scalability bottlenecks or single points of failure?
- Which "Future Enhancements" listed in ISSUES.md are now urgent vs. deferrable?
- What refactoring would pay dividends across multiple future features?
- Review open PRs — are any stale, conflicting, or blocking other work?

### 4. UX / DX Researcher
- How is the developer experience for someone deploying their first app?
- What friction exists in the CLI, the manifest format, or the error messages?
- Are logs, status output, and diagnostics actionable?
- What documentation or examples are missing?"#;

#[allow(clippy::too_many_arguments)]
pub fn build_strategic_review_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    report_synthesis: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let synthesis_section = if report_synthesis.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Report Synthesis (from GitHub issue labelled `uxr-synthesis`)

The most recent UXR Synth phase produced the following synthesis (fetched from the
open `uxr-synthesis` GitHub issue). Use it as a starting point — validate, challenge,
or build on its findings. Reference the synthesis issue number when creating downstream
issues so they link back via `Depends On #<synthesis>`.

{report_synthesis}

---
"#
        )
    };
    format!(
        r#"You are a strategic review board for the {project_name} project. You will conduct a
multi-perspective analysis, role-playing the viewpoints that typically drive a product
forward, then synthesise a unified recommendation.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{synthesis_section}
{STRATEGIC_PERSPECTIVES}

---

## Synthesis

After completing all four perspectives, produce:

1. **Unified Assessment** — A 2-3 paragraph summary of where the project stands and what matters most.
2. **Recommended Path Forward** — An ordered list of 5-10 work items, each with:
   - Title (a clear, actionable headline — these are recommendation entries inside the
     single strategic-review issue body, NOT separate GitHub issues)
   - Perspective(s) driving it (Stakeholder / BA / Engineering / DX)
   - Sizing (S / M / L)
   - Brief rationale
3. **Risks & Watch Items** — Anything that could derail progress if ignored.

The finalized strategic review will be published as **exactly one** GitHub issue carrying
the `strategic-review` label — a single living strategic-direction artifact. Do not
propose a parent-tracker / child-issue layout; the recommended path forward lives as a
section inside that one issue, not as separate trackable work items. Sprint planning
consumes its own workflow and will turn these recommendations into trackable sprint
issues at that stage.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the analysis, adjust priorities, add context, or redirect focus.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_strategic_review_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    report_synthesis: &str,
    feedback: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let synthesis_section = if report_synthesis.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Report Synthesis (from GitHub issue labelled `uxr-synthesis`)

{report_synthesis}

The single strategic-review issue body MUST include
`Depends On #<synthesis-issue-number>` so it links back to the synthesis.

---
"#
        )
    };
    format!(
        r#"You are a strategic review board for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{synthesis_section}
## Human Feedback

The human reviewed the draft strategic analysis and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Adjust the recommended path forward — reprioritise,
add, remove, or reshape work items as directed.

Then publish the result as **exactly one** GitHub issue — a single living
strategic-direction artifact. Do NOT create child or recommendation issues; the
recommended path forward belongs as a section inside this single issue's body, not as
separate trackable work items. Sprint planning consumes its own workflow and will turn
these recommendations into trackable sprint issues at that stage; the strategic review
must not percolate into sprint planning as discrete tickets.

1. **Find or create the strategic review issue.** Run
   `gh issue list --state open --label "{strategic_label}" --json number,title --limit 5`
   to see if an open strategic-review issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the strategic review
     remains a single living document.
   - If none exists, create one with
     `gh issue create --title "Strategic Review: <YYYY-MM-DD> — <unified-assessment-headline>" --label "{strategic_label}"`.
     Use only the `{strategic_label}` label — do NOT add `{tracker_label}` or any
     sprint/area labels, since this issue is a strategic-direction artifact, not
     schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Unified Assessment** — Updated 2-3 paragraph summary reflecting the feedback.
   - **Recommended Path Forward** — Ordered list of 5-10 work items, each as a sub-section
     (NOT as `#N` issue refs) with: Title, Perspective(s) driving it, Sizing (S/M/L),
     Rationale, and Acceptance Criteria. These are recommendation entries, not tickets.
   - **Risks & Watch Items** — Updated risks.
   - **Dependencies** — `Depends On #<synthesis-issue-number>` linking back to the UXR
     Synthesis issue this review was built from (if one exists).
   - **Last Updated** — today's date.

3. **Do not file recommendation issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Sprint Planning.

4. **Update ISSUES.md** — Reference the single strategic-review issue. Do NOT add a
   per-recommendation Task Dependency Hierarchy here — that lives in sprint planning.
5. **Update STATUS.md** — If any new capability is being tracked, add or update the
   relevant rows.
6. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW.

This output closes the feedback loop: sprint planning will read this single issue's
"Recommended Path Forward" section and turn the items it picks into trackable sprint
issues at that stage."#,
        project_name = project_name,
        context = context,
        synthesis_section = synthesis_section,
        feedback = feedback,
        strategic_label = labels::STRATEGIC_REVIEW,
        tracker_label = labels::TRACKER,
    )
}

const ROADMAP_PHASES: &str = r#"## Create a long-term Roadmap based on the Strategic Review:

### Phase 1: Foundation (Next 1-2 Sprints)
- What critical blockers or technical debt must be addressed immediately?
- Which core features need stabilization before further expansion?

### Phase 2: Expansion (Next 2-4 Sprints)
- What primary capabilities will unlock new user segments or use cases?
- How will the system scale to handle increased load or node types?

### Phase 3: Ecosystem (Future)
- How will Freq Cloud integrate with external systems, clouds, or developer tools?
- What are the long-term extensibility and sustainability goals?"#;

#[allow(clippy::too_many_arguments)]
pub fn build_roadmapper_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    strategic_review: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let strategic_section = if strategic_review.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Strategic Review Recommendation (from GitHub issue labelled `strategic-review`)

The most recent Strategic Review produced the following analysis and recommendations (fetched from the
open `strategic-review` GitHub issue). Use it as the primary input for the Roadmap.

{strategic_review}

---
"#
        )
    };
    format!(
        r#"You are the Roadmapper for the {project_name} project. Your goal is to transform strategic
intent into a structured, long-term roadmap.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{strategic_section}
{ROADMAP_PHASES}

---

## Roadmap Output

Produce a structured roadmap that includes:

1. **Strategic Intent** — A brief (1-2 paragraph) vision statement for the next several months.
2. **Milestone Phases** — For each of the three phases defined above, provide:
   - Goals & Outcomes
   - 3-5 high-level initiatives (as a bulleted list — these are NOT separate GitHub issues,
     they are sections of the single roadmap document)
   - Success metrics

The finalized roadmap will be published as **exactly one** GitHub issue carrying the
`roadmap` label — a single common operating picture for management forecasting. Do not
propose a parent-tracker / child-issue layout; phases and initiatives live inside the
body of that one issue, not as separate trackable work items.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will review the roadmap, adjust timelines, and refine initiatives.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_roadmapper_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    strategic_review: &str,
    feedback: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let strategic_section = if strategic_review.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Strategic Review Recommendation (from GitHub issue labelled `strategic-review`)

{strategic_review}

---
"#
        )
    };
    format!(
        r#"You are the Roadmapper for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{strategic_section}

## Human Feedback

Incorporating this feedback into the final roadmap:
{feedback}

---

## Final Roadmap Execution

Your final task is to publish the roadmap as **exactly one** GitHub issue — a single
"common operating picture" for management forecasting. Do NOT create child or initiative
issues; phases and initiatives belong as sections inside this single issue's body, not as
separate trackable work items. Sprint planning consumes its own workflow; the roadmap must
not percolate into sprint planning as discrete tickets.

1. **Find or create the roadmap issue.** Run
   `gh issue list --state open --label "{roadmap_label}" --json number,title --limit 5`
   to see if an open roadmap issue already exists.
   - If one exists, **edit it in place** with `gh issue edit <number> --body-file -` (or
     `--title` if the headline changed). Reuse the same issue so the roadmap remains a
     single living document.
   - If none exists, create one with
     `gh issue create --title "Roadmap: <YYYY-MM-DD> — <headline>" --label "{roadmap_label}"`.
     Use only the `{roadmap_label}` label — do NOT add `{tracker_label}` or any sprint/area
     labels, since this issue is a strategic artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Strategic Intent** — 1-2 paragraph vision statement.
   - **Phase 1: Foundation**, **Phase 2: Expansion**, **Phase 3: Ecosystem** — each with
     Goals & Outcomes, the 3-5 initiatives as a bulleted list (NOT as `#N` issue refs),
     and Success Metrics.
   - **Dependencies** — `Depends On #<strategic-review-number>` linking back to the
     Strategic Review issue this roadmap was built from.
   - **Last Updated** — today's date.

3. **Do not file initiative issues, do not file a parent tracker issue, do not edit any
   other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Strategic Review and Sprint Planning.

Use a clear, evocative title and a structured, scannable body."#,
        project_name = project_name,
        context = context,
        strategic_section = strategic_section,
        feedback = feedback,
        tracker_label = labels::TRACKER,
        roadmap_label = labels::ROADMAP,
    )
}

pub fn build_ideation_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    format!(
        r#"You are an ideation partner for the {project_name} project. Your job is to generate
a wide, varied set of raw ideas — not to evaluate, prioritise, or structure them.
Aim for quantity and variety over quality.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Instructions

Produce at least 15 distinct ideas across these buckets:

### Capability ideas
Features users would notice — new APIs, CLI commands, dashboard panels, deployment
targets, developer workflows, or integrations.

### Foundational ideas
Infrastructure, refactors, dev-experience improvements — things that make the system
faster, more reliable, easier to develop, or cheaper to operate.

### Provocations
"What if we did the opposite?", "What if we deleted X?", contrarian or uncomfortable
ideas that challenge assumptions. These should make the reader pause.

### Wildcards
Half-formed hypotheses, analogies from other systems, things you'd normally dismiss.
Connections between unrelated domains, speculative features, "wouldn't it be cool if…"
thoughts.

## Format

For each idea: one-sentence description, one-sentence rationale.
No sizing, no commitment, no ranking. Do **not** create GitHub issues.
Do **not** filter or evaluate ideas — the human will react in feedback.

This is a DRAFT for human review. The human will keep some ideas, drop others,
expand on a few, and provide feedback before anything is finalised."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_ideation_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    feedback: &str,
    dry_run: bool,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let dry_run_note = if dry_run {
        "\n\n**DRY RUN MODE**: Do NOT actually run any `gh` commands. Instead, print the \
         exact commands you WOULD run (gh issue list, gh issue close, gh issue create) \
         with their full arguments, so the human can review what would be filed."
    } else {
        ""
    };
    format!(
        r#"You are an ideation partner for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Human Feedback

The human reviewed the ideation draft and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Keep the ideas the human endorsed, drop the ones they
rejected, and expand on any they flagged for elaboration. You may add new ideas if the
feedback suggests directions not yet covered.

Produce the FINAL ideation set, organised by bucket (Capability / Foundational /
Provocations / Wildcards). For each surviving idea: one-sentence description,
one-sentence rationale, and (if the human requested it) a short expansion paragraph.

## Publishing the Ideation as a GitHub Issue

After completing the final ideation set, publish it as a GitHub issue so it is
reviewable, durable, and consumable by downstream workflows (UXR Synth, Strategic
Review).
{dry_run_note}

### Step 1 — Close any prior open ideation issues

Run:
```
gh issue list --label ideation --state open --json number --jq '.[].number'
```

For each open issue number returned, close it with a superseded comment:
```
gh issue close <NUMBER> --comment "Superseded by the new ideation issue."
```

### Step 2 — Create the new ideation issue

Run:
```
gh issue create \
  --title "Ideation: <YYYY-MM-DD> — <one-line headline>" \
  --body "<full ideation set with all buckets and surviving ideas, plus a footer: 'Generated by Ideation agent run on <YYYY-MM-DD>.'>" \
  --label "ideation"
```

Use today's date for `<YYYY-MM-DD>`. The title headline should capture the overall
theme of the surviving ideas. The body must contain the complete final ideation set.

### Step 3 — Update the superseded comments

Go back to each issue you closed in Step 1 and update the close comment to include the
new issue number: "Superseded by #<new>."

### Step 4 — Echo the issue URL

After creating the issue, print the issue URL so it appears in the editor panel output.
Format: `Ideation published: <URL>`

Do NOT write any files to disk — the GitHub issue IS the artifact."#
    )
}

fn report_persona_lens_section(skill_paths: &crate::agent::types::SkillPaths) -> String {
    format!(
        r#"## Synthesis Lens — User Personas

Before producing any analysis, load `{skill_path}`.
This skill describes users of the platform, not contributors to the project
itself. Do NOT conflate it with other skills such as architecture,
coding standards, issue tracking, or project context, which are about building the
platform rather than using it.

For sections 2-6, tag each evidence item to the single closest persona by matching
`recognition_cues:`. Weight each finding against that persona's `jobs_to_be_done:`,
`pains:`, `adoption_yes_if:`, `rejection_no_if:`, and `anti_goals:`. If a piece of
signal matches no persona cleanly, surface it in section 7 as a possible persona blind
spot instead of forcing a weak fit.
"#,
        skill_path = skill_paths.user_personas,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_report_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    ideation: &str,
    skill_paths: &crate::agent::types::SkillPaths,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let ideation_section = if ideation.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Ideation (from GitHub issue labelled `ideation`)

The most recent Ideation phase produced the following raw ideas (fetched from the
open `ideation` GitHub issue). Use this as upstream input — **converge** these ideas
into the structured report. Pick the strongest threads, discard the noise, and explain
your filtering rationale in the Executive Summary or Recommended Next Actions.

{ideation}

---
"#
        )
    };
    let persona_lens_section = report_persona_lens_section(skill_paths);
    format!(
        r#"You are a project analyst for the {project_name} project. Produce a concise
**Strategic Report** summarising current state, progress, and recommended next actions.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{ideation_section}
{persona_lens_section}
## Report Structure

Produce the report with these sections:

### 1. Executive Summary
2-3 sentences on overall project health and momentum.

### 2. Progress Since Last Review
- What has shipped (recent commits, merged PRs)?
- Which issues were closed?
- Velocity trend: accelerating, steady, or slowing?

### 3. Current Sprint Status
- How many issues are open vs completed on active trackers?
- What percentage of the sprint is done?
- Any issues that are overdue or stalled?

### 4. Blockers & Dependencies
- Which issues are blocked and by what?
- Are there dependency chains that could cascade delays?
- External blockers (tooling, infrastructure, reviews)?

### 5. Risk Assessment
For each risk, rate severity (High/Medium/Low) and likelihood:
- Technical risks (architecture, scalability, debt)
- Delivery risks (scope creep, resource, timeline)
- Quality risks (test coverage, error handling, security)

### 6. Recommended Next Actions
Ordered list of 3-5 concrete actions with rationale. Each should be:
- Actionable within the current sprint
- Tied to a specific issue or gap identified above

### 7. Open Questions
Items that need human decision-making or clarification.

Keep the report factual and data-driven. Reference specific issue numbers and PRs.
Do NOT create any GitHub issues — this is a DRAFT for human review.
The human will review the report, adjust emphasis, correct misreadings, or add context.
Present the output clearly so they can give targeted feedback."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_report_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    ideation: &str,
    feedback: &str,
    dry_run: bool,
    skill_paths: &crate::agent::types::SkillPaths,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let dry_run_note = if dry_run {
        "\n\n**DRY RUN MODE**: Do NOT actually run any `gh` commands. Instead, print the \
         exact commands you WOULD run (gh issue list, gh issue close, gh issue create) \
         with their full arguments, so the human can review what would be filed."
    } else {
        ""
    };
    let ideation_section = if ideation.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"
## Prior Ideation (from GitHub issue labelled `ideation`)

{ideation}

When producing the synthesis, reference the strongest ideation threads and explain
which were kept and which were filtered out, and why.

---
"#
        )
    };
    let persona_lens_section = report_persona_lens_section(skill_paths);
    format!(
        r#"You are a project analyst for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---
{ideation_section}
{persona_lens_section}
## Human Feedback

The human reviewed the draft report and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback above. Adjust the report — correct any misreadings,
shift emphasis, add missing context, or reshape sections as directed.

Then produce the FINAL report with these sections:

1. **Executive Summary** — Updated to reflect the feedback.
2. **Progress Since Last Review** — Adjusted findings.
3. **Current Sprint Status** — Corrected if needed.
4. **Blockers & Dependencies** — Updated.
5. **Risk Assessment** — Re-rated if directed.
6. **Recommended Next Actions** — Reprioritised per feedback.
7. **Open Questions** — Updated.

After the full report, produce a **## Synthesis** section that distils the report into
a compact briefing suitable for feeding directly into a Strategic Review. This synthesis
should contain:
- The top 3-5 priorities with brief rationale
- Key risks and blockers that must inform strategic decisions
- Velocity assessment (one line)
- Visible persona attribution: name the dominant persona signal this cycle and call out
  any persona that appeared in zero evidence as a possible blind spot

## Publishing the Synthesis as a GitHub Issue

After completing the report, publish it as a GitHub issue so it is reviewable, durable,
and consumable by downstream workflows (Strategic Review, Sprint Planning).
{dry_run_note}

### Step 1 — Capture the list of prior open synthesis issues

Run:
```
gh issue list --label uxr-synthesis --state open --json number --jq '.[].number'
```

Save the list of issue numbers — you'll close them in Step 3 with a back-reference.

### Step 2 — Create the new synthesis issue

Run:
```
gh issue create \
  --title "UXR Synthesis: <YYYY-MM-DD> — <one-line headline>" \
  --body "<full report body including sections 1-7 and the ## Synthesis block, plus a footer: 'Generated by UXR Synth agent run on <YYYY-MM-DD>.'>" \
  --label "uxr-synthesis"
```

Use today's date for `<YYYY-MM-DD>`. The title headline should capture the single most
important finding. The body must contain the complete final report (sections 1–7) and
the Synthesis block. Capture the new issue number (`#<new>`) from the URL output.

### Step 3 — Close prior synthesis issues with a back-reference

For each issue number captured in Step 1, close it with a single comment that already
includes the new issue number — no follow-up edit needed:
```
gh issue close <NUMBER> --comment "Superseded by #<new>."
```

### Step 4 — Echo the issue URL

After creating the issue, print the issue URL so it appears in the editor panel output.
Format: `UXR synthesis published: <URL>`

Do NOT write any files to disk — the GitHub issue IS the artifact."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_retrospective_draft_prompt(
    project_name: &str,
    recent_commits: &str,
    closed_issues: &str,
    merged_prs: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"You are a sprint retrospective facilitator for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## What Happened This Cycle

### Recent Commits (last 50)
{recent_commits}

### Recently Closed Issues
{closed_issues}

### Recently Merged PRs
{merged_prs}

### Still Open Issues
{open_issues}

### Still Open PRs
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

---

## Conduct the retrospective:

### 1. What shipped
- Summarise the features, fixes, and improvements that landed.
- Were the sprint goals met? What was left incomplete and why?

### 2. What went well
- Which patterns, tools, or approaches produced good results?
- Were there any wins worth repeating (clean merges, good test coverage, fast turnarounds)?

### 3. What was painful
- Where did the process break down? Flaky tests, merge conflicts, unclear requirements?
- Were there bottlenecks — blocked issues, stale PRs, missing context?
- Did any implemented work need immediate follow-up fixes?

### 4. What to change
- Concrete process improvements for the next cycle.
- Are there recurring problems that need a systemic fix (tooling, documentation, conventions)?
- Should the sprint size, scope, or structure change?

### 5. Velocity & health
- Rough throughput: how many issues closed vs. opened?
- Is the open issue/PR backlog growing, shrinking, or stable?
- Any signs of tech debt accumulating faster than it's being addressed?

---

## Output

Produce a structured retrospective report with the five sections above.

The finalized retrospective will be published as **exactly one** GitHub issue carrying
the `retrospective` label — a single living retrospective artifact for this cycle. Do
not propose a one-issue-per-action-item layout; action items live as a checklist inside
the body of that one issue, not as separate trackable work items, so the retro does not
percolate into sprint planning as discrete tickets.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will add their own observations, correct misreadings, and highlight what matters most."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_retrospective_finalize_prompt(
    project_name: &str,
    recent_commits: &str,
    closed_issues: &str,
    merged_prs: &str,
    open_issues: &str,
    open_prs: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    format!(
        r#"You are a sprint retrospective facilitator for the {project_name} project.

Read AGENTS.md and skills/ for project conventions.

## What Happened This Cycle

### Recent Commits (last 50)
{recent_commits}

### Recently Closed Issues
{closed_issues}

### Recently Merged PRs
{merged_prs}

### Still Open Issues
{open_issues}

### Still Open PRs
{open_prs}

### Project Status
{status}

### Implementation Guidance (ISSUES.md)
{issues_md}

---

## Human Feedback on the Draft

The human reviewed the draft retrospective and provided this feedback:

{feedback}

## Instructions

Incorporate the feedback. Adjust the retrospective findings and recommendations accordingly.

Then produce the FINAL output as **exactly one** GitHub issue — a single living
retrospective artifact for this cycle. Do NOT create one issue per action item; action
items live as a checklist inside the body of this single issue, not as separate trackable
work items. Sprint planning consumes its own workflow; the retrospective must not
percolate into sprint planning as discrete tickets.

1. **Find or create the retrospective issue.** Run
   `gh issue list --state open --label "{retro_label}" --json number,title --limit 5`
   to see if an open retrospective issue already exists for the current cycle.
   - If one exists for this cycle, **edit it in place** with
     `gh issue edit <number> --body-file -` (or `--title` if the headline changed). Reuse
     the same issue so the retro remains a single living document for the cycle.
   - If none exists, create one with
     `gh issue create --title "Retro: <YYYY-MM-DD> — <headline>" --label "{retro_label}"`.
     Use only the `{retro_label}` label — do NOT add `{tracker_label}` or any sprint/area
     labels, since this issue is a reflective artifact, not schedulable work.

2. **Body structure.** The single issue body MUST contain, in order:
   - **Retrospective Report** — the five sections (What shipped, What went well, What was
     painful, What to change, Velocity & health), updated with the human's corrections
     and observations.
   - **Action Items** — a markdown checklist (`- [ ] ...`) of small, concrete process
     improvements and follow-ups, each with a one-line "definition of done". These are
     checklist items, NOT separate `#N` issue refs.
   - **Last Updated** — today's date.

3. **Do not file per-action-item issues, do not file a parent tracker issue, do not edit
   any other GitHub issue.** The output of this workflow is exactly one issue artifact.
   If the agent harness suggests a multi-issue tracker layout, ignore it — that pattern
   is reserved for Strategic Review and Sprint Planning.

4. **Update ISSUES.md** — Mark completed issues as ✅ Done in the Task Dependency
   Hierarchy tables. Reference the single retro issue, not per-item children.
5. **Update STATUS.md** — Reflect any status changes from the completed sprint work.
6. CRITICAL: ISSUES.md and GitHub issues must remain in parity. Update them NOW.

The action items inside this single issue feed directly into the next strategic review
and sprint planning cycle."#,
        project_name = project_name,
        recent_commits = recent_commits,
        closed_issues = closed_issues,
        merged_prs = merged_prs,
        open_issues = open_issues,
        open_prs = open_prs,
        status = status,
        issues_md = issues_md,
        feedback = feedback,
        retro_label = labels::RETROSPECTIVE,
        tracker_label = labels::TRACKER,
    )
}

pub fn build_code_review_prompt(
    project_name: &str,
    pr_num: u32,
    title: &str,
    body: &str,
    diff: &str,
) -> String {
    format!(
        r#"You are a code reviewer for the {project_name} project.

Read AGENTS.md and skills/ for project conventions and coding standards.

## Pull Request #{pr_num}: {title}

### Description
{body}

### Diff
```diff
{diff}
```

## Review Dimensions

1. **Correctness** — Does the code do what the PR claims? Logic errors?
2. **Security** — OWASP top 10, unsafe code, command injection, path traversal.
3. **Performance** — Unnecessary allocations, blocking in async, O(n²) where O(n) is possible.
4. **Style** — Consistency with project conventions in skills/.
5. **Tests** — Adequately tested? Edge cases covered?
6. **Memory** — Idle memory under 10MB — flag any unnecessary allocations.

For each finding, capture:
- `path` — file relative to repo root
- `line` — line number in the **new** version of the file (RIGHT side of the diff)
- `severity` — critical / warning / nit
- `body` — markdown explanation with the suggested fix, prefixed with the severity tag (e.g. `**[warning]** ...`)

## Posting the Review

Submit the review as a **single** REST call via `gh api`. This posts the
verdict and all inline comments atomically. Do NOT use `gh pr review` —
it cannot attach line-anchored comments, and that is the whole point of
this workflow.

### Step 1 — Resolve repo + head SHA
```sh
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
HEAD_SHA=$(gh pr view {pr_num} --json headRefOid -q .headRefOid)
```

### Step 2 — Pick the verdict
- **APPROVE** — zero findings worth flagging.
- **REQUEST_CHANGES** — at least one critical or warning finding.
- **COMMENT** — only nits, or non-actionable observations.

Your `gh` commands run under the bot account identity (GH_TOKEN is set
for you), so same-author rules do not apply. Do NOT downgrade
REQUEST_CHANGES to COMMENT to work around restrictions you do not have.

### Step 3 — Build the payload and POST it
Write the JSON to a temp file, then submit it via `--input`:

```sh
cat > /tmp/review-{pr_num}.json <<'JSON'
{{
  "commit_id": "<HEAD_SHA>",
  "event": "APPROVE | REQUEST_CHANGES | COMMENT",
  "body": "<top-level summary, 1-3 sentences>",
  "comments": [
    {{
      "path": "<file path>",
      "line": <new-version line number>,
      "side": "RIGHT",
      "body": "**[severity]** <markdown finding with suggested fix>"
    }}
  ]
}}
JSON

gh api -X POST \
  -H "Accept: application/vnd.github+json" \
  "repos/$REPO/pulls/{pr_num}/reviews" \
  --input /tmp/review-{pr_num}.json
```

Constraints the API enforces — respect them or you will get HTTP 422:
- `line` MUST refer to a line that is actually present in the diff hunks
  for that file. Anchoring to an unchanged line outside the hunks is rejected.
- For multi-line findings, include both `start_line` and `line`, both
  using `side: "RIGHT"`.
- An empty `comments` array is allowed — that turns the call into a
  verdict-only review (use this for a clean APPROVE).
- If `gh api` returns non-2xx, surface the response body verbatim and
  stop. Do not retry blindly and do not fall back to `gh pr review`.

### Step 4 — Confirm and stop
On success, log the `html_url` from the response and exit. Do NOT also
run `gh pr review` or `gh pr comment` — the inline comments and verdict
are already posted in the single call above."#
    )
}

/// Narrow review: verify whether the PR resolves outstanding bot-authored
/// review threads. Prefer **APPROVE** or **REQUEST_CHANGES** only — avoid a
/// full green-field audit when [`fetch_unresolved_review_threads`] returned
/// comments to verify.
pub fn build_review_followup_code_review_prompt(
    project_name: &str,
    pr_num: u32,
    title: &str,
    body: &str,
    diff: &str,
    threads: &[ReviewThread],
) -> String {
    let mut threads_section = String::new();
    for (i, t) in threads.iter().enumerate() {
        threads_section.push_str(&format!(
            "### Thread {i} — `{path}:{line}` (by @{author})\n\n{body}\n\n",
            i = i + 1,
            path = t.path,
            line = t.line,
            author = t.author,
            body = t.body,
        ));
    }
    let thread_count = threads.len();

    format!(
        r#"You are performing a **follow-up verification review** on pull request #{pr_num} for the {project_name} project.

This is **not** a full code review. The automated reviewer previously left **{thread_count}** unresolved thread(s) below. Your only job is to decide whether the **current diff** adequately addresses those concerns, and to either approve or request further changes.

Read AGENTS.md and skills/ for project conventions.

## Pull Request #{pr_num}: {title}

### Description
{body}

### Diff
```diff
{diff}
```

## Outstanding review threads (must verify)

{threads_section}

## Instructions

1. **Scope** — Judge whether each thread is addressed in the post-change code. Do **not** perform a broad style pass, hunt for nits in unrelated files, or re-review the entire PR as if it were new.
2. **New problems** — If the fix introduces a **critical** or **warning**-level regression in touched code, include it via **REQUEST_CHANGES** with the same line-anchored REST payload as a normal review.
3. **Verdict** — If every thread is satisfactorily resolved and there is no new blocking issue: submit **APPROVE** (comments array may be empty). If anything material remains: submit **REQUEST_CHANGES** with inline comments anchored to the **new** file lines (`side: RIGHT`).

## Posting the Review

Use the **same** single REST `POST repos/{{owner}}/{{repo}}/pulls/{pr_num}/reviews` pattern as a full caretta review. Do NOT use `gh pr review` for inline comments.

### Step 1 — Resolve repo + head SHA
```sh
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
HEAD_SHA=$(gh pr view {pr_num} --json headRefOid -q .headRefOid)
```

### Step 2 — Verdict
- **APPROVE** — all threads above are addressed; no new blocking issues in scope.
- **REQUEST_CHANGES** — at least one thread remains inadequately fixed, or a new blocking regression appeared.

### Step 3 — POST payload
Write JSON to a temp file and submit:

```sh
cat > /tmp/review-followup-{pr_num}.json <<'JSON'
{{
  "commit_id": "<HEAD_SHA>",
  "event": "APPROVE | REQUEST_CHANGES",
  "body": "<1-3 sentence summary>",
  "comments": [
    {{
      "path": "<file path>",
      "line": <new-version line number>,
      "side": "RIGHT",
      "body": "**[severity]** …"
    }}
  ]
}}
JSON

gh api -X POST \
  -H "Accept: application/vnd.github+json" \
  "repos/$REPO/pulls/{pr_num}/reviews" \
  --input /tmp/review-followup-{pr_num}.json
```

Constraints: `line` MUST fall inside diff hunks. On HTTP errors, print the response body and stop.

### Step 4 — Confirm
On success, log the review `html_url` from the response. Do NOT also run `gh pr review`."#
    )
}

pub fn build_security_review_prompt(
    project_name: &str,
    crate_tree: &str,
    snapshot: &str,
    dry_run: bool,
) -> String {
    let snapshot_section = if snapshot.is_empty() {
        "Read the codebase directly using the tools available to you. Start with AGENTS.md, \
         skills/, then systematically review each crate under crates/."
            .to_string()
    } else {
        format!(
            "## Codebase Snapshot\n\n\
             The following is a cleaned snapshot of the project. Use this as your primary reference.\n\n\
             {snapshot}"
        )
    };

    format!(
        r#"You are a security auditor performing a comprehensive security-focused code review
of the {project_name} project.

Read AGENTS.md and skills/ for full project context and coding standards.

## Project Crates
```
{crate_tree}
```

{snapshot_section}

---

## Security Review Scope

Perform a thorough static security analysis covering ALL of the following areas:

### 1. OWASP Top 10
- **Injection** — SQL injection, command injection, code injection in any ops or handlers.
- **XSS** — Cross-site scripting in any HTML output, SSE streams, or dashboard rendering.
- **SSRF** — Server-side request forgery in fetch ops, proxy handlers, or service discovery.
- **Path traversal** — Verify all filesystem operations reject `..`, leading `/`, null bytes.
- **Broken authentication** — Weak token generation, missing auth checks on endpoints.
- **Broken access control** — RBAC bypass, privilege escalation, missing authorization gates.
- **Security misconfiguration** — Default credentials, overly permissive CORS, debug endpoints.
- **Insecure deserialization** — Unsafe deserialization of untrusted input.
- **Insufficient logging** — Missing audit trails for security-sensitive operations.
- **CSRF** — Missing CSRF protections on state-changing endpoints.

### 2. Authentication & Authorization
- Are all management API endpoints properly gated (API key, RBAC)?
- Is session/token handling secure (timing-safe comparison, proper expiry)?
- Are password hashing parameters adequate (Argon2id tuning)?
- Any endpoints reachable without authentication that should require it?

### 3. Secrets Handling
- Hardcoded keys, tokens, or credentials anywhere in the codebase?
- Are secrets properly encrypted at rest (ChaCha20-Poly1305)?
- Could secrets leak into logs, error messages, or SSE streams?
- Is `ANTHROPIC_API_KEY` / other API keys handled safely?

### 4. Sandbox Escape Vectors
- **V8 isolate** — Can tenant code escape the sandbox via deno_core ops?
- **Filesystem** — Are read/write ops properly scoped to tenant directories?
- **Network** — Can tenant code access internal services or private IPs?
- **Environment** — Can tenant code read host environment variables beyond allowed scope?
- **Resource exhaustion** — Can a tenant exhaust memory, CPU, disk, or file descriptors?

### 5. Wire Protocol Weaknesses
- **Replay attacks** — Is nonce-based replay protection correctly implemented?
- **Spoofing** — Can a node impersonate another in the mesh?
- **DoS** — Can malformed or oversized messages crash a node?
- **Key exchange** — Is the ML-KEM-768 handshake correctly implemented?
- **Message integrity** — Are all messages authenticated (AEAD)?

### 6. Dependency Vulnerabilities
- Review `Cargo.toml` for known vulnerable crate versions.
- Check for dependencies with known CVEs.
- Flag any unnecessary dependencies that expand the attack surface.

### 7. Unsafe Rust Usage
- Audit all `unsafe` blocks for soundness.
- Check for undefined behavior, data races, or memory corruption.
- Verify safety invariants are documented and upheld.

---

## Output Format

For each finding, produce a structured entry:

### [SEVERITY] Title
- **Severity**: Critical / High / Medium / Low / Informational
- **Category**: (e.g., OWASP-A01, Sandbox Escape, Wire Protocol, etc.)
- **Location**: `crate/file.rs:line_range`
- **Description**: What the vulnerability is and how it could be exploited.
- **Impact**: What an attacker could achieve.
- **Remediation**: Specific code changes or mitigations to fix it.

---

## Summary

After all findings, produce:

1. **Executive Summary** — 2-3 sentences on overall security posture.
2. **Finding Count** — Table of findings by severity (Critical/High/Medium/Low/Info).
3. **Top 3 Priority Fixes** — The most impactful issues to address first.
4. **Positive Observations** — Security practices that are already well-implemented.

Be thorough but avoid false positives. Only flag real, actionable issues.

## Issue Creation

After the review is complete, file the results as GitHub issues:

### Duplicate Detection

Before creating any issue, check for an existing open issue with the same title:
```
gh issue list --label security --search "<finding title>" --state open
```
If a matching open issue already exists, skip creating it and note "Already tracked: #<N>" in the summary.

### Actionable Findings (Critical / High / Medium)

For each actionable finding (Critical, High, or Medium severity), create a GitHub issue with a severity label:
```
gh issue create \
  --title "security: [SEVERITY] <finding title>" \
  --body "<severity, category, location, description, impact, remediation>" \
  --label "security,code-review,severity:<severity_lowercase>"
```
Where `severity:<severity_lowercase>` is one of `severity:critical`, `severity:high`, or `severity:medium`.

**Ordering**: create all per-finding issues first, collect their `#N` numbers, then create the tracker.

### Low / Informational Findings

Low and Informational findings should be batched into a single rollup issue:
```
gh issue create \
  --title "security: Low/Info findings rollup — <YYYY-MM-DD>" \
  --body "<list of all Low and Info findings with severity, category, location, description>" \
  --label "security,code-review,severity:low"
```
If there are no Low/Info findings, skip this step.

### Tracker Issue

After all finding issues (including the rollup) are created, create a tracker issue:
```
gh issue create \
  --title "Security Review: <YYYY-MM-DD> — <executive-summary-headline>" \
  --body "..." \
  --label "security,tracker"
```
The tracker body must contain:
- The executive summary
- The finding count table (Critical/High/Medium/Low/Info)
- A checklist with `- [ ] #N <finding title>` entries for each child issue (including the rollup)
- The top 3 priority fixes

### Link Children to Tracker

Edit each child issue to add `Tracked by #<tracker>` in the body using `gh issue edit <child> --body "..."`.

### Cross-Reference Summary

After all issues are filed, output a final summary line in this exact format:
```
Filed: #<N1>, #<N2>, ... (tracker: #<T>)
```
This allows the human reviewer to audit the created issues at a glance.{dry_run_section}"#,
        dry_run_section = if dry_run {
            "\n\n## DRY RUN MODE\n\n\
             **IMPORTANT**: This is a dry-run. Do NOT execute any `gh issue create` or `gh issue edit` commands.\n\
             Instead, for each issue you would create, output the full `gh` command you would have run, prefixed with `[dry-run]`.\n\
             Still perform the full security analysis and duplicate detection, but only print what would be filed."
        } else {
            ""
        }
    )
}

pub fn build_refresh_agents_prompt(project_name: &str, agent_files: &[String]) -> String {
    let file_list = if agent_files.is_empty() {
        "- `AGENTS.md`".to_string()
    } else {
        agent_files
            .iter()
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"You are refreshing agent-facing documentation for the {project_name} project.

Read AGENTS.md and the listed skill files before making any edits.

## Allowed Files

You may edit ONLY the following existing agent-facing files:
{file_list}

Do NOT edit source code, tests, Cargo manifests, README files, STATUS.md, ISSUES.md, or any
other project files. Do NOT create new skills from scratch.

## Refresh Objective

Review each allowed file against the current repository state and update only documentation drift.
For every stale claim you change, confirm the real repo state first.

At a minimum, verify:
- referenced file paths still exist
- referenced scripts still exist and still match the described purpose
- referenced crates, macros, ops, and APIs still exist with the described names/shapes
- referenced docs still exist and the cited filenames/sections still line up

## Execution Rules

1. Inspect the repo directly with read-only commands before editing.
2. Edit only the allowed files above.
3. Do NOT commit, push, or open a pull request. The shell will handle git and PR creation.
4. If nothing drifted, leave the worktree unchanged.

## Final Output Contract

After you finish, output exactly one of the following:

### If you made no edits
`REFRESH_AGENTS_NO_CHANGES`

### If you edited files
Emit this exact block:

```
REFRESH_AGENTS_SUMMARY_BEGIN
path/to/file | one-line reason for the edit
path/to/other/file | one-line reason for the edit
REFRESH_AGENTS_SUMMARY_END
```

Requirements for each reason:
- one line only
- describe the specific drift you corrected
- mention the repo fact that forced the update

Do not include files outside the allowed list in that summary block."#
    )
}

// ── Refresh Docs (one-shot) ──

pub fn build_refresh_docs_prompt(project_name: &str, doc_files: &[String]) -> String {
    let file_list = if doc_files.is_empty() {
        "- `README.md`".to_string()
    } else {
        doc_files
            .iter()
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"You are refreshing project documentation for the {project_name} project.

Read the listed files and compare each claim against the current repository state.

## Allowed Files

You may edit ONLY the following existing project documentation files:
{file_list}

Do NOT edit source code, tests, Cargo manifests, agent-facing files (AGENTS.md, skills/**,
CLAUDE.md, CLINE.md, GEMINI.md, COPILOT.md, GROK.md, JUNIE.md, XAI.md), or any other non-documentation files.

## Refresh Objective

Review each allowed file against the current repository state and update only documentation drift.
For every stale claim you change, confirm the real repo state first.

At a minimum, verify:
- referenced crates, binaries, and scripts still exist with the described shapes
- code snippets and command examples still compile / run
- feature lists and architectural descriptions match the current node types and crate layout
- STATUS.md and ISSUES.md reflect the current state of trackers and open work
  (tracker parity: every documented tracker should match what `gh issue list --label tracker` returns)
- the documented Dev UI workflow inventory matches the actual shipped workflows in
  `crates/dev/src/agent/types.rs::Workflow` and the sidebar buttons in
  `crates/dev/src/ui/sidebar.rs` — no missing workflows, no leftover renamed names

## Execution Rules

1. Inspect the repo directly with read-only commands before editing.
2. Edit only the allowed files above.
3. Do NOT commit, push, or open a pull request. The shell will handle git and PR creation.
4. If nothing drifted, leave the worktree unchanged.

## Final Output Contract

After you finish, output exactly one of the following:

### If you made no edits
`REFRESH_DOCS_NO_CHANGES`

### If you edited files
Emit this exact block:

```
REFRESH_DOCS_SUMMARY_BEGIN
path/to/file | one-line reason for the edit
path/to/other/file | one-line reason for the edit
REFRESH_DOCS_SUMMARY_END
```

Requirements for each reason:
- one line only
- describe the specific drift you corrected
- mention the repo fact that forced the update

Do not include files outside the allowed list in that summary block."#
    )
}

// ── Housekeeping (two-phase) ──

/// Build context string from pre-gathered housekeeping data.
fn housekeeping_context(
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
) -> String {
    format!(
        r#"## Project Context

### Open Issues (JSON)
{open_issues}

### Open Pull Requests (JSON)
{open_prs}

### Local Branches
{local_branches}

### Tracker Issue Bodies
{tracker_bodies}

### STATUS.md
{status}

### ISSUES.md
{issues_md}"#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_housekeeping_draft_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
) -> String {
    let context = housekeeping_context(
        open_issues,
        open_prs,
        local_branches,
        tracker_bodies,
        status,
        issues_md,
    );
    format!(
        r#"You are a housekeeping agent for the {project_name} project. Your job is to audit
the project for orphaned, stale, and drifted artifacts and produce a structured report.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Sweep Categories

Run ALL of the following sweeps. For each finding, report:
- **Kind**: the sweep category (1-7)
- **Target**: the specific artifact (issue/PR/branch/file/label)
- **Age**: how long since last activity
- **Suggested action**: what to do about it
- **Severity**: Critical / High / Medium / Low / Info

### 1. Tracker Drift (highest priority)

- **Closed children not checked off**: tracker checklist shows `- [ ]` for an issue
  that is `state:closed`. Run `gh issue view <N> --json state` for each unchecked item.
  Suggested fix: tick the box.
- **Trackers with all children closed but tracker still open**: propose closing the tracker.
- **Tracker children that no longer exist**: an issue number in the checklist that 404s.
  Propose removing the line.
- **Orphan child references**: issue body says `Tracked by #N` where #N is closed or
  doesn't exist. Propose removing the back-reference.
- **Layer ordering inconsistencies**: child issue's `Depends On` includes a closed issue
  (no longer blocking). Propose downgrading the dependency.

### 2. Stale Issues

- Open issues with **zero activity** (no commits, comments, label changes, assignee
  changes) for **>60 days**. Use `gh issue view <N> --json updatedAt,assignees,labels`
  to check. Surface each with: last-activity timestamp, assignee, label set, linked PRs.
- Open issues whose `Blocked by #X` references a closed issue — the blocker is gone,
  they're actually unblocked. Propose removing the blocked-by line.
- Open issues with `wontfix` / `duplicate` / `invalid` labels still in `open` state —
  propose closing.

### 3. Stale Pull Requests

- PRs in `open` state with no commits, no review activity, no comments for **>14 days**.
  Use `gh pr view <N> --json updatedAt,author,headRefName,mergeable`. Surface with:
  author, branch, last activity, conflict status.
- PRs whose `Closes #N` references an already-closed issue. The PR is doing nothing.
- PRs from `agent/issue-N` branches where issue #N is closed without merge.
  Propose closing the PR + deleting the branch.

### 4. Orphaned Local Branches

- Local branches matching `agent/issue-N` for issues that are `state:closed`.
  Safe to delete after confirming no uncommitted changes.
- Local branches matching `agent/issue-N` with no remote tracking and no recent commits.
- **NEVER** propose auto-deleting unmerged branches. Surface for human approval with
  last commit metadata.

### 5. Generated / Orphaned Files

- `REPORT_SYNTHESIS.md` at the project root — if found, propose deletion.
- Files matching `.agent-tmp-*` or similar agent scratchpads.
- Other generated artifacts (e.g., `prompt.md`, `embeddings.json`).

### 6. Label Taxonomy Drift

- Labels referenced in `crates/dev/src/agent/tracker.rs` or in AGENTS.md but **not present**
  in the repo. Run `gh label list --json name --limit 200` to get current labels. Propose
  `gh label create`.
- Labels present in the repo but not referenced anywhere in the codebase. Surface for review.
- Labels with zero open issues attached and last applied >90 days ago.

### 7. ISSUES.md / STATUS.md Drift

- Entries in `ISSUES.md` Task Dependency Hierarchy tables whose status disagrees with
  the actual GitHub issue state (e.g. table says 🔴 Not Started but issue is closed).
- `STATUS.md` rows referencing capabilities whose tracking issue is closed without the
  row being updated.

---

## Output Format

Group findings by category. Within each category, sort by severity (Critical first).
Use this structure:

```
## Category N: <Name>

### [SEVERITY] <Target>
- **Kind**: <sweep category>
- **Target**: <artifact identifier>
- **Age**: <days since last activity>
- **Suggested action**: <what to do>
- **Details**: <any additional context>
```

After all categories, produce:

## Summary
- Total findings by severity
- Top 3 most impactful cleanups
- Estimated effort (low/medium/high) for each suggested action

**IMPORTANT**: Do NOT modify anything. This is a READ-ONLY audit.
Do NOT run `gh issue close`, `gh issue edit`, `git branch -d`, or any mutating commands.
Only run read commands (`gh issue view`, `gh issue list`, `gh pr list`, `git branch`, etc.)
to gather data for the report."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_housekeeping_finalize_prompt(
    project_name: &str,
    open_issues: &str,
    open_prs: &str,
    local_branches: &str,
    tracker_bodies: &str,
    status: &str,
    issues_md: &str,
    feedback: &str,
) -> String {
    let context = housekeeping_context(
        open_issues,
        open_prs,
        local_branches,
        tracker_bodies,
        status,
        issues_md,
    );
    format!(
        r#"You are a housekeeping agent for the {project_name} project.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Human Feedback

The human reviewed the housekeeping draft and provided this feedback:

{feedback}

## Instructions

Execute ONLY the cleanup actions the human approved. The feedback may say things like:
- "Skip section X entirely"
- "Fix the tracker drift but skip the stale-PR section"
- "Delete REPORT_SYNTHESIS.md but keep the agent branches for now"
- "All clear, go ahead"

Respect the feedback precisely. If the human skipped a section, do not touch it.

## Execution Order

Execute approved cleanups in this order (lowest-risk first):

1. **Tick tracker checkboxes** — Update tracker issue bodies to check off closed children.
   Use `gh issue edit <tracker> --body "<updated body>"`.
2. **Remove stale references** — Edit issue bodies to remove dead `Blocked by` or
   `Tracked by` references.
3. **Close stale issues** — Close issues with `wontfix`/`duplicate`/`invalid` labels.
   Use `gh issue close <N> --comment "Closed by housekeeping: <reason>"`.
4. **Close stale PRs** — Close PRs that reference closed issues or abandoned branches.
   Use `gh pr close <N> --comment "Closed by housekeeping: <reason>"`.
5. **Delete orphaned branches** — Delete local branches for closed issues.
   Use `git branch -d <branch>` (safe delete only).
   **NEVER delete unmerged branches** even if the human approved it — surface again
   with a warning: "Branch <name> has unmerged commits. Skipping deletion for safety.
   Use `git branch -D <name>` manually if you are sure."
6. **Delete generated files** — Remove orphaned generated files.
7. **Create missing labels** — Use `gh label create <name> --color <hex>`.
8. **Close completed trackers** — Close trackers where all children are done.

## Audit Trail

After executing all approved actions, file a `housekeeping` GitHub issue summarising
what was done:

### Step 1 — Close any prior open housekeeping issues

Run:
```
gh issue list --label housekeeping --state open --json number --jq '.[].number'
```

For each open issue number returned, close it with:
```
gh issue close <NUMBER> --comment "Superseded by the new housekeeping run."
```

### Step 2 — Create the new housekeeping issue

Run:
```
gh issue create \
  --title "Housekeeping: <YYYY-MM-DD> — <one-line summary>" \
  --body "<structured report of all actions taken, grouped by category>" \
  --label "housekeeping"
```

If tracker-related actions were taken, also add the `tracker` label:
```
--label "housekeeping,tracker"
```

### Step 3 — Echo the issue URL

Print the new issue URL so it appears in the editor panel.
Format: `Housekeeping complete: <URL>`"#
    )
}

// ── Interview prompts ──

pub fn build_interview_draft_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    format!(
        r#"You are an interview facilitator for a software project. Your job is to conduct
a structured discovery interview with the project maintainer to surface the gap
between what currently exists and what the user intends.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Instructions

Analyze the project state above — code, issues, PRs, and commit history — then ask
**directed, project-specific questions**. Do NOT use generic templates; every question
must reference concrete artifacts you observed (specific issues, code patterns, PRs,
missing tests, architectural gaps, etc.).

Organize your questions under these section headers (use exactly these headings):

### Intent vs. Current State
Ask about gaps between what exists and what the user intends. Reference specific
issues, code areas, or patterns that seem incomplete or misaligned.

### Priority and Sequencing
Ask what matters most and what should come first. Reference competing priorities
you detected (e.g. open issues that pull in different directions).

### Scope Boundaries
Ask what is in scope and what is out. Reference features or ideas that seem
ambitious or unclear in their boundaries.

### Open Questions
Surface unresolved decisions or tensions you detected in the codebase or issue
tracker. Ask the user to weigh in.

## Format

- Use the exact section headings above (### level)
- Ask 2-3 focused questions per section
- Each question should be concrete and reference specific project artifacts
- Keep questions concise — one or two sentences each
- Number your questions within each section

This is round 1 of an interactive interview. The user will answer, and you will
ask follow-up questions in round 2."#
    )
}

pub fn build_interview_followup_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    prior_answers: &[String],
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let answers_section: String = prior_answers
        .iter()
        .enumerate()
        .map(|(i, a)| format!("### Round {} response\n\n{a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        r#"You are an interview facilitator for a software project, conducting round {round}
of a structured discovery interview.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Prior Interview Responses

{answers_section}

---

## Instructions

Based on the user's prior answers, ask **targeted follow-up questions** that dig
deeper into areas where:
- Answers were ambiguous or revealed new tensions
- Important details were missing
- Priorities or scope need further clarification
- You detected contradictions between answers and the project state

Organize follow-ups under the same section headings:

### Intent vs. Current State
### Priority and Sequencing
### Scope Boundaries
### Open Questions

Only include sections where you have meaningful follow-ups. Skip sections where
the user's answers were already clear and complete.

## Format

- 1-3 follow-up questions per section (only where needed)
- Reference the user's specific answers when asking follow-ups
- Keep questions concrete and actionable

This is round {round} of the interview. The user will answer, then you will
generate a final summary."#,
        round = prior_answers.len() + 1
    )
}

pub fn build_interview_summary_prompt(
    open_issues: &str,
    open_prs: &str,
    recent_commits: &str,
    status: &str,
    issues_md: &str,
    crate_tree: &str,
    all_answers: &[String],
) -> String {
    let context = strategic_review_context(
        open_issues,
        open_prs,
        recent_commits,
        status,
        issues_md,
        crate_tree,
    );
    let answers_section: String = all_answers
        .iter()
        .enumerate()
        .map(|(i, a)| format!("### Round {} response\n\n{a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        r#"You are an interview facilitator for a software project. You have completed a
multi-round discovery interview. Now generate the structured summary.

Read AGENTS.md, skills/, STATUS.md, and ISSUES.md for full project context.

{context}

---

## Interview Responses

{answers_section}

---

## Instructions

Synthesize all interview responses into a structured summary document. This summary
will be consumed by other agent workflows for downstream planning.

## Output Format

Produce a Markdown document with these sections:

### Vision & Intent
One paragraph distilling the user's intended direction for the project.

### Priorities (ordered)
A numbered list of priorities in the order the user specified, with brief rationale
for each.

### Scope Boundaries
Two subsections:
- **In scope**: What the user confirmed is in scope
- **Out of scope**: What the user explicitly excluded or deferred

### Key Decisions
Bullet list of decisions made during the interview, referencing specific issues
or code areas where applicable.

### Open Items
Anything still unresolved — questions the user deferred, tensions that remain,
or areas needing further investigation.

### Recommended Next Actions
3-5 concrete next steps derived from the interview, each tied to a specific
issue, PR, or code area.

Do NOT create GitHub issues. Output only the summary document.
End with the line: `---interview-summary-complete---`"#
    )
}
