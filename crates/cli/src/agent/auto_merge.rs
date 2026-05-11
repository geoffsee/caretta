//! Lineage-aware batch merge for `agent/issue-N` stacked pull requests.

use crate::agent::cmd::{cmd_capture, cmd_run, cmd_stdout, cmd_stdout_or_die, has_command};
use crate::agent::conflicts::CONFLICT_RESOLUTION_MARKER;
use crate::agent::shell::log;
use crate::agent::tracker::{
    enable_auto_merge, find_tracker, find_upstream_branch, get_tracker_body, is_auto_merge_enabled,
    parse_pending, pending_issues_execution_order,
};
use crate::agent::types::{BRANCH_PREFIX, Config};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GhPrMergeRow {
    number: u32,
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    is_draft: bool,
    merge_state_status: Option<String>,
    review_decision: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrStatusRefresh {
    merge_state_status: Option<String>,
    review_decision: Option<String>,
    is_draft: bool,
}

fn parse_gh_pr_merge_rows(raw: &str) -> Vec<GhPrMergeRow> {
    match serde_json::from_str(raw) {
        Ok(rows) => rows,
        Err(err) => {
            log(&format!(
                "auto-merge (lineage): failed to parse `gh pr list` output: {err}"
            ));
            Vec::new()
        }
    }
}

fn gh_list_merge_candidate_prs() -> Vec<GhPrMergeRow> {
    let out = cmd_stdout_or_die(
        "gh",
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "150",
            "--json",
            "number,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision",
        ],
        "failed to list open PRs for auto-merge",
    );
    parse_gh_pr_merge_rows(&out)
}

fn issue_num_from_agent_head(branch: &str) -> Option<u32> {
    branch
        .strip_prefix(BRANCH_PREFIX)
        .and_then(|s| s.parse::<u32>().ok())
}

/// Open PR rows whose heads follow `agent/issue-N`. Drafts omitted.
pub(crate) fn agent_issue_pull_rows(rows: &[GhPrMergeRow]) -> HashMap<u32, GhPrMergeRow> {
    let mut map = HashMap::new();
    for row in rows {
        if row.is_draft {
            continue;
        }
        if let Some(n) = issue_num_from_agent_head(&row.head_ref) {
            map.insert(n, row.clone());
        }
    }
    map
}

/// Same ordering policy as sprint workers: tracker execution order narrowed to rows that still have PRs.
pub fn tracker_merge_candidates_order(body: &str, pr_issues: &HashSet<u32>) -> Vec<u32> {
    pending_issues_execution_order(body)
        .into_iter()
        .filter(|n| pr_issues.contains(n))
        .collect()
}

pub fn tracker_from_env() -> Option<u32> {
    std::env::var("CARETTA_MERGE_TRACKER")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
}

fn merge_order_tracker(pr_by_issue: &HashMap<u32, GhPrMergeRow>, tracker_id: u32) -> Vec<u32> {
    let body = get_tracker_body(tracker_id);
    let keys: HashSet<u32> = pr_by_issue.keys().copied().collect();
    tracker_merge_candidates_order(&body, &keys)
}

fn append_missing_topological_order(
    mut order: Vec<u32>,
    pr_by_issue: &HashMap<u32, GhPrMergeRow>,
    trunk: &str,
) -> Vec<u32> {
    let mut seen: HashSet<u32> = order.iter().copied().collect();
    for issue in merge_order_topological(pr_by_issue, trunk) {
        if seen.insert(issue) {
            order.push(issue);
        }
    }
    order
}

pub(crate) fn merge_order_topological(
    pr_by_issue: &HashMap<u32, GhPrMergeRow>,
    trunk: &str,
) -> Vec<u32> {
    if pr_by_issue.is_empty() {
        return Vec::new();
    }

    let head_owner: HashMap<&str, u32> = pr_by_issue
        .iter()
        .map(|(&issue, row)| (row.head_ref.as_str(), issue))
        .collect();

    #[derive(Clone, Copy)]
    struct IssueNode {
        parent_issue: Option<u32>,
        issue: u32,
    }

    let rows: Vec<u32> = pr_by_issue.keys().copied().collect();
    let mut nodes = Vec::<IssueNode>::new();
    for issue in rows.iter() {
        let row = pr_by_issue.get(issue).expect("consistent map");
        let parent = if row.base_ref == trunk {
            None
        } else if let Some(&p) = head_owner.get(row.base_ref.as_str()) {
            Some(p)
        } else {
            None
        };
        nodes.push(IssueNode {
            parent_issue: parent,
            issue: *issue,
        });
    }

    let children: HashMap<Option<u32>, Vec<u32>> = {
        let mut m: HashMap<Option<u32>, Vec<u32>> = HashMap::new();
        for n in &nodes {
            m.entry(n.parent_issue).or_default().push(n.issue);
        }
        for v in m.values_mut() {
            v.sort_unstable();
        }
        m
    };

    let mut out = Vec::<u32>::new();
    let mut seen = HashSet::<u32>::new();
    let mut q: VecDeque<u32> = VecDeque::new();
    if let Some(roots) = children.get(&None) {
        for &r in roots {
            q.push_back(r);
        }
    }

    while let Some(issue) = q.pop_front() {
        if !seen.insert(issue) {
            continue;
        }
        out.push(issue);
        let key = Some(issue);
        if let Some(kids) = children.get(&key) {
            for &k in kids {
                q.push_back(k);
            }
        }
    }

    let mut orphans: Vec<u32> = nodes
        .iter()
        .filter(|n| !seen.contains(&n.issue))
        .map(|n| n.issue)
        .collect();
    orphans.sort_unstable();
    for o in orphans {
        if seen.insert(o) {
            log(&format!(
                "auto-merge (lineage): issue #{o} stacks on '{}' which is not trunk nor another queued agent branch — appending deterministically behind the main lineage slice",
                pr_by_issue
                    .get(&o)
                    .map(|r| r.base_ref.as_str())
                    .unwrap_or("?"),
            ));
            out.push(o);
        }
    }

    out
}

fn eligible_for_immediate_merge(row: &GhPrMergeRow) -> bool {
    if row.is_draft {
        return false;
    }
    if matches!(
        row.merge_state_status.as_deref(),
        Some(s) if s.eq_ignore_ascii_case("DIRTY")
    ) {
        return false;
    }
    matches!(
        row.review_decision.as_deref(),
        Some(d) if d.trim().eq_ignore_ascii_case("APPROVED")
    )
}

/// Approved, non-draft rows — may still be `DIRTY` until `gh pr update-branch`
/// merges the latest base.
fn eligible_for_automerge_queue(row: &GhPrMergeRow) -> bool {
    !row.is_draft
        && matches!(
            row.review_decision.as_deref(),
            Some(d) if d.trim().eq_ignore_ascii_case("APPROVED")
        )
}

fn is_dirty(row: &GhPrMergeRow) -> bool {
    matches!(
        row.merge_state_status.as_deref(),
        Some(s) if s.eq_ignore_ascii_case("DIRTY")
    )
}

#[derive(Clone, Copy, Debug)]
enum MergePassMode {
    SquashMergeWhenEligible,
    UpdateBranchThenAutomergeQueue,
    SyncBranchesOnly,
}

fn pr_update_branch(pr_num: u32, dry_run: bool) -> bool {
    if dry_run {
        log(&format!(
            "[dry-run] Would update PR #{pr_num} with `gh pr update-branch`."
        ));
        return true;
    }
    log(&format!(
        "Merging latest base into PR #{pr_num} (`gh pr update-branch`)…"
    ));
    let (ok, out) = cmd_capture("gh", &["pr", "update-branch", &pr_num.to_string()]);
    if !ok {
        log(&format!(
            "`gh pr update-branch` failed for PR #{pr_num}: {out}"
        ));
    }
    ok
}

fn retarget_pull_base(pr_num: u32, new_base: &str, dry_run: bool) -> bool {
    if dry_run {
        log(&format!(
            "[dry-run] Would retarget PR #{pr_num}: gh pr edit … --base {new_base}",
        ));
        return true;
    }
    log(&format!(
        "Retargeting PR #{pr_num} to merge into '{new_base}'…"
    ));
    cmd_run(
        "gh",
        &["pr", "edit", &pr_num.to_string(), "--base", new_base],
    )
}

fn merge_pull_squash(pr_num: u32, dry_run: bool) -> bool {
    if dry_run {
        log(&format!("[dry-run] Would squash-merge PR #{pr_num}."));
        return true;
    }
    log(&format!("Squash-merge PR #{pr_num}…"));
    let (ok, out) = cmd_capture("gh", &["pr", "merge", &pr_num.to_string(), "--squash"]);
    if !ok {
        log(&format!("Merge failed for PR #{pr_num}: {out}"));
    }
    ok
}

fn resolve_execution_order(
    pr_by_issue: &HashMap<u32, GhPrMergeRow>,
    trunk: &str,
    hint: Option<u32>,
) -> Vec<u32> {
    if let Some(tid) = hint {
        log(&format!(
            "auto-merge (lineage): using tracker #{tid} for deterministic execution order"
        ));
        return merge_order_tracker(pr_by_issue, tid);
    }
    let trackers = find_tracker();
    if trackers.len() == 1 {
        let tid = trackers[0].number;
        log(&format!(
            "auto-merge (lineage): single open tracker #{tid} detected — deriving order from tracker body"
        ));
        return merge_order_tracker(pr_by_issue, tid);
    }
    if trackers.len() > 1 {
        log(
            "auto-merge (lineage): multiple trackers and no `--tracker` / `CARETTA_MERGE_TRACKER`; falling back to stack graph traversal. Specify a tracker to mirror sprint deterministic ordering.",
        );
    }
    merge_order_topological(pr_by_issue, trunk)
}

/// Walk deterministic tracker order / stack graph, aligning each `--base` with
/// [`find_upstream_branch`] (same chaining policy as [`crate::agent::issue::work_on_issue`]), then squash-merge when Approved and GitHub marks no explicit conflict state (`DIRTY`).
pub fn run_auto_merge_stack(cfg: &Config, tracker_override: Option<u32>) {
    run_lineage_pass(
        cfg,
        tracker_override,
        MergePassMode::SquashMergeWhenEligible,
    );
}

/// Like [`run_auto_merge_stack`], but for each **approved** PR: merge the
/// latest base into the head branch (`gh pr update-branch`), then enable
/// squash auto-merge. Use in CI when merges should wait on branch protection /
/// checks rather than an immediate `gh pr merge`.
pub fn run_automerge_queue(cfg: &Config, tracker_override: Option<u32>) {
    run_lineage_pass(
        cfg,
        tracker_override,
        MergePassMode::UpdateBranchThenAutomergeQueue,
    );
}

fn build_conflict_resolution_comment_body(
    issue: u32,
    row: &GhPrMergeRow,
    expected_base_branch: &str,
    reason: &str,
) -> String {
    format!(
        r#"{CONFLICT_RESOLUTION_MARKER}
@caretta fix: this PR needs branch conflict resolution before it can be reviewed or queued.

Context:
- Issue: #{issue}
- PR: #{pr}
- Head branch: `{head}`
- Current base: `{base}`
- Expected base: `{expected_base_branch}`
- Merge state: `{merge_state}`
- Sync reason: {reason}

Please merge `{expected_base_branch}` into `{head}`, resolve the conflicts, run the relevant checks, and push the result back to `{head}`."#,
        pr = row.number,
        head = row.head_ref.as_str(),
        base = row.base_ref.as_str(),
        merge_state = row.merge_state_status.as_deref().unwrap_or("UNKNOWN"),
    )
}

fn conflict_marker_already_present(pr_num: u32) -> bool {
    let num_s = pr_num.to_string();
    cmd_stdout(
        "gh",
        &[
            "pr",
            "view",
            &num_s,
            "--json",
            "comments",
            "--jq",
            ".comments[].body",
        ],
    )
    .is_some_and(|comments| comments.contains(CONFLICT_RESOLUTION_MARKER))
}

fn post_conflict_resolution_marker(
    issue: u32,
    row: &GhPrMergeRow,
    expected_base_branch: &str,
    reason: &str,
    dry_run: bool,
) -> bool {
    let body = build_conflict_resolution_comment_body(issue, row, expected_base_branch, reason);
    if dry_run {
        log(&format!(
            "[dry-run] Would comment on PR #{} with conflict-resolution marker:\n{body}",
            row.number
        ));
        return true;
    }
    if conflict_marker_already_present(row.number) {
        log(&format!(
            "PR #{} already has a caretta conflict-resolution marker; not commenting again.",
            row.number
        ));
        return true;
    }

    let num_s = row.number.to_string();
    let (ok, out) = cmd_capture("gh", &["pr", "comment", &num_s, "--body", &body]);
    if ok {
        log(&format!(
            "Posted caretta conflict-resolution marker on PR #{}.",
            row.number
        ));
    } else {
        log(&format!(
            "WARNING: failed to post conflict-resolution marker on PR #{}: {out}",
            row.number
        ));
    }
    ok
}

/// Align all open non-draft `agent/issue-*` PR bases, update each branch from
/// its expected base, and mark conflicted PRs for follow-up. Does not merge,
/// approve, or enable GitHub auto-merge.
pub fn run_branch_sync(cfg: &Config, tracker_override: Option<u32>) {
    run_lineage_pass(cfg, tracker_override, MergePassMode::SyncBranchesOnly);
}

fn run_lineage_pass(cfg: &Config, tracker_override: Option<u32>, mode: MergePassMode) {
    if !cfg.dry_run && !has_command("gh") {
        log("auto-merge: `gh` CLI not installed — abort.");
        return;
    }
    let trunk = crate::agent::cmd::origin_default_branch();
    let pass_label = match mode {
        MergePassMode::SquashMergeWhenEligible => "lineage (immediate squash)",
        MergePassMode::UpdateBranchThenAutomergeQueue => "queue (update-branch + auto-merge)",
        MergePassMode::SyncBranchesOnly => "sync (update branches)",
    };
    log(&format!("auto-merge ({pass_label}): trunk base '{trunk}'"));

    let gh_rows = if has_command("gh") {
        if cfg.dry_run {
            log("[dry-run] Listing open PRs (read-only)");
        }
        match cmd_stdout(
            "gh",
            &[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "150",
                "--json",
                "number,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision",
            ],
        ) {
            Some(raw) => parse_gh_pr_merge_rows(&raw),
            None => Vec::new(),
        }
    } else {
        log("[dry-run] `gh` missing — pretending no open PR rows.");
        Vec::new()
    };

    let pr_by_issue = agent_issue_pull_rows(&gh_rows);
    if pr_by_issue.is_empty() {
        log("auto-merge (lineage): no open non-draft agent/issue-* pull requests matched.");
        return;
    }

    let hint = tracker_override.or_else(tracker_from_env);
    let mut order = resolve_execution_order(&pr_by_issue, &trunk, hint);
    if matches!(mode, MergePassMode::SyncBranchesOnly) {
        order = append_missing_topological_order(order, &pr_by_issue, &trunk);
    }

    let pending_by_issue: HashMap<u32, _> = if let Some(tid) =
        tracker_override.or_else(tracker_from_env).or_else(|| {
            let ts = find_tracker();
            (ts.len() == 1).then(|| ts[0].number)
        }) {
        parse_pending(&get_tracker_body(tid))
            .into_iter()
            .map(|p| (p.number, p))
            .collect()
    } else {
        HashMap::new()
    };

    log(&format!(
        "auto-merge (lineage): sequence (issue #): {}",
        order
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" → ")
    ));

    if order.is_empty() {
        log(
            "auto-merge (lineage): nothing scheduled after deterministic ordering filtered to open PR rows.",
        );
        return;
    }

    let mut refreshed = gh_rows.clone();
    let mut by_issue_cursor = agent_issue_pull_rows(&refreshed);

    for issue in order {
        let Some(mut row_snapshot) = by_issue_cursor.get(&issue).cloned() else {
            log(&format!(
                "Issue #{issue} no longer has an open matching PR — likely merged already.",
            ));
            continue;
        };

        let inferred_parent: Vec<u32> = issue_num_from_agent_head(&row_snapshot.base_ref)
            .into_iter()
            .collect();

        let blockers: Vec<u32> = pending_by_issue
            .get(&issue)
            .map(|p| p.blockers.clone())
            .unwrap_or_else(|| inferred_parent.clone());

        let expected_base_branch = find_upstream_branch(&blockers);

        let lineage_unknown = !pending_by_issue.contains_key(&issue)
            && inferred_parent.is_empty()
            && row_snapshot.base_ref != trunk;

        if lineage_unknown {
            log(&format!(
                "Issue #{issue}: PR #{num} stacks on '{}' without tracker blocker metadata nor an agent/issue-* inferred parent — skipping base edits.",
                row_snapshot.base_ref,
                num = row_snapshot.number,
            ));
        }

        let needs_retarget = row_snapshot.base_ref != expected_base_branch && !lineage_unknown;
        if needs_retarget
            && !retarget_pull_base(row_snapshot.number, &expected_base_branch, cfg.dry_run)
        {
            log(&format!(
                "Giving up on PR #{} (#{issue}): unable to align base to '{}'.",
                row_snapshot.number, expected_base_branch
            ));
            continue;
        }

        if cfg.dry_run && needs_retarget {
            row_snapshot.base_ref.clone_from(&expected_base_branch);
        } else {
            let num_s = row_snapshot.number.to_string();
            if let Some(b) = cmd_stdout(
                "gh",
                &[
                    "pr",
                    "view",
                    &num_s,
                    "--json",
                    "baseRefName",
                    "--jq",
                    ".baseRefName",
                ],
            )
            .filter(|s| !s.trim().is_empty())
            {
                row_snapshot.base_ref = b.trim().to_owned();
            }

            let out = cmd_stdout(
                "gh",
                &[
                    "pr",
                    "view",
                    &num_s,
                    "--json",
                    "mergeStateStatus,reviewDecision,isDraft",
                ],
            );
            if let Some(json) = out.filter(|s| !s.trim().is_empty())
                && let Ok(partial) = serde_json::from_str::<PrStatusRefresh>(&json)
            {
                row_snapshot.merge_state_status = partial.merge_state_status;
                row_snapshot.review_decision = partial.review_decision;
                row_snapshot.is_draft = partial.is_draft;
            }
        }

        if matches!(mode, MergePassMode::SyncBranchesOnly) {
            if is_dirty(&row_snapshot) {
                log(&format!(
                    "PR #{} (#{issue}) is DIRTY against '{}'; marking for conflict resolution.",
                    row_snapshot.number, row_snapshot.base_ref,
                ));
                post_conflict_resolution_marker(
                    issue,
                    &row_snapshot,
                    &expected_base_branch,
                    "GitHub reports `mergeStateStatus=DIRTY`.",
                    cfg.dry_run,
                );
                continue;
            }

            if !pr_update_branch(row_snapshot.number, cfg.dry_run) {
                post_conflict_resolution_marker(
                    issue,
                    &row_snapshot,
                    &expected_base_branch,
                    "`gh pr update-branch` failed while syncing this branch.",
                    cfg.dry_run,
                );
                continue;
            }
            if !cfg.dry_run {
                refreshed = gh_list_merge_candidate_prs();
                by_issue_cursor = agent_issue_pull_rows(&refreshed);
            }
            continue;
        }

        let eligible = match mode {
            MergePassMode::SquashMergeWhenEligible => eligible_for_immediate_merge(&row_snapshot),
            MergePassMode::UpdateBranchThenAutomergeQueue => {
                eligible_for_automerge_queue(&row_snapshot)
            }
            MergePassMode::SyncBranchesOnly => unreachable!("sync mode handled above"),
        };
        if !eligible {
            log(&format!(
                "Skipping PR #{} (#{issue}); mergeState={:?} reviewDecision={:?} draft={} (mode={:?})",
                row_snapshot.number,
                row_snapshot.merge_state_status,
                row_snapshot.review_decision,
                row_snapshot.is_draft,
                mode,
            ));
            continue;
        }

        match mode {
            MergePassMode::SquashMergeWhenEligible => {
                if !merge_pull_squash(row_snapshot.number, cfg.dry_run) {
                    continue;
                }
            }
            MergePassMode::UpdateBranchThenAutomergeQueue => {
                if !pr_update_branch(row_snapshot.number, cfg.dry_run) {
                    continue;
                }
                if !cfg.dry_run {
                    if is_auto_merge_enabled(row_snapshot.number) {
                        log(&format!(
                            "PR #{} already has auto-merge enabled.",
                            row_snapshot.number
                        ));
                    } else if !enable_auto_merge(row_snapshot.number) {
                        log(&format!(
                            "WARNING: could not enable auto-merge on PR #{}.",
                            row_snapshot.number
                        ));
                    }
                }
            }
            MergePassMode::SyncBranchesOnly => unreachable!("sync mode handled above"),
        }

        if !cfg.dry_run {
            refreshed = gh_list_merge_candidate_prs();
            by_issue_cursor = agent_issue_pull_rows(&refreshed);
        }
    }

    log(&format!("auto-merge ({pass_label}): pass complete."));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch_for_issue(n: u32) -> String {
        format!("{BRANCH_PREFIX}{n}")
    }

    fn fixture_row(number: u32, issue_head: u32, base_ref: impl Into<String>) -> GhPrMergeRow {
        GhPrMergeRow {
            number,
            head_ref: branch_for_issue(issue_head),
            base_ref: base_ref.into(),
            is_draft: false,
            merge_state_status: None,
            review_decision: None,
        }
    }

    #[test]
    fn topo_roots_before_dependents() {
        let trunk = "main";
        let mut m = HashMap::new();
        m.insert(10, fixture_row(1000, 10, trunk));
        m.insert(11, fixture_row(1001, 11, branch_for_issue(10)));
        assert_eq!(merge_order_topological(&m, trunk), vec![10, 11]);
    }

    #[test]
    fn topo_siblings_sorted_lowest_issue_first_among_roots() {
        let trunk = "main";
        let mut m = HashMap::new();
        m.insert(30, fixture_row(1, 30, trunk));
        m.insert(20, fixture_row(2, 20, trunk));
        assert_eq!(merge_order_topological(&m, trunk), vec![20, 30]);
    }

    #[test]
    fn agent_issue_pull_skips_drafts_and_foreign_heads() {
        let trunk = "main".to_owned();
        let rows = vec![
            GhPrMergeRow {
                is_draft: true,
                ..fixture_row(1, 5, trunk.clone())
            },
            GhPrMergeRow {
                head_ref: "contrib/other".into(),
                ..fixture_row(2, 999, trunk.clone())
            },
            GhPrMergeRow {
                ..fixture_row(3, 77, trunk.clone())
            },
        ];
        let m = agent_issue_pull_rows(&rows);
        assert_eq!(m.len(), 1);
        assert!(m.contains_key(&77));
        let rows2 = vec![GhPrMergeRow {
            is_draft: false,
            ..fixture_row(1, 5, trunk.clone())
        }];
        let m2 = agent_issue_pull_rows(&rows2);
        assert!(m2.contains_key(&5));
    }

    #[test]
    fn tracker_order_subtracts_issues_without_pulls() {
        let body = "## Sprint\n- [ ] #20 a\n- [ ] #99 b blocked by #20\n";

        assert_eq!(
            pending_issues_execution_order(body),
            vec![20, 99],
            "fixture tracker body should unblock #99 after #20"
        );

        let mut hs = HashSet::new();
        hs.insert(99);
        assert_eq!(
            tracker_merge_candidates_order(body, &hs),
            vec![99],
            "only issues with pulls remain in deterministic slice"
        );
    }

    #[test]
    fn gh_pr_list_json_fields_deserialize_into_merge_rows() {
        let raw = r#"[
            {
                "number": 77,
                "headRefName": "agent/issue-70",
                "baseRefName": "master",
                "isDraft": false,
                "mergeStateStatus": "BEHIND",
                "reviewDecision": "APPROVED"
            }
        ]"#;

        let rows = parse_gh_pr_merge_rows(raw);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].head_ref, "agent/issue-70");
        assert_eq!(rows[0].base_ref, "master");

        let matches = agent_issue_pull_rows(&rows);
        assert!(matches.contains_key(&70));
    }

    #[test]
    fn sync_order_appends_open_prs_missing_from_tracker_slice() {
        let trunk = "main";
        let mut m = HashMap::new();
        m.insert(10, fixture_row(1000, 10, trunk));
        m.insert(11, fixture_row(1001, 11, branch_for_issue(10)));

        let order = append_missing_topological_order(vec![10], &m, trunk);

        assert_eq!(order, vec![10, 11]);
    }

    #[test]
    fn conflict_resolution_comment_has_caretta_marker_and_context() {
        let mut row = fixture_row(77, 70, "master");
        row.merge_state_status = Some("DIRTY".to_string());

        let body = build_conflict_resolution_comment_body(
            70,
            &row,
            "master",
            "GitHub reports `mergeStateStatus=DIRTY`.",
        );

        assert!(body.contains(CONFLICT_RESOLUTION_MARKER));
        assert!(body.contains("@caretta fix"));
        assert!(body.contains("Issue: #70"));
        assert!(body.contains("PR: #77"));
        assert!(body.contains("Head branch: `agent/issue-70`"));
        assert!(body.contains("Expected base: `master`"));
        assert!(body.contains("mergeStateStatus=DIRTY"));
    }
}
