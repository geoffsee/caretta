use super::*;

/// #88 / #137: `find_tracker` must return one entry per row in the
/// `gh issue list --label tracker` JSON, sorted by issue number.
#[test]
fn parse_tracker_list_extracts_number_and_title() {
    let json = r#"[
            {"number": 102, "title": "Agents: behavior, skills, and issue hygiene"},
            {"number": 14, "title": "Sprint 1 Tracker"}
        ]"#;
    let trackers = parse_tracker_list(json);
    assert_eq!(trackers.len(), 2);
    // Sorted ascending by number, regardless of input order.
    assert_eq!(trackers[0].number, 14);
    assert_eq!(trackers[0].title, "Sprint 1 Tracker");
    assert_eq!(trackers[1].number, 102);
    assert_eq!(
        trackers[1].title,
        "Agents: behavior, skills, and issue hygiene"
    );
}

/// #88 / #137: duplicate rows must be collapsed (defends against gh
/// label paging quirks that have surfaced doubles).
#[test]
fn parse_tracker_list_dedupes_repeated_numbers() {
    let json = r#"[
            {"number": 5, "title": "tracker A"},
            {"number": 5, "title": "tracker A"},
            {"number": 7, "title": "tracker B"}
        ]"#;
    let trackers = parse_tracker_list(json);
    assert_eq!(trackers.len(), 2);
    assert_eq!(trackers[0].number, 5);
    assert_eq!(trackers[1].number, 7);
}

/// #88 / #137: an empty `gh` response must yield an empty Vec rather
/// than panicking.
#[test]
fn parse_tracker_list_handles_empty_input() {
    assert!(parse_tracker_list("[]").is_empty());
    assert!(parse_tracker_list("").is_empty());
}

/// #88 / #137: regression guard for the title-keyword bug. Before
/// the fix, `find_tracker` matched issues whose title contained
/// "tracker" (e.g. the parent-tracker child issue from #84). The
/// label-based call now filters server-side, so the parser is fed
/// only label-tagged rows — but this guard also asserts the gh
/// argument list still includes `--label labels::TRACKER` so a
/// future refactor cannot silently revert to title search.
#[test]
fn find_tracker_uses_label_filter_not_title_search() {
    let src = include_str!("mod.rs");
    // Locate the find_tracker function body.
    let body_start = src
        .find("pub fn find_tracker()")
        .expect("find_tracker function should exist");
    // Bound the search to the next top-level `pub fn` so we only
    // inspect this function's body.
    let body_end = src[body_start + 1..]
        .find("\npub fn ")
        .map(|i| body_start + 1 + i)
        .unwrap_or(src.len());
    let body = &src[body_start..body_end];
    assert!(
        body.contains("\"--label\""),
        "find_tracker must call gh with --label, body was: {body}"
    );
    assert!(
        body.contains("labels::TRACKER"),
        "find_tracker must filter by labels::TRACKER, body was: {body}"
    );
    // Defensive: the deprecated title-search path used `--search`
    // with quoted title keywords. Make sure it's not back.
    assert!(
        !body.contains("\"--search\""),
        "find_tracker must not use --search (title-keyword regression)"
    );
}

#[test]
fn refs_basic() {
    assert_eq!(extract_issue_refs("- [ ] #42 something"), vec![42]);
}

#[test]
fn refs_multiple() {
    assert_eq!(extract_issue_refs("blocked by #3, #7"), vec![3, 7]);
}

#[test]
fn refs_ignores_bare_numbers() {
    assert_eq!(extract_issue_refs("keep under 10MB"), Vec::<u32>::new());
}

#[test]
fn refs_ignores_hash_without_digits() {
    assert_eq!(extract_issue_refs("use # as comment"), Vec::<u32>::new());
}

#[test]
fn refs_adjacent_to_punctuation() {
    assert_eq!(extract_issue_refs("(#5)"), vec![5]);
    assert_eq!(extract_issue_refs("#5."), vec![5]);
    assert_eq!(extract_issue_refs("#5,#6"), vec![5, 6]);
}

#[test]
fn refs_with_spaces() {
    assert_eq!(extract_issue_refs("# 42"), vec![42]);
    assert_eq!(extract_issue_refs("#  42"), vec![42]);
}

#[test]
fn bare_basic() {
    assert_eq!(extract_bare_numbers("blocked by 3, 5"), vec![3, 5]);
}

#[test]
fn bare_mixed_text() {
    assert_eq!(extract_bare_numbers("issues 12 and 34"), vec![12, 34]);
}

#[test]
fn blockers_prefers_hash_refs() {
    assert_eq!(extract_blockers(" #3, #7"), vec![3, 7]);
}

#[test]
fn blockers_falls_back_to_bare() {
    assert_eq!(extract_blockers(" 3, 7"), vec![3, 7]);
}

#[test]
fn blockers_empty() {
    assert_eq!(extract_blockers(""), Vec::<u32>::new());
}

#[test]
fn completed_basic() {
    let body = "\
- [x] #1 Set up project
- [x] #2 Add CI
- [ ] #3 Implement feature";
    let done = parse_completed(body);
    assert_eq!(done, HashSet::from([1, 2]));
}

#[test]
fn completed_uppercase_x() {
    let body = "- [X] #99 Done thing";
    assert_eq!(parse_completed(body), HashSet::from([99]));
}

#[test]
fn completed_ignores_bare_numbers_in_text() {
    let body = "- [x] #5 keep under 10MB";
    let done = parse_completed(body);
    assert_eq!(done, HashSet::from([5]));
    assert!(!done.contains(&10));
}

#[test]
fn completed_with_emoji() {
    let body = "| #5 | ✅ Done |";
    let done = parse_completed(body);
    assert_eq!(done, HashSet::from([5]));
}

#[test]
fn completed_with_alternate_markers() {
    let body = r#"
| #1 | Item 1 | ✔️ Done |
| #2 | Item 2 | ☑️ Done |
| #3 | Item 3 | done |
| #4 | Item 4 | Complete |
"#;
    let set = parse_completed(body);
    assert!(set.contains(&1));
    assert!(set.contains(&2));
    assert!(set.contains(&3));
    assert!(set.contains(&4));
}

#[test]
fn completed_skips_dependencies_in_tables() {
    let body = "| #5 | ✅ Done | #1, #2 |";
    let done = parse_completed(body);
    assert!(done.contains(&5));
    assert!(!done.contains(&1));
    assert!(!done.contains(&2));
}

#[test]
fn completed_empty() {
    assert_eq!(parse_completed(""), HashSet::new());
}

#[test]
fn execution_order_single_issue() {
    let body = "- [ ] #10 New task";
    assert_eq!(pending_issues_execution_order(body), vec![10]);
}

#[test]
fn execution_order_respects_pending_blockers_before_dependents() {
    let body = "\
- [ ] #49 Child blocked by #48
- [ ] #48 Parent
";
    assert_eq!(pending_issues_execution_order(body), vec![48, 49]);
}

#[test]
fn execution_order_completed_blocker_skips_reordering() {
    let body = "\
- [x] #48 Parent done
- [ ] #49 Child blocked by #48
";
    assert_eq!(pending_issues_execution_order(body), vec![49]);
}

#[test]
fn execution_order_fallback_document_order_on_cycle() {
    let body = "\
- [ ] #1 Alpha blocked by #2
- [ ] #2 Beta blocked by #1
";
    assert_eq!(pending_issues_execution_order(body), vec![1, 2]);
}

#[test]
fn pending_no_blockers() {
    let body = "- [ ] #10 New task";
    let pending = parse_pending(body);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].number, 10);
    assert!(pending[0].blockers.is_empty());
}

#[test]
fn pending_with_hash_blockers() {
    let body = "- [ ] #11 Task blocked by #10";
    let pending = parse_pending(body);
    assert_eq!(pending[0].number, 11);
    assert_eq!(pending[0].blockers, vec![10]);
}

#[test]
fn pending_with_bare_blockers() {
    let body = "- [ ] #12 Task blocked by 10, 11";
    let pending = parse_pending(body);
    assert_eq!(pending[0].blockers, vec![10, 11]);
}

#[test]
fn pending_does_not_leak_issue_into_blockers() {
    let body = "- [ ] #13 task";
    let pending = parse_pending(body);
    assert!(pending[0].blockers.is_empty());
}

#[test]
fn pending_with_table_status() {
    let body = "| #42 | #10 | — | 0 | 🟡 In progress |";
    let pending = parse_pending(body);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].number, 42);
    assert_eq!(pending[0].blockers, vec![10]);
}

#[test]
fn pending_skips_completed_lines() {
    let body = "- [x] #1 done";
    assert!(parse_pending(body).is_empty());
}

#[test]
fn pending_deduplicates_repeated_issues() {
    let body = "\
- [ ] #34 Focused Delivery: Gateway WebSocket upgrade handling MVP
| #34 Focused Delivery: Gateway WebSocket upgrade handling MVP | — | #35, #36, #32 | 0 | 🔴 Not Started |
";
    let pending = parse_pending(body);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].number, 34);
    // The table row heuristic should not pick up dependents as blockers
    assert!(pending[0].blockers.is_empty());
}

#[test]
fn pending_extracts_blockers_from_table() {
    let body = "| #36 | #34, #35 | #32 | 2 | 🔴 Not Started |";
    let pending = parse_pending(body);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].number, 36);
    assert_eq!(pending[0].blockers, vec![34, 35]);
}

#[test]
fn ready_no_blockers() {
    let issue = PendingIssue {
        number: 1,
        title: String::new(),
        blockers: vec![],
        pr_number: None,
    };
    let completed = HashSet::new();
    assert!(is_ready(&issue, &completed));
}

#[test]
fn ready_all_done() {
    let issue = PendingIssue {
        number: 3,
        title: String::new(),
        blockers: vec![1, 2],
        pr_number: None,
    };
    let completed = HashSet::from([1, 2]);
    assert!(is_ready(&issue, &completed));
}

#[test]
fn blocked_missing_dep() {
    let issue = PendingIssue {
        number: 3,
        title: String::new(),
        blockers: vec![1, 2],
        pr_number: None,
    };
    let completed = HashSet::from([1]);
    assert!(!is_ready(&issue, &completed));
}

#[test]
fn mark_replaces_checkbox() {
    let body = "- [ ] #123 task";
    assert_eq!(mark_completed(body, 123), "- [x] #123 task");
}

#[test]
fn mark_bold_issue_ref() {
    let body = "- [ ] **#19** — Persist controller state `[M]`";
    assert_eq!(
        mark_completed(body, 19),
        "- [x] **#19** — Persist controller state `[M]`"
    );
}

#[test]
fn mark_no_match_is_noop() {
    let body = "- [ ] #456 task";
    assert_eq!(mark_completed(body, 123), body);
}

// ── PrSummary deserialization ──

#[test]
fn pr_summary_deserialize_full() {
    let json = r#"[
            {"number":42,"title":"Add caching","headRefName":"feat/cache","author":{"login":"alice"}}
        ]"#;
    let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 42);
    assert_eq!(prs[0].title, "Add caching");
    assert_eq!(prs[0].head_ref_name, "feat/cache");
    assert_eq!(prs[0].author.as_ref().unwrap().login, "alice");
}

#[test]
fn pr_summary_deserialize_no_author() {
    let json = r#"[{"number":1,"title":"Fix","headRefName":"fix/bug"}]"#;
    let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
    assert_eq!(prs.len(), 1);
    assert!(prs[0].author.is_none());
}

#[test]
fn pr_summary_deserialize_empty_array() {
    let prs: Vec<PrSummary> = serde_json::from_str("[]").unwrap();
    assert!(prs.is_empty());
}

/// Phase 4 (#146): the new `unresolved_thread_count` field must default
/// to 0 when missing from the `gh pr list` JSON, so the existing CLI
/// payload (which has no thread-count column) deserializes unchanged.
#[test]
fn pr_summary_unresolved_thread_count_defaults_to_zero() {
    let json = r#"[
            {"number":42,"title":"Add caching","headRefName":"feat/cache","author":{"login":"alice"}}
        ]"#;
    let prs: Vec<PrSummary> = serde_json::from_str(json).unwrap();
    assert_eq!(prs[0].unresolved_thread_count, 0);
}

// ── Phase 4: batched PR thread-count parser (#146) ──

/// Acceptance criterion from #146: parses a batched
/// `repository.pullRequests.reviewThreads` GraphQL response into a
/// `{pr_number: count}` map. Resolved threads and human-authored
/// threads are excluded so the badge count matches what the Phase 2
/// Fix Comments dispatch would actually act on.
#[test]
fn parse_pr_thread_counts_filters_resolved_and_human_authors() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 143,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": true,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "geoffsee"}}]}
                                        }
                                    ]
                                }
                            },
                            {
                                "number": 144,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": true,
                                            "comments": {"nodes": [{"author": {"login": "llm-overlord"}}]}
                                        }
                                    ]
                                }
                            },
                            {
                                "number": 145,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "dependabot[bot]"}}]}
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }"#;
    let counts = parse_pr_thread_counts(json, "llm-overlord");

    // PR #143: 4 threads total, but only 2 are unresolved AND bot-authored.
    assert_eq!(counts.get(&143), Some(&2));
    // PR #144: only 1 thread, and it's resolved => not in the map at all.
    assert!(!counts.contains_key(&144));
    // PR #145: dependabot[bot] qualifies via the bracket-bot suffix rule.
    assert_eq!(counts.get(&145), Some(&1));
}

/// Human-authored threads opt in via [`HUMAN_FIX_MARKER`] in the body.
/// The batched count and the per-PR parser must agree on this rule, so
/// the sidebar badge does not under-report human opt-in threads.
#[test]
fn parse_pr_thread_counts_accepts_human_with_fix_marker() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 99,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{
                                                "author": {"login": "geoffsee", "__typename": "User"},
                                                "body": "@caretta fix: please rename this"
                                            }]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{
                                                "author": {"login": "geoffsee", "__typename": "User"},
                                                "body": "thoughts?"
                                            }]}
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }"#;
    let counts = parse_pr_thread_counts(json, "llm-overlord");
    assert_eq!(counts.get(&99), Some(&1));
}

/// GitHub Apps surface in GraphQL as `__typename: "Bot"` even when their
/// login lacks the `[bot]` suffix. The batched parser must count those
/// threads or the badge under-reports for App-installation reviewers.
#[test]
fn parse_pr_thread_counts_accepts_bot_typename() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 77,
                                "reviewThreads": {
                                    "nodes": [
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "caretta-ai", "__typename": "Bot"}}]}
                                        },
                                        {
                                            "isResolved": false,
                                            "comments": {"nodes": [{"author": {"login": "caretta-ai", "__typename": "Bot"}}]}
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }"#;
    let counts = parse_pr_thread_counts(json, "llm-overlord");
    assert_eq!(counts.get(&77), Some(&2));
}

/// PRs with no review threads at all (the common case for fresh PRs)
/// must NOT appear in the map — callers treat absence as zero.
#[test]
fn parse_pr_thread_counts_omits_zero_count_prs() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 200,
                                "reviewThreads": {"nodes": []}
                            }
                        ]
                    }
                }
            }
        }"#;
    let counts = parse_pr_thread_counts(json, "llm-overlord");
    assert!(counts.is_empty());
}

/// Empty `pullRequests.nodes` (no open PRs) must yield an empty map,
/// not a panic.
#[test]
fn parse_pr_thread_counts_handles_empty_response() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {"nodes": []}
                }
            }
        }"#;
    assert!(parse_pr_thread_counts(json, "llm-overlord").is_empty());
}

/// Malformed JSON / unrelated payloads return an empty map without
/// panicking — Phase 4 must NOT crash refresh on a parse error.
#[test]
fn parse_pr_thread_counts_survives_garbage() {
    assert!(parse_pr_thread_counts("not json", "llm-overlord").is_empty());
    assert!(parse_pr_thread_counts("", "llm-overlord").is_empty());
    assert!(parse_pr_thread_counts("{}", "llm-overlord").is_empty());
}

// ── Prompt builder: issue implementation ──

#[test]
fn build_prompt_contains_issue_number_and_body() {
    let p = build_prompt(
        "test-project",
        7,
        "Add caching",
        "Implement LRU cache",
        "fn main() {}",
        0,
        "",
    );
    assert!(p.contains("test-project"));
    assert!(p.contains("Issue #7"));
    assert!(p.contains("Add caching"));
    assert!(p.contains("Implement LRU cache"));
    assert!(p.contains("fn main() {}"));
    assert!(p.contains("Codebase Snapshot"));
    assert!(p.contains("ISSUES.md"));
    assert!(p.contains("STATUS.md"));
    assert!(p.contains("Do NOT commit"));
    // No tracker section when tracker body is empty
    assert!(!p.contains("Parent Tracker"));
}

#[test]
fn build_prompt_includes_parent_tracker_when_present() {
    let tracker_body =
        "## Sprint Goal\nShip caching layer.\n- [ ] #7 Add caching\n- [ ] #8 Add eviction";
    let p = build_prompt(
        "test-project",
        7,
        "Add caching",
        "Implement LRU cache",
        "fn main() {}",
        42,
        tracker_body,
    );
    assert!(p.contains("## Parent Tracker #42"));
    assert!(p.contains("Ship caching layer."));
    assert!(p.contains("Treat the tracker as authoritative for scope"));
    assert!(p.contains("surface the conflict as a comment on the tracker"));
    // Still contains the issue content
    assert!(p.contains("Issue #7"));
    assert!(p.contains("Implement LRU cache"));
}

#[test]
fn build_prompt_no_tracker_section_when_body_empty() {
    let p = build_prompt(
        "test-project",
        7,
        "Add caching",
        "Implement LRU cache",
        "",
        99,
        "",
    );
    assert!(!p.contains("Parent Tracker"));
    assert!(!p.contains("surface the conflict"));
    assert!(p.contains("Issue #7"));
}

// ── Prompt builder: sprint planning draft vs finalize ──

#[test]
fn sprint_draft_does_not_create_issues() {
    let p = build_sprint_planning_draft_prompt(
        "test-project",
        "[issues]",
        "[prs]",
        "[status]",
        "[issues_md]",
    );
    assert!(p.contains("[issues]"));
    assert!(p.contains("[prs]"));
    assert!(p.contains("[status]"));
    assert!(p.contains("[issues_md]"));
    assert!(p.contains("Dependency Hierarchy"));
    assert!(p.contains("DRAFT"));
    assert!(!p.contains("gh issue create"));
}

#[test]
fn sprint_finalize_includes_feedback_and_creates_issues() {
    let p = build_sprint_planning_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[s]",
        "[m]",
        "focus on DX",
    );
    assert!(p.contains("focus on DX"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("ISSUES.md"));
    assert!(!p.contains("DRAFT"));
}

#[test]
fn sprint_finalize_creates_tracker_with_labels() {
    let p = build_sprint_planning_finalize_prompt("test-project", "[i]", "[p]", "[s]", "[m]", "fb");
    assert!(p.contains("--label \"sprint,tracker\""));
    assert!(p.contains("Tracked by #<tracker>"));
}

// ── Prompt builder: strategic review draft vs finalize ──

#[test]
fn strategic_draft_contains_all_perspectives() {
    let p = build_strategic_review_draft_prompt(
        "test-project",
        "[issues]",
        "[prs]",
        "[commits]",
        "[status]",
        "[issues_md]",
        "[crates]",
        "",
    );
    assert!(p.contains("Product Stakeholder"));
    assert!(p.contains("Business Analyst"));
    assert!(p.contains("Lead Engineer"));
    assert!(p.contains("UX / DX Researcher"));
    assert!(p.contains("DRAFT"));
    assert!(!p.contains("gh issue create"));
}

#[test]
fn strategic_draft_includes_all_context() {
    let p = build_strategic_review_draft_prompt(
        "test-project",
        "ISSUES_JSON",
        "PRS_JSON",
        "abc123 commit",
        "STATUS_CONTENT",
        "ISSUES_MD",
        "CRATE_LIST",
        "",
    );
    assert!(p.contains("ISSUES_JSON"));
    assert!(p.contains("PRS_JSON"));
    assert!(p.contains("abc123 commit"));
    assert!(p.contains("STATUS_CONTENT"));
    assert!(p.contains("ISSUES_MD"));
    assert!(p.contains("CRATE_LIST"));
}

#[test]
fn strategic_draft_includes_report_synthesis_when_present() {
    let p = build_strategic_review_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "Top priority: fix auth. Velocity: steady.",
    );
    assert!(p.contains("Prior Report Synthesis"));
    assert!(p.contains("Top priority: fix auth. Velocity: steady."));
}

#[test]
fn strategic_draft_omits_synthesis_when_empty() {
    let p = build_strategic_review_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
    );
    assert!(!p.contains("Prior Report Synthesis"));
}

#[test]
fn strategic_finalize_includes_feedback_and_creates_single_issue() {
    let p = build_strategic_review_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        "skip OIDC, focus on CLI",
    );
    assert!(p.contains("skip OIDC, focus on CLI"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("gh issue edit"));
    assert!(!p.contains("DRAFT"));
    // Single-issue contract: exactly one strategic-review issue, edited in place on
    // subsequent runs. No per-recommendation children, no parent tracker.
    assert!(p.contains("**exactly one** GitHub issue"));
    assert!(p.contains("--label \"strategic-review\""));
    assert!(p.contains("Do not file recommendation issues"));
}

#[test]
fn strategic_finalize_does_not_emit_tracker_layout() {
    let p = build_strategic_review_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        "fb",
    );
    // The old strategic-review,tracker layout is gone — strategic review is a single
    // living artifact, not a parent + child issue tree. Sprint Planning is the only
    // workflow that still files trackers.
    assert!(!p.contains("\"strategic-review,tracker\""));
    assert!(!p.contains("Tracked by #<tracker>"));
}

#[test]
fn strategic_draft_sets_single_issue_expectation() {
    let p = build_strategic_review_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
    );
    // The draft must tell the agent up front that finalize publishes one issue, not
    // many — otherwise it shapes the recommended path forward around per-item issues.
    assert!(p.contains("**exactly one** GitHub issue"));
    assert!(p.contains("`strategic-review` label"));
}

#[test]
fn strategic_finalize_includes_report_synthesis() {
    let p = build_strategic_review_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "synthesis content here",
        "my feedback",
    );
    assert!(p.contains("Prior Report Synthesis"));
    assert!(p.contains("synthesis content here"));
    assert!(p.contains("my feedback"));
}

// ── Prompt builder: ideation draft vs finalize ──

#[test]
fn ideation_draft_is_divergent_draft() {
    let p = build_ideation_draft_prompt("test-project", "[i]", "[p]", "[c]", "[s]", "[m]", "[t]");
    assert!(p.contains("DRAFT"));
    assert!(p.contains("Capability ideas"));
    assert!(p.contains("Foundational ideas"));
    assert!(p.contains("Provocations"));
    assert!(p.contains("Wildcards"));
    assert!(p.contains("at least 15"));
    assert!(!p.contains("gh issue create"));
}

#[test]
fn ideation_draft_includes_all_context() {
    let p = build_ideation_draft_prompt(
        "test-project",
        "ISSUES_JSON",
        "PRS_JSON",
        "abc123 commit",
        "STATUS_CONTENT",
        "ISSUES_MD",
        "CRATE_LIST",
    );
    assert!(p.contains("ISSUES_JSON"));
    assert!(p.contains("PRS_JSON"));
    assert!(p.contains("abc123 commit"));
    assert!(p.contains("STATUS_CONTENT"));
    assert!(p.contains("ISSUES_MD"));
    assert!(p.contains("CRATE_LIST"));
}

#[test]
fn ideation_finalize_includes_feedback_and_creates_issue() {
    let p = build_ideation_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "keep ideas 1-5, drop the rest",
        false,
    );
    assert!(p.contains("keep ideas 1-5, drop the rest"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("ideation"));
    assert!(!p.contains("DRAFT"));
    assert!(!p.contains("DRY RUN"));
}

#[test]
fn ideation_finalize_dry_run_includes_dry_run_note() {
    let p = build_ideation_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "feedback",
        true,
    );
    assert!(p.contains("DRY RUN"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("ideation"));
}

// ── Prompt builder: report draft vs finalize ──

#[test]
fn report_draft_is_draft_not_final() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        &sp,
    );
    assert!(p.contains("DRAFT"));
    assert!(p.contains("Executive Summary"));
    assert!(p.contains("Risk Assessment"));
    assert!(!p.contains("gh issue create"));
}

#[test]
fn report_draft_includes_ideation_when_present() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "Add WebSocket support idea",
        &sp,
    );
    assert!(p.contains("Prior Ideation"));
    assert!(p.contains("Add WebSocket support idea"));
}

#[test]
fn report_draft_includes_persona_lens() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        &sp,
    );
    assert!(p.contains(&sp.user_personas));
    assert!(p.contains("Synthesis Lens"));
    assert!(p.contains("Do NOT conflate it with other skills"));
    assert!(p.contains("`recognition_cues:`"));
    assert!(p.contains("`jobs_to_be_done:`"));
    assert!(p.contains("`pains:`"));
    assert!(p.contains("`anti_goals:`"));
    assert!(p.contains("possible persona blind"));
}

#[test]
fn report_draft_includes_persona_lens_with_custom_skill_path() {
    // Verifies that a library consumer can override the user-personas skill
    // path and have it propagate into the prompt verbatim — drop-in support
    // for prefixed skill layouts.
    let sp = crate::agent::types::SkillPaths {
        user_personas: "/custom/skills/prefixed-user-personas/SKILL.md".into(),
        issue_tracking: "/custom/skills/prefixed-issue-tracking/SKILL.md".into(),
    };
    let p = build_report_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        &sp,
    );
    assert!(p.contains("/custom/skills/prefixed-user-personas/SKILL.md"));
    assert!(!p.contains("user-personas/SKILL.md\n"));
}

#[test]
fn report_draft_omits_ideation_when_empty() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_draft_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        &sp,
    );
    assert!(!p.contains("Prior Ideation"));
}

#[test]
fn report_finalize_includes_feedback_and_synthesis() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        "add more detail on blockers",
        false,
        &sp,
    );
    assert!(p.contains("add more detail on blockers"));
    assert!(p.contains("Synthesis"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("uxr-synthesis"));
    assert!(!p.contains("REPORT_SYNTHESIS.md"));
    assert!(!p.contains("DRY RUN"));
    assert!(!p.contains("DRAFT"));
}

#[test]
fn report_finalize_includes_persona_lens_and_synthesis_attribution() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        "feedback",
        false,
        &sp,
    );
    assert!(p.contains(&sp.user_personas));
    assert!(p.contains("Synthesis Lens"));
    assert!(p.contains("Do NOT conflate it with other skills"));
    assert!(p.contains("`recognition_cues:`"));
    assert!(p.contains("`jobs_to_be_done:`"));
    assert!(p.contains("`pains:`"));
    assert!(p.contains("`anti_goals:`"));
    assert!(p.contains("possible persona blind"));
    assert!(p.contains("dominant persona signal"));
    assert!(p.contains("appeared in zero evidence"));
}

#[test]
fn report_finalize_dry_run_includes_dry_run_note() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "",
        "feedback",
        true,
        &sp,
    );
    assert!(p.contains("DRY RUN"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("uxr-synthesis"));
}

#[test]
fn report_finalize_includes_ideation_when_present() {
    let sp = crate::agent::types::SkillPaths::default();
    let p = build_report_finalize_prompt(
        "test-project",
        "[i]",
        "[p]",
        "[c]",
        "[s]",
        "[m]",
        "[t]",
        "ideation content here",
        "my feedback",
        false,
        &sp,
    );
    assert!(p.contains("Prior Ideation"));
    assert!(p.contains("ideation content here"));
    assert!(p.contains("my feedback"));
}

// ── Prompt builder: retrospective draft vs finalize ──

#[test]
fn retro_draft_contains_all_sections() {
    let p = build_retrospective_draft_prompt(
        "test-project",
        "[commits]",
        "[closed]",
        "[merged]",
        "[open_i]",
        "[open_p]",
        "[status]",
        "[issues_md]",
    );
    assert!(p.contains("What shipped"));
    assert!(p.contains("What went well"));
    assert!(p.contains("What was painful"));
    assert!(p.contains("What to change"));
    assert!(p.contains("Velocity"));
    assert!(p.contains("DRAFT"));
    assert!(!p.contains("gh issue create"));
}

#[test]
fn retro_draft_includes_all_context() {
    let p = build_retrospective_draft_prompt(
        "test-project",
        "COMMITS",
        "CLOSED",
        "MERGED",
        "OPEN_I",
        "OPEN_P",
        "STATUS",
        "ISSUES_MD",
    );
    assert!(p.contains("COMMITS"));
    assert!(p.contains("CLOSED"));
    assert!(p.contains("MERGED"));
    assert!(p.contains("OPEN_I"));
    assert!(p.contains("OPEN_P"));
    assert!(p.contains("STATUS"));
    assert!(p.contains("ISSUES_MD"));
}

#[test]
fn retro_finalize_includes_feedback_and_creates_single_issue() {
    let p = build_retrospective_finalize_prompt(
        "test-project",
        "[c]",
        "[cl]",
        "[m]",
        "[oi]",
        "[op]",
        "[s]",
        "[im]",
        "error messages need work",
    );
    assert!(p.contains("error messages need work"));
    assert!(p.contains("gh issue create"));
    assert!(p.contains("gh issue edit"));
    assert!(p.contains("ISSUES.md"));
    assert!(!p.contains("DRAFT"));
    // Single-issue contract: exactly one retrospective issue, edited in place on
    // subsequent runs. No per-action-item children, no parent tracker.
    assert!(p.contains("**exactly one** GitHub issue"));
    assert!(p.contains("--label \"retrospective\""));
    assert!(p.contains("Do not file per-action-item issues"));
}

#[test]
fn retro_draft_sets_single_issue_expectation() {
    let p = build_retrospective_draft_prompt(
        "test-project",
        "[c]",
        "[cl]",
        "[m]",
        "[oi]",
        "[op]",
        "[s]",
        "[im]",
    );
    // The draft must tell the agent up front that finalize publishes one issue,
    // not many — otherwise it shapes the draft around per-item issues.
    assert!(p.contains("**exactly one** GitHub issue"));
    assert!(p.contains("`retrospective` label"));
}

// ── Prompt builder: code review ──

#[test]
fn code_review_prompt_includes_pr_context() {
    let p = build_code_review_prompt(
        "test-project",
        42,
        "Add caching",
        "Implements LRU",
        "+fn cache()",
    );
    assert!(p.contains("test-project"));
    assert!(p.contains("Pull Request #42"));
    assert!(p.contains("Add caching"));
    assert!(p.contains("Implements LRU"));
    assert!(p.contains("+fn cache()"));
    assert!(p.contains("APPROVE"));
    assert!(p.contains("REQUEST_CHANGES"));
    assert!(p.contains("gh api"));
    assert!(p.contains("/pulls/42/reviews"));
}

#[test]
fn code_review_prompt_uses_inline_comment_schema() {
    let p = build_code_review_prompt(
        "test-project",
        42,
        "Add caching",
        "Implements LRU",
        "+fn cache()",
    );
    // Inline-comment payload schema must be present so future edits
    // can't accidentally drop the line-anchored review path.
    assert!(p.contains("\"path\""));
    assert!(p.contains("\"line\""));
    assert!(p.contains("\"side\": \"RIGHT\""));
    assert!(p.contains("\"comments\""));
    assert!(p.contains("commit_id"));
    // Must explicitly forbid the gh pr review fallback.
    assert!(p.contains("Do NOT use `gh pr review`"));
}

#[test]
fn code_review_prompt_checks_security() {
    let p = build_code_review_prompt("test-project", 1, "t", "b", "d");
    assert!(p.contains("Security"));
    assert!(p.contains("OWASP"));
}

#[test]
fn review_followup_prompt_scopes_to_outstanding_threads() {
    let threads = vec![ReviewThread {
        id: "thr1".into(),
        path: "src/lib.rs".into(),
        line: 10,
        body: "Handle the None case.".into(),
        author: DEFAULT_REVIEW_BOT_LOGIN.to_string(),
    }];
    let p = build_review_followup_code_review_prompt(
        "test-project",
        7,
        "Fix parser",
        "closes issues",
        "+foo",
        &threads,
    );
    assert!(p.contains("follow-up verification"));
    assert!(p.contains("src/lib.rs"));
    assert!(p.contains("Handle the None case."));
    assert!(p.contains("/pulls/7/reviews"));
    assert!(
        !p.contains("OWASP"),
        "follow-up prompt must not mandate full security audit"
    );
}

// ── Phase 2: Fix Comments prompt + thread parser (#144) ──

fn sample_thread(id: &str, path: &str, line: u32, body: &str) -> ReviewThread {
    ReviewThread {
        id: id.to_string(),
        path: path.to_string(),
        line,
        body: body.to_string(),
        author: DEFAULT_REVIEW_BOT_LOGIN.to_string(),
    }
}

/// Acceptance criterion from #144: "New unit tests on the prompt builder
/// asserting it includes the diff, thread bodies, and per-thread line
/// anchors."
#[test]
fn pr_review_fix_prompt_includes_diff_branch_and_thread_anchors() {
    let threads = vec![
        sample_thread(
            "PRT_kw1",
            "test-review-fixture.md",
            14,
            "Item 5 is incorrect — JWTs are signed by default, not encrypted.",
        ),
        sample_thread(
            "PRT_kw2",
            "test-review-fixture.md",
            16,
            "Item 7 is incorrect — fast-forward merges do not create a merge commit.",
        ),
    ];
    let p = build_pr_review_fix_prompt(
        "test-project",
        143,
        "test: PR review comment fixture",
        "test-pr-review-comments",
        "@@ -10,5 +10,5 @@\n-old\n+new\n",
        &threads,
    );

    // Project + PR identification.
    assert!(p.contains("test-project"));
    assert!(p.contains("Pull Request #143"));
    assert!(p.contains("test: PR review comment fixture"));

    // Branch must be embedded so the agent knows which worktree it's in.
    assert!(p.contains("test-pr-review-comments"));

    // Diff must be included verbatim inside the diff fence.
    assert!(p.contains("```diff"));
    assert!(p.contains("@@ -10,5 +10,5 @@"));
    assert!(p.contains("-old"));
    assert!(p.contains("+new"));

    // Each thread must surface its anchor (path:line), bot author, and body.
    assert!(p.contains("test-review-fixture.md:14"));
    assert!(p.contains("test-review-fixture.md:16"));
    assert!(p.contains(&format!("@{DEFAULT_REVIEW_BOT_LOGIN}")));
    assert!(p.contains("JWTs are signed by default"));
    assert!(p.contains("fast-forward merges do not create a merge commit"));

    // Thread count is reported so the agent can sanity-check coverage.
    assert!(p.contains("Unresolved Review Threads (2)"));

    // Worktree contract: do NOT commit, do NOT push, do NOT cd elsewhere.
    assert!(p.contains("Do NOT commit"));
    assert!(p.contains("Do NOT `cd`"));
}

/// A Fix Comments run with zero threads is supposed to bail out before
/// reaching the prompt builder, but if it ever does the prompt must
/// still be coherent (no panic, no missing sections).
#[test]
fn pr_review_fix_prompt_handles_empty_threads() {
    let p = build_pr_review_fix_prompt(
        "test-project",
        143,
        "fixture",
        "test-pr-review-comments",
        "diff content",
        &[],
    );
    assert!(p.contains("Unresolved Review Threads (0)"));
    assert!(p.contains("diff content"));
}

/// Fixture mirrors the shape of `gh api graphql` output for the
/// `reviewThreads` query used by [`fetch_unresolved_review_threads`].
/// Resolved threads and human-authored threads (without the opt-in
/// marker) must be filtered out so a Fix Comments run only acts on
/// findings the project's review bot raised.
#[test]
fn parse_review_threads_filters_resolved_and_human_authors() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_kw1",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 14,
                                                "originalLine": 14,
                                                "body": "Item 5 is incorrect."
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw2",
                                    "isResolved": true,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 16,
                                                "body": "already resolved"
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw3",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee"},
                                                "path": "src/foo.rs",
                                                "line": 42,
                                                "body": "human comment"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "PRT_kw1");
    assert_eq!(threads[0].path, "test-review-fixture.md");
    assert_eq!(threads[0].line, 14);
    assert_eq!(threads[0].author, "llm-overlord");
    assert_eq!(threads[0].body, "Item 5 is incorrect.");
}

/// Same fixture as [`parse_review_threads_filters_resolved_and_human_authors`]:
/// the all-authors parser must retain human inline review threads so the issue
/// runner and `fix-pr` can address requested changes.
#[test]
fn parse_all_unresolved_review_threads_includes_human_authors_without_marker() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_kw1",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 14,
                                                "originalLine": 14,
                                                "body": "Item 5 is incorrect."
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw2",
                                    "isResolved": true,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "test-review-fixture.md",
                                                "line": 16,
                                                "body": "already resolved"
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_kw3",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee"},
                                                "path": "src/foo.rs",
                                                "line": 42,
                                                "body": "human comment"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_all_unresolved_review_threads(json);
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].id, "PRT_kw1");
    assert_eq!(threads[1].id, "PRT_kw3");
    assert_eq!(threads[1].author, "geoffsee");
    assert_eq!(threads[1].body, "human comment");
}

/// GitHub apps surface as `<name>[bot]` in the GraphQL response (e.g.
/// `dependabot[bot]`). The parser must accept any author whose login
/// ends with `[bot]` so the bot-only filter doesn't depend on the
/// configured `bot_login` matching exactly.
#[test]
fn parse_review_threads_accepts_bracket_bot_suffix() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_x",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "dependabot[bot]"},
                                                "path": "Cargo.toml",
                                                "line": 5,
                                                "body": "bump"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].author, "dependabot[bot]");
}

/// Human review comments are excluded by default, but a human can opt in
/// per-comment by including [`HUMAN_FIX_MARKER`] in the body. Without
/// the marker the same author is still dropped.
#[test]
fn parse_review_threads_accepts_human_with_fix_marker() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_human_opt_in",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee", "__typename": "User"},
                                                "path": "src/lib.rs",
                                                "line": 3,
                                                "body": "@caretta fix: rename foo to bar"
                                            }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRT_human_plain",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee", "__typename": "User"},
                                                "path": "src/lib.rs",
                                                "line": 4,
                                                "body": "should we benchmark this first?"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "PRT_human_opt_in");
    assert_eq!(threads[0].author, "geoffsee");
}

/// The marker check must be case-insensitive so e.g. `@Caretta Fix` or
/// `@CARETTA FIX` at the start of a sentence still opts the thread in.
#[test]
fn parse_review_threads_marker_is_case_insensitive() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_case",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "geoffsee", "__typename": "User"},
                                                "path": "src/lib.rs",
                                                "line": 3,
                                                "body": "@Caretta Fix please address this"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
}

/// GitHub Apps acting through an installation token surface their login
/// in GraphQL without the `[bot]` suffix (e.g. `caretta-ai`) but always
/// with `__typename: "Bot"`. The parser must recognise this so reviews
/// posted by App identities are picked up even when the configured
/// `bot_login` does not match exactly.
#[test]
fn parse_review_threads_accepts_bot_typename() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_app",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "caretta-ai", "__typename": "Bot"},
                                                "path": "src/lib.rs",
                                                "line": 9,
                                                "body": "use Path not PathBuf"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].author, "caretta-ai");
}

/// Outdated threads can have `line: null`. The parser must fall back to
/// `originalLine` so the prompt still has a meaningful anchor instead of
/// dropping the thread or printing `:0`.
#[test]
fn parse_review_threads_falls_back_to_original_line() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRT_outdated",
                                    "isResolved": false,
                                    "comments": {
                                        "nodes": [
                                            {
                                                "author": {"login": "llm-overlord"},
                                                "path": "src/foo.rs",
                                                "line": null,
                                                "originalLine": 42,
                                                "body": "outdated finding"
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;
    let threads = parse_review_threads(json, "llm-overlord");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].line, 42);
}

/// An empty `reviewThreads.nodes` array (PR with no review activity) must
/// yield an empty Vec, not a panic.
#[test]
fn parse_review_threads_handles_empty_response() {
    let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": []
                        }
                    }
                }
            }
        }"#;
    assert!(parse_review_threads(json, "llm-overlord").is_empty());
}

/// Malformed JSON must not panic — the function logs a warning and
/// returns an empty Vec so the calling Fix run can bail cleanly.
#[test]
fn parse_review_threads_survives_malformed_json() {
    assert!(parse_review_threads("not json at all", "llm-overlord").is_empty());
    assert!(parse_review_threads("", "llm-overlord").is_empty());
}

// ── Phase 3: resolveReviewThread mutation parser (#145) ──

/// The mutation query string itself must contain the resolveReviewThread
/// operation name and the threadId variable so a future refactor can't
/// silently degrade it into a no-op.
#[test]
fn resolve_review_thread_mutation_targets_correct_operation() {
    assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("resolveReviewThread"));
    assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("$threadId: ID!"));
    assert!(RESOLVE_REVIEW_THREAD_MUTATION.contains("isResolved"));
}

/// The acceptance criterion in #145: a successful mutation response with
/// `isResolved: true` returns true.
#[test]
fn parse_resolve_review_thread_response_accepts_success() {
    let json = r#"{
            "data": {
                "resolveReviewThread": {
                    "thread": { "id": "PRT_kw1", "isResolved": true }
                }
            }
        }"#;
    assert!(parse_resolve_review_thread_response(json));
}

/// `isResolved: false` in the response means the mutation succeeded at
/// the API level but the thread did not flip to resolved (e.g. already
/// merged, permission edge case). Treat as failure so the caller logs
/// it and the user can investigate.
#[test]
fn parse_resolve_review_thread_response_rejects_unresolved() {
    let json = r#"{
            "data": {
                "resolveReviewThread": {
                    "thread": { "id": "PRT_kw1", "isResolved": false }
                }
            }
        }"#;
    assert!(!parse_resolve_review_thread_response(json));
}

/// A GraphQL `errors` response with no `data` payload must surface as
/// failure, not panic.
#[test]
fn parse_resolve_review_thread_response_rejects_graphql_error() {
    let json = r#"{
            "errors": [
                { "message": "Resource not accessible by integration", "type": "FORBIDDEN" }
            ]
        }"#;
    assert!(!parse_resolve_review_thread_response(json));
}

/// Malformed JSON / empty bodies / unrelated payloads return false
/// without panicking — Phase 3 must NOT abort the Fix run on a parse
/// error since the fix is already pushed.
#[test]
fn parse_resolve_review_thread_response_survives_garbage() {
    assert!(!parse_resolve_review_thread_response("not json"));
    assert!(!parse_resolve_review_thread_response(""));
    assert!(!parse_resolve_review_thread_response("{}"));
    assert!(!parse_resolve_review_thread_response(
        r#"{"data": {"unrelated": true}}"#
    ));
}

// ── Prompt builder: fix prompt ──

#[test]
fn fix_prompt_includes_output() {
    let p = build_fix_prompt(5, "error: cannot find type");
    assert!(p.contains("issue #5"));
    assert!(p.contains("error: cannot find type"));
    assert!(p.contains("Do NOT commit"));
}

// ── build_security_review_prompt ──

#[test]
fn security_review_prompt_with_snapshot() {
    let prompt = build_security_review_prompt(
        "test-project",
        "compute-node\nedge-node",
        "fn main() {}",
        false,
    );
    assert!(prompt.contains("test-project"));
    assert!(prompt.contains("compute-node"));
    assert!(prompt.contains("edge-node"));
    assert!(prompt.contains("Codebase Snapshot"));
    assert!(prompt.contains("fn main() {}"));
    assert!(!prompt.contains("Read the codebase directly"));
}

#[test]
fn security_review_prompt_without_snapshot() {
    let prompt = build_security_review_prompt("test-project", "compute-node", "", false);
    assert!(prompt.contains("compute-node"));
    assert!(prompt.contains("Read the codebase directly"));
    assert!(!prompt.contains("Codebase Snapshot"));
}

#[test]
fn security_review_prompt_creates_tracker_with_labels() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(prompt.contains("--label \"security,tracker\""));
    assert!(prompt.contains("Tracked by #<tracker>"));
    assert!(prompt.contains("Tracker Issue"));
    assert!(prompt.contains("Actionable Findings"));
}

#[test]
fn security_review_prompt_creates_per_finding_issues() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(prompt.contains("gh issue create"));
    assert!(prompt.contains("gh issue edit"));
    assert!(prompt.contains("security:"));
    assert!(prompt.contains("severity:critical"));
    assert!(prompt.contains("severity:high"));
    assert!(prompt.contains("severity:medium"));
}

#[test]
fn security_review_prompt_duplicate_detection() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(prompt.contains("Duplicate Detection"));
    assert!(prompt.contains("gh issue list --label security --search"));
    assert!(prompt.contains("Already tracked"));
}

#[test]
fn security_review_prompt_low_info_rollup() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(prompt.contains("Low / Informational Findings"));
    assert!(prompt.contains("rollup"));
    assert!(prompt.contains("severity:low"));
}

#[test]
fn security_review_prompt_cross_reference_summary() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(prompt.contains("Cross-Reference Summary"));
    assert!(prompt.contains("Filed:"));
}

#[test]
fn security_review_prompt_dry_run() {
    let prompt = build_security_review_prompt("test-project", "compute-node", "fn main() {}", true);
    assert!(prompt.contains("DRY RUN MODE"));
    assert!(prompt.contains("[dry-run]"));
    assert!(prompt.contains("Do NOT execute any `gh issue create`"));
}

#[test]
fn security_review_prompt_no_dry_run_section_when_false() {
    let prompt =
        build_security_review_prompt("test-project", "compute-node", "fn main() {}", false);
    assert!(!prompt.contains("DRY RUN MODE"));
}

#[test]
fn refresh_agents_prompt_limits_scope_and_requires_summary_block() {
    let prompt = build_refresh_agents_prompt(
        "test-project",
        &[
            "AGENTS.md".to_string(),
            "skills/testing/SKILL.md".to_string(),
        ],
    );
    assert!(prompt.contains("test-project"));
    assert!(prompt.contains("AGENTS.md"));
    assert!(prompt.contains("skills/testing/SKILL.md"));
    assert!(prompt.contains("Do NOT edit source code"));
    assert!(prompt.contains("REFRESH_AGENTS_SUMMARY_BEGIN"));
    assert!(prompt.contains("REFRESH_AGENTS_NO_CHANGES"));
    assert!(prompt.contains("Do NOT commit, push, or open a pull request"));
}

#[test]
fn refresh_docs_prompt_limits_scope_and_requires_summary_block() {
    let prompt = build_refresh_docs_prompt(
        "test-project",
        &[
            "README.md".to_string(),
            "STATUS.md".to_string(),
            "docs/ARCHITECTURE.md".to_string(),
        ],
    );
    assert!(prompt.contains("test-project"));
    assert!(prompt.contains("README.md"));
    assert!(prompt.contains("STATUS.md"));
    assert!(prompt.contains("docs/ARCHITECTURE.md"));
    assert!(prompt.contains("Do NOT edit source code"));
    assert!(prompt.contains("REFRESH_DOCS_SUMMARY_BEGIN"));
    assert!(prompt.contains("REFRESH_DOCS_NO_CHANGES"));
    assert!(prompt.contains("Do NOT commit, push, or open a pull request"));
}

// ── parse_auto_merge_response ──

#[test]
fn auto_merge_null_is_disabled() {
    assert!(!parse_auto_merge_response(Some("null".into())));
}

#[test]
fn auto_merge_empty_is_disabled() {
    assert!(!parse_auto_merge_response(Some(String::new())));
}

#[test]
fn auto_merge_none_is_disabled() {
    assert!(!parse_auto_merge_response(None));
}

#[test]
fn auto_merge_json_object_is_enabled() {
    assert!(parse_auto_merge_response(Some(
        r#"{"mergeMethod":"SQUASH"}"#.into()
    )));
}

// ── find_upstream_branch ──

#[test]
fn upstream_branch_no_blockers() {
    assert_eq!(
        find_upstream_branch(&[]),
        crate::agent::cmd::origin_default_branch()
    );
}

// ── Housekeeping prompt builders ──

#[test]
fn housekeeping_draft_prompt_contains_all_sweep_categories() {
    let prompt = build_housekeeping_draft_prompt(
        "test-project",
        "[]",
        "[]",
        "master\nagent/issue-1",
        "- [ ] #1 task",
        "| Feature | ✅ |",
        "# ISSUES",
    );
    assert!(prompt.contains("Tracker Drift"));
    assert!(prompt.contains("Stale Issues"));
    assert!(prompt.contains("Stale Pull Requests"));
    assert!(prompt.contains("Orphaned Local Branches"));
    assert!(prompt.contains("Generated / Orphaned Files"));
    assert!(prompt.contains("Label Taxonomy Drift"));
    assert!(prompt.contains("ISSUES.md / STATUS.md Drift"));
    assert!(prompt.contains("READ-ONLY audit"));
    assert!(prompt.contains("Do NOT modify anything"));
}

#[test]
fn housekeeping_draft_prompt_includes_context() {
    let prompt = build_housekeeping_draft_prompt(
        "test-project",
        "[{\"number\":42}]",
        "[{\"number\":10}]",
        "master\nagent/issue-42",
        "- [ ] #42 task",
        "status content",
        "issues content",
    );
    assert!(prompt.contains("[{\"number\":42}]"));
    assert!(prompt.contains("agent/issue-42"));
    assert!(prompt.contains("- [ ] #42 task"));
}

#[test]
fn housekeeping_finalize_prompt_contains_feedback() {
    let prompt = build_housekeeping_finalize_prompt(
        "test-project",
        "[]",
        "[]",
        "master",
        "",
        "",
        "",
        "Fix tracker drift only, skip everything else",
    );
    assert!(prompt.contains("Fix tracker drift only, skip everything else"));
    assert!(prompt.contains("Execution Order"));
    assert!(prompt.contains("NEVER delete unmerged branches"));
    assert!(prompt.contains("housekeeping"));
}

// ── Tracker drift detection (unit test for sweep category 1) ──

#[test]
fn detect_tracker_drift_closed_children_unchecked() {
    // Simulates a tracker body where issue #5 is listed as unchecked
    // but in reality the issue is closed. The housekeeping sweep should
    // detect this as "Closed children not checked off".
    let tracker_body = "- [ ] #5 Implement feature\n- [x] #6 Setup CI\n- [ ] #7 Add tests";
    let completed = parse_completed(tracker_body);
    let pending = parse_pending(tracker_body);

    // #5 and #7 are pending (unchecked)
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().any(|p| p.number == 5));
    assert!(pending.iter().any(|p| p.number == 7));
    // #6 is completed (checked)
    assert!(completed.contains(&6));
    assert!(!completed.contains(&5));

    // If we know issue #5 is closed (state:closed), it should be flagged
    // as tracker drift — the checkbox should be ticked.
    let closed_issues: HashSet<u32> = HashSet::from([5]);
    let drifted: Vec<&PendingIssue> = pending
        .iter()
        .filter(|p| closed_issues.contains(&p.number))
        .collect();
    assert_eq!(drifted.len(), 1);
    assert_eq!(drifted[0].number, 5);
}

// ── Verification verdict parser ──

#[test]
fn parse_verification_verdict_splits_verified_and_unverified() {
    let json = r#"{
            "verified": ["PRT_a", "PRT_b"],
            "unverified": [
                {"id": "PRT_c", "reason": "fix touches wrong file"}
            ]
        }"#;
    let v = parse_verification_verdict(json).expect("parses");
    assert_eq!(v.verified, vec!["PRT_a", "PRT_b"]);
    assert_eq!(v.unverified.len(), 1);
    assert_eq!(v.unverified[0].id, "PRT_c");
    assert_eq!(v.unverified[0].reason, "fix touches wrong file");
}

#[test]
fn parse_verification_verdict_handles_missing_fields() {
    let v = parse_verification_verdict(r#"{}"#).expect("parses");
    assert!(v.verified.is_empty());
    assert!(v.unverified.is_empty());
}

#[test]
fn parse_verification_verdict_rejects_garbage() {
    assert!(parse_verification_verdict("not json").is_none());
    assert!(parse_verification_verdict("").is_none());
}

#[test]
fn build_pr_review_verification_prompt_includes_thread_ids_and_output_path() {
    let threads = vec![
        ReviewThread {
            id: "PRT_x".to_string(),
            path: "src/lib.rs".to_string(),
            line: 12,
            body: "guard against panic".to_string(),
            author: "llm-overlord".to_string(),
        },
        ReviewThread {
            id: "PRT_y".to_string(),
            path: "src/main.rs".to_string(),
            line: 7,
            body: "use anyhow::Context".to_string(),
            author: "llm-overlord".to_string(),
        },
    ];
    let prompt = build_pr_review_verification_prompt(
        "caretta",
        42,
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        &threads,
        "/tmp/verify.json",
    );
    assert!(prompt.contains("PRT_x"));
    assert!(prompt.contains("PRT_y"));
    assert!(prompt.contains("/tmp/verify.json"));
    assert!(prompt.contains("verified"));
    assert!(prompt.contains("unverified"));
}
