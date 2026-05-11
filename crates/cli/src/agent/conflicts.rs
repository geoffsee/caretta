use crate::agent::cmd::{cmd_capture, cmd_run, cmd_stdout, log};
use crate::agent::issue::preflight;
use crate::agent::review::WorktreeGuard;
use crate::agent::run::run_agent_with_env_in_dir;
use crate::agent::tracker::{list_open_prs, pr_diff};
use crate::agent::types::{AgentEvent, Config};
use crate::agent::{launch::log_resolved_agent_launch, process::emit_event};
use std::path::Path;

pub const CONFLICT_RESOLUTION_MARKER: &str = "<!-- caretta:branch-sync-conflict -->";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConflictMarkerContext {
    head_branch: String,
    expected_base: String,
    body: String,
}

#[derive(Debug, serde::Deserialize)]
struct PrConflictView {
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct PrCommentsView {
    comments: Vec<PrComment>,
}

#[derive(Debug, serde::Deserialize)]
struct PrComment {
    body: String,
}

struct ConflictFixPromptContext<'a> {
    project_name: &'a str,
    pr_num: u32,
    title: &'a str,
    branch: &'a str,
    expected_base: &'a str,
    merge_state: &'a str,
    marker_body: &'a str,
    diff: &'a str,
}

fn parse_backtick_field(body: &str, label: &str) -> Option<String> {
    let needle = format!("- {label}: `");
    let line = body
        .lines()
        .find(|line| line.trim_start().starts_with(&needle))?;
    let start = line.find('`')? + 1;
    let tail = &line[start..];
    let end = tail.find('`')?;
    let value = tail[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_conflict_marker_body(body: &str) -> Option<ConflictMarkerContext> {
    if !body.contains(CONFLICT_RESOLUTION_MARKER) {
        return None;
    }
    let head_branch = parse_backtick_field(body, "Head branch")?;
    let expected_base = parse_backtick_field(body, "Expected base")?;
    Some(ConflictMarkerContext {
        head_branch,
        expected_base,
        body: body.to_string(),
    })
}

fn parse_latest_conflict_marker(raw: &str) -> Option<ConflictMarkerContext> {
    let parsed: PrCommentsView = serde_json::from_str(raw).ok()?;
    parsed
        .comments
        .iter()
        .rev()
        .filter_map(|comment| parse_conflict_marker_body(&comment.body))
        .next()
}

fn fetch_conflict_marker_context(pr_num: u32) -> Option<ConflictMarkerContext> {
    let num_s = pr_num.to_string();
    let raw = cmd_stdout("gh", &["pr", "view", &num_s, "--json", "comments"])?;
    parse_latest_conflict_marker(&raw)
}

fn fetch_pr_conflict_view(pr_num: u32) -> Option<PrConflictView> {
    let num_s = pr_num.to_string();
    let raw = cmd_stdout(
        "gh",
        &[
            "pr",
            "view",
            &num_s,
            "--json",
            "headRefName,baseRefName,mergeStateStatus,title",
        ],
    )?;
    serde_json::from_str(&raw).ok()
}

fn build_conflict_fix_prompt(ctx: &ConflictFixPromptContext<'_>) -> String {
    format!(
        r#"You are resolving merge conflicts on pull request #{pr_num} for the {project_name} project.

Read AGENTS.md and skills/ for project conventions and coding standards.

## Working directory

Your current working directory is a freshly-created git worktree on branch `{branch}`. The calling script has already attempted to merge `{expected_base}` into `{branch}`. If there were conflicts, the files contain normal Git conflict markers.

Do NOT run `git checkout`, `git merge`, `git rebase`, `git commit`, or `git push`. The calling script handles branching, merge setup, commit, push, and cleanup.

## Pull Request #{pr_num}: {title}

Merge state reported before this run: `{merge_state}`.

## Conflict Request

{marker_body}

## Current PR Diff

```diff
{diff}
```

## Instructions

- Inspect `git status` to find unmerged paths.
- Edit every conflicted file to remove conflict markers and preserve the intended behavior from both `{expected_base}` and `{branch}`.
- Keep the change focused to conflict resolution. Do not refactor unrelated code.
- If a generated lockfile is conflicted, resolve it consistently with the manifest files in the worktree.
- Run the smallest relevant format/check command if it is quick. If not, re-read the resolved files and leave validation to CI.
- Do not post comments or reviews back to GitHub."#,
        pr_num = ctx.pr_num,
        project_name = ctx.project_name,
        branch = ctx.branch,
        expected_base = ctx.expected_base,
        title = ctx.title,
        merge_state = ctx.merge_state,
        marker_body = ctx.marker_body,
        diff = ctx.diff,
    )
}

fn unresolved_merge_paths(worktree: &Path) -> Vec<String> {
    let worktree_str = worktree.to_string_lossy().to_string();
    cmd_stdout(
        "git",
        &[
            "-C",
            &worktree_str,
            "diff",
            "--name-only",
            "--diff-filter=U",
        ],
    )
    .unwrap_or_default()
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .map(ToOwned::to_owned)
    .collect()
}

fn worktree_status(worktree: &Path) -> String {
    let worktree_str = worktree.to_string_lossy().to_string();
    cmd_stdout("git", &["-C", &worktree_str, "status", "--porcelain"]).unwrap_or_default()
}

pub fn run_pr_conflict_fix(cfg: &Config, pr_num: u32) {
    preflight(cfg);
    log(&format!(
        "Starting conflict-resolution run for PR #{pr_num}..."
    ));

    let Some(pr) = fetch_pr_conflict_view(pr_num) else {
        log(&format!("No open pull request matched PR #{pr_num}."));
        emit_event(AgentEvent::Done);
        return;
    };

    let marker = fetch_conflict_marker_context(pr_num);
    let marker_body = marker
        .as_ref()
        .map(|ctx| ctx.body.clone())
        .unwrap_or_else(|| {
            format!(
                "{CONFLICT_RESOLUTION_MARKER}\n@caretta fix: resolve merge conflicts for PR #{pr_num}."
            )
        });
    let branch = marker
        .as_ref()
        .map(|ctx| ctx.head_branch.clone())
        .unwrap_or_else(|| pr.head_ref.clone());
    let expected_base = marker
        .as_ref()
        .map(|ctx| ctx.expected_base.clone())
        .unwrap_or_else(|| pr.base_ref.clone());
    let merge_state = pr.merge_state_status.as_deref().unwrap_or("UNKNOWN");

    if branch != pr.head_ref {
        log(&format!(
            "Conflict marker head branch '{}' differs from current PR head '{}'; using current head.",
            branch, pr.head_ref
        ));
    }
    let branch = pr.head_ref.clone();

    if cfg.dry_run {
        log_resolved_agent_launch(cfg, &[]);
        log(&format!(
            "[dry-run] Would resolve PR #{pr_num} conflicts by merging '{expected_base}' into '{branch}'."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let worktree_path = std::env::temp_dir().join(format!(
        "caretta-conflicts-pr-{pr_num}-{}",
        std::process::id()
    ));
    let worktree_str = worktree_path.to_string_lossy().to_string();
    let remote_ref = format!("origin/{branch}");
    let fetch_refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    if !cmd_run("git", &["fetch", "origin", &fetch_refspec]) {
        log(&format!("Failed to fetch branch '{branch}' from origin."));
        emit_event(AgentEvent::Done);
        return;
    }

    if !cmd_run(
        "git",
        &[
            "worktree",
            "add",
            "--force",
            "-B",
            &branch,
            &worktree_str,
            &remote_ref,
        ],
    ) {
        log(&format!(
            "Failed to create conflict-resolution worktree for PR #{pr_num} from {remote_ref}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }
    let _guard = WorktreeGuard {
        path: worktree_path.clone(),
    };

    let expected_base_refspec =
        format!("+refs/heads/{expected_base}:refs/remotes/origin/{expected_base}");
    if !cmd_run(
        "git",
        &[
            "-C",
            &worktree_str,
            "fetch",
            "origin",
            &expected_base_refspec,
        ],
    ) {
        log(&format!(
            "Failed to fetch expected base '{expected_base}' for PR #{pr_num}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let expected_base_ref = format!("origin/{expected_base}");
    let merge_ok = cmd_run(
        "git",
        &[
            "-C",
            &worktree_str,
            "merge",
            "--no-ff",
            "--no-commit",
            &expected_base_ref,
        ],
    );

    if merge_ok && worktree_status(&worktree_path).trim().is_empty() {
        log(&format!(
            "PR #{pr_num} already includes '{expected_base}'; no conflict fix needed."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    if merge_ok {
        log(&format!(
            "Merged '{expected_base}' into PR #{pr_num} without file conflicts; committing the branch update."
        ));
    } else {
        let unresolved = unresolved_merge_paths(&worktree_path);
        log(&format!(
            "Merge produced {} conflicted path(s) for PR #{pr_num}; launching agent.",
            unresolved.len()
        ));

        let diff = pr_diff(pr_num);
        let prompt = build_conflict_fix_prompt(&ConflictFixPromptContext {
            project_name: &cfg.project_name,
            pr_num,
            title: &pr.title,
            branch: &branch,
            expected_base: &expected_base,
            merge_state,
            marker_body: &marker_body,
            diff: &diff,
        });

        if !run_agent_with_env_in_dir(cfg, &prompt, &[], &worktree_path) {
            log(&format!(
                "Conflict-resolution agent failed for PR #{pr_num}."
            ));
            emit_event(AgentEvent::Done);
            return;
        }
    }

    let unresolved = unresolved_merge_paths(&worktree_path);
    if !unresolved.is_empty() {
        log(&format!(
            "Conflict-resolution run left unresolved merge path(s) for PR #{pr_num}: {}",
            unresolved.join(", ")
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    if worktree_status(&worktree_path).trim().is_empty() {
        log(&format!(
            "Conflict-resolution run made no file changes for PR #{pr_num}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let message = format!(
        "resolve merge conflicts for PR #{pr_num}\n\n{}",
        cfg.agent.co_author()
    );
    let committed = cmd_run("git", &["-C", &worktree_str, "add", "."])
        && cmd_run("git", &["-C", &worktree_str, "commit", "-m", &message]);
    if !committed {
        log(&format!(
            "Failed to commit conflict-resolution changes for PR #{pr_num}."
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let (ok, out) = cmd_capture("git", &["-C", &worktree_str, "push", "origin", &branch]);
    if !ok {
        log(&format!(
            "Failed to push conflict-resolution changes for PR #{pr_num}: {out}"
        ));
        emit_event(AgentEvent::Done);
        return;
    }

    let remaining_dirty = list_open_prs()
        .into_iter()
        .find(|summary| summary.number == pr_num)
        .map(|_| {
            fetch_pr_conflict_view(pr_num)
                .and_then(|view| view.merge_state_status)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    log(&format!(
        "Conflict-resolution complete for PR #{pr_num}; pushed '{branch}' (mergeStateStatus={remaining_dirty:?})."
    ));
    emit_event(AgentEvent::Done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conflict_marker_context() {
        let body = r#"<!-- caretta:branch-sync-conflict -->
@caretta fix: this PR needs branch conflict resolution.

Context:
- Issue: #74
- PR: #80
- Head branch: `agent/issue-74`
- Current base: `agent/issue-70`
- Expected base: `agent/issue-70`
- Merge state: `DIRTY`
"#;

        let parsed = parse_conflict_marker_body(body).expect("marker should parse");

        assert_eq!(parsed.head_branch, "agent/issue-74");
        assert_eq!(parsed.expected_base, "agent/issue-70");
    }

    #[test]
    fn latest_marker_wins() {
        let raw = serde_json::json!({
            "comments": [
                {"body": format!("{CONFLICT_RESOLUTION_MARKER}\n- Head branch: `agent/issue-1`\n- Expected base: `master`")},
                {"body": format!("{CONFLICT_RESOLUTION_MARKER}\n- Head branch: `agent/issue-2`\n- Expected base: `agent/issue-1`")}
            ]
        })
        .to_string();

        let parsed = parse_latest_conflict_marker(&raw).expect("latest marker should parse");

        assert_eq!(parsed.head_branch, "agent/issue-2");
        assert_eq!(parsed.expected_base, "agent/issue-1");
    }

    #[test]
    fn conflict_fix_prompt_contains_merge_context() {
        let prompt = build_conflict_fix_prompt(&ConflictFixPromptContext {
            project_name: "caretta",
            pr_num: 80,
            title: "implement #74",
            branch: "agent/issue-74",
            expected_base: "agent/issue-70",
            merge_state: "DIRTY",
            marker_body: "@caretta fix",
            diff: "diff --git a/a b/a",
        });

        assert!(prompt.contains("pull request #80"));
        assert!(prompt.contains("agent/issue-70"));
        assert!(prompt.contains("git status"));
        assert!(prompt.contains("Do NOT run `git checkout`"));
    }
}
