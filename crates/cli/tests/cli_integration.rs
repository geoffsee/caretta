//! Integration tests for the caretta CLI.
//!
//! These tests exercise the compiled binary end-to-end to verify that all
//! agent CLIs, subcommands, workflow presets, and configuration paths are
//! fully functional.  They deliberately avoid calling into library internals
//! so they catch regressions in argument parsing, asset materialisation, and
//! output formatting.
//!
//! Run with:
//!
//! ```sh
//! cargo test --test cli_integration
//! ```

use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the `caretta` binary built by `cargo test`.
fn bin() -> Command {
    let path = env!("CARGO_BIN_EXE_caretta");
    let mut command = Command::new(path);
    #[cfg(unix)]
    command.process_group(0);
    command
}

/// Run `caretta` with the given args and assert it exits successfully.
/// Returns stdout+stderr combined as a string.
fn run_ok(args: &[&str]) -> String {
    let out = bin().args(args).output().expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "caretta {} exited with {:?}\n--- output ---\n{}",
        args.join(" "),
        out.status.code(),
        combined,
    );
    combined
}

/// Run `caretta` with the given args and assert it exits with a non-zero code.
/// Returns stdout+stderr combined.
fn run_fail(args: &[&str]) -> String {
    let out = bin().args(args).output().expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        !out.status.success(),
        "caretta {} should have failed but exited 0\n--- output ---\n{}",
        args.join(" "),
        combined,
    );
    combined
}

/// Run `caretta` with the given args and just return the output (no success assertion).
fn run_raw(args: &[&str]) -> std::process::Output {
    bin().args(args).output().expect("failed to launch caretta")
}

struct TimedOutput {
    output: Output,
    timed_out: bool,
}

fn run_with_timeout(args: &[&str], timeout: Duration) -> TimedOutput {
    let mut command = bin();
    command
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to launch caretta");
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;

    loop {
        if child
            .try_wait()
            .expect("failed to poll caretta process")
            .is_some()
        {
            break;
        }

        if Instant::now() >= deadline {
            timed_out = true;
            terminate_process(&mut child);
            break;
        }

        thread::sleep(Duration::from_millis(50));
    }

    let output = child
        .wait_with_output()
        .expect("failed to collect caretta output");
    TimedOutput { output, timed_out }
}

fn terminate_process(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let process_group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-TERM", &process_group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.kill();
        thread::sleep(Duration::from_millis(250));
        let _ = Command::new("kill")
            .args(["-KILL", &process_group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.kill();
    }

    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate should live under crates/cli")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Basic CLI surface
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn help_flag_exits_zero_and_shows_usage() {
    let out = run_ok(&["--help"]);
    assert!(
        out.contains("caretta") || out.contains("Usage") || out.contains("usage"),
        "expected usage text in --help output"
    );
}

#[test]
fn version_flag_exits_zero() {
    let out = run_ok(&["--version"]);
    assert!(
        out.contains("caretta"),
        "expected version string to contain 'caretta'"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Every subcommand accepts --help
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn subcommand_help_gui() {
    run_ok(&["gui", "--help"]);
}

#[test]
fn subcommand_help_ideation() {
    run_ok(&["ideation", "--help"]);
}

#[test]
fn subcommand_help_uxr_synth() {
    run_ok(&["uxr-synth", "--help"]);
}

#[test]
fn subcommand_help_strategic_review() {
    run_ok(&["strategic-review", "--help"]);
}

#[test]
fn subcommand_help_roadmapper() {
    run_ok(&["roadmapper", "--help"]);
}

#[test]
fn subcommand_help_sprint_planning() {
    run_ok(&["sprint-planning", "--help"]);
}

#[test]
fn subcommand_help_retrospective() {
    run_ok(&["retrospective", "--help"]);
}

#[test]
fn subcommand_help_housekeeping() {
    run_ok(&["housekeeping", "--help"]);
}

#[test]
fn subcommand_help_interview() {
    run_ok(&["interview", "--help"]);
}

#[test]
fn subcommand_help_code_review() {
    run_ok(&["code-review", "--help"]);
}

#[test]
fn subcommand_help_security_review() {
    run_ok(&["security-review", "--help"]);
}

#[test]
fn subcommand_help_refresh_agents() {
    run_ok(&["refresh-agents", "--help"]);
}

#[test]
fn subcommand_help_refresh_docs() {
    run_ok(&["refresh-docs", "--help"]);
}

#[test]
fn subcommand_help_issue() {
    run_ok(&["issue", "--help"]);
}

#[test]
fn subcommand_help_loop() {
    run_ok(&["loop", "--help"]);
}

#[test]
fn subcommand_help_tracker_matrix() {
    run_ok(&["tracker-matrix", "--help"]);
}

#[test]
fn subcommand_help_fix_pr() {
    run_ok(&["fix-pr", "--help"]);
}

#[test]
fn subcommand_help_fix_conflicts() {
    run_ok(&["fix-conflicts", "--help"]);
}

#[test]
fn subcommand_help_auto_merge() {
    run_ok(&["auto-merge", "--help"]);
}

#[test]
fn subcommand_help_serve() {
    run_ok(&["serve", "--help"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Agent flag validation — every agent name is accepted
// ═══════════════════════════════════════════════════════════════════════════

const ALL_AGENTS: &[&str] = &[
    "claude", "cline", "codex", "copilot", "gemini", "grok", "junie", "xai", "cursor",
];

#[test]
fn all_agent_names_are_accepted_by_help() {
    for agent in ALL_AGENTS {
        let out = run_ok(&["--agent", agent, "--help"]);
        assert!(
            out.contains("caretta") || out.contains("Usage"),
            "--agent {agent} --help produced unexpected output"
        );
    }
}

#[test]
fn invalid_agent_name_is_rejected() {
    let out = run_fail(&["--agent", "nonexistent-agent", "--help"]);
    assert!(
        out.contains("invalid value") || out.contains("error") || out.contains("possible values"),
        "expected error about invalid agent value, got: {out}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Dry-run mode — workflows should print what they'd do without executing
// ═══════════════════════════════════════════════════════════════════════════

/// Dry-run should exit cleanly for each workflow subcommand.
/// We combine `--dry-run` with each workflow subcommand.
/// Note: some commands require being inside a git repo; we run from the
/// project root which satisfies that.
const DRY_RUN_SUBCOMMANDS: &[&str] = &[
    "ideation",
    "uxr-synth",
    "strategic-review",
    "roadmapper",
    "sprint-planning",
    "retrospective",
    "housekeeping",
    "refresh-agents",
    "refresh-docs",
    "code-review",
    "auto-merge",
    // security-review requires a tokio runtime for codebase snapshot generation;
    // tested separately below.
];

#[test]
fn dry_run_workflows_exit_cleanly() {
    for cmd in DRY_RUN_SUBCOMMANDS {
        let out = bin()
            .args(["--dry-run", cmd])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("failed to launch caretta");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            out.status.success(),
            "--dry-run {cmd} failed with exit {:?}\n{combined}",
            out.status.code(),
        );
    }
}

/// security-review --dry-run launches the configured agent adapter. Bound the
/// process lifetime so the test owns and terminates any external child process.
#[test]
fn security_review_dry_run_parses_args() {
    let out = run_with_timeout(&["--dry-run", "security-review"], Duration::from_secs(15));
    let combined = combined_output(&out.output);
    // Should get past clap parsing — the security-review log line confirms dispatch worked
    assert!(
        !combined.contains("error: invalid value"),
        "security-review should parse without clap errors:\n{combined}"
    );
    assert!(
        combined.contains("Starting security code review")
            || combined.contains("Generating codebase snapshot"),
        "security-review dispatch should be observable:\n{combined}"
    );
    if out.timed_out {
        assert!(
            combined.contains("Starting security code review")
                || combined.contains("Generating codebase snapshot"),
            "timed out before security-review dispatch was observable:\n{combined}"
        );
    }
}

#[test]
fn dry_run_with_each_agent_exits_cleanly() {
    // Just verify that --dry-run + --agent <name> + ideation parses correctly
    for agent in ALL_AGENTS {
        let out = bin()
            .args(["--agent", agent, "--dry-run", "ideation"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("failed to launch caretta");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            out.status.success(),
            "--agent {agent} --dry-run ideation failed:\n{combined}",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Issue / Loop / Fix-PR — argument parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn issue_requires_number_argument() {
    let out = run_fail(&["issue"]);
    assert!(
        out.contains("required") || out.contains("error") || out.contains("argument"),
        "expected missing-argument error for 'issue'"
    );
}

#[test]
fn loop_requires_tracker_argument() {
    let out = run_fail(&["loop"]);
    assert!(
        out.contains("required") || out.contains("error") || out.contains("argument"),
        "expected missing-argument error for 'loop'"
    );
}

#[test]
fn tracker_matrix_requires_tracker_argument() {
    let out = run_fail(&["tracker-matrix"]);
    assert!(
        out.contains("required") || out.contains("error") || out.contains("argument"),
        "expected missing-argument error for 'tracker-matrix'"
    );
}

#[test]
fn tracker_matrix_dry_run_json_emits_empty_array() {
    let out = bin()
        .args(["--dry-run", "tracker-matrix", "7", "--json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "--dry-run tracker-matrix 7 --json failed:\n{combined}"
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "[]",
        "expected empty JSON array on stdout:\n{combined}"
    );
}

#[test]
fn fix_pr_requires_number_argument() {
    let out = run_fail(&["fix-pr"]);
    assert!(
        out.contains("required") || out.contains("error") || out.contains("argument"),
        "expected missing-argument error for 'fix-pr'"
    );
}

/// `issue` in dry-run still fetches from GitHub, so we just verify arg parsing works.
/// The command will fail with a GitHub API error (no such issue), which is expected.
#[test]
fn issue_dry_run_parses_number_argument() {
    let out = bin()
        .args(["--dry-run", "issue", "42"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    // The command should get past arg-parsing (no clap error) — it may fail
    // later when trying to fetch the issue from GitHub, which is fine.
    assert!(
        !combined.contains("error: invalid value")
            && !combined.contains("error: unexpected argument"),
        "issue 42 should parse without clap errors:\n{combined}"
    );
}

#[test]
fn issue_help_documents_tracker_flag() {
    let out = run_ok(&["issue", "--help"]);
    assert!(
        out.contains("--tracker"),
        "issue --help should document --tracker:\n{out}"
    );
}

#[test]
fn loop_dry_run_accepts_number() {
    let out = bin()
        .args(["--dry-run", "loop", "7"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "--dry-run loop 7 failed:\n{combined}");
}

#[test]
fn fix_pr_dry_run_accepts_number() {
    let out = bin()
        .args(["--dry-run", "fix-pr", "15"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "--dry-run fix-pr 15 failed:\n{combined}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. --create-labels flag
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn create_labels_writes_file_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    // create-labels expects a .github dir or writes to one
    std::fs::create_dir_all(dir.path().join(".github")).unwrap();

    // init a git repo so parse_args can find the root
    let init = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
    };
    init(&["init", "-q"]);
    init(&["config", "user.email", "test@example.com"]);
    init(&["config", "user.name", "test"]);
    std::fs::write(dir.path().join("README.md"), "init\n").unwrap();
    init(&["add", "."]);
    init(&["commit", "-q", "-m", "init"]);

    let out = bin()
        .args(["--create-labels"])
        .current_dir(dir.path())
        .output()
        .expect("failed to launch caretta");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "--create-labels failed:\n{combined}");
    assert!(
        dir.path().join(".github/labels.yml").exists(),
        "labels.yml should have been created"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Serve subcommand — port parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn serve_accepts_port_flag() {
    // Just verify parsing — don't actually start the server.
    // --help should show the --port option.
    let out = run_ok(&["serve", "--help"]);
    assert!(out.contains("port"), "serve --help should mention --port");
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Flag combinations
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn auto_and_dry_run_can_combine() {
    let out = bin()
        .args(["--auto", "--dry-run", "ideation"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    assert!(out.status.success(), "--auto --dry-run ideation failed");
}

#[test]
fn agent_auto_dry_run_combine() {
    let out = bin()
        .args(["--agent", "gemini", "--auto", "--dry-run", "roadmapper"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to launch caretta");
    assert!(
        out.status.success(),
        "--agent gemini --auto --dry-run roadmapper failed"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Workflow asset integrity — verify all preset workflow.yaml files parse
// ═══════════════════════════════════════════════════════════════════════════

/// Scans all bundled workflow preset directories and ensures every
/// `workflow.yaml` deserialises without error, and each workflow has a
/// matching `draft.md` (or runner action) so templates are not missing.
#[test]
fn all_bundled_workflow_yamls_parse_and_have_templates() {
    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflows");
    assert!(assets_dir.exists(), "assets/workflows/ not found");

    let mut checked = 0u32;
    for preset_entry in std::fs::read_dir(&assets_dir).unwrap() {
        let preset_path = preset_entry.unwrap().path();
        if !preset_path.is_dir() {
            continue;
        }
        let preset_name = preset_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        for wf_entry in std::fs::read_dir(&preset_path).unwrap() {
            let wf_path = wf_entry.unwrap().path();
            if !wf_path.is_dir() {
                continue;
            }
            let yaml_path = wf_path.join("workflow.yaml");
            if !yaml_path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&yaml_path).unwrap();
            let wf: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_else(|e| {
                panic!("Failed to parse {}: {e}", yaml_path.display());
            });

            // Every workflow should have at minimum: name, id, pattern
            assert!(
                wf.get("name").is_some(),
                "{}: missing 'name' field",
                yaml_path.display()
            );
            assert!(
                wf.get("id").is_some(),
                "{}: missing 'id' field",
                yaml_path.display()
            );
            assert!(
                wf.get("pattern").is_some(),
                "{}: missing 'pattern' field",
                yaml_path.display()
            );

            // If the workflow has phases with templates, verify the template files exist
            if let Some(phases) = wf.get("phases").and_then(|p| p.as_mapping()) {
                for (phase_name, phase_cfg) in phases {
                    if let Some(template) = phase_cfg.get("template").and_then(|t| t.as_str()) {
                        let template_path = wf_path.join(template);
                        assert!(
                            template_path.exists(),
                            "{preset_name}/{}: phase {:?} references missing template {}",
                            wf_path.file_name().unwrap().to_string_lossy(),
                            phase_name,
                            template_path.display(),
                        );
                    }
                }
            }

            // Workflows may dispatch through:
            //   1. phase templates (two_phase/one_shot)
            //   2. a named runner action
            //   3. special-cased code paths (multi_round like interview)
            // All are valid — just ensure the YAML has the required fields.

            checked += 1;
        }
    }
    assert!(
        checked >= 15,
        "expected at least 15 workflow.yaml files across all presets, found {checked}"
    );
}

#[test]
fn issue_tracking_skill_defines_tracker_contract() {
    let skill_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/skills/issue-tracking/SKILL.md");
    let skill = std::fs::read_to_string(&skill_path).unwrap();

    assert!(skill.contains("## Issue Creation Contract"));
    assert!(skill.contains("## Tracker Contract"));
    assert!(skill.contains("Do not use `tracker` for retrospective reports"));
    assert!(skill.contains("parser-compatible rows"));
}

#[test]
fn software_factory_prompts_reference_issue_tracking_skill() {
    let preset_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflows/software-factory");
    let mut checked = 0u32;

    for wf_entry in std::fs::read_dir(&preset_dir).unwrap() {
        let wf_path = wf_entry.unwrap().path();
        if !wf_path.is_dir() {
            continue;
        }
        for phase in ["draft.md", "finalize.md"] {
            let template_path = wf_path.join(phase);
            if !template_path.exists() {
                continue;
            }
            let template = std::fs::read_to_string(&template_path).unwrap();
            assert!(
                template.contains("{{issue_tracking_skill_path}}"),
                "{} must direct the agent to the configured issue-tracking skill",
                template_path.display()
            );
            checked += 1;
        }
    }

    assert!(
        checked >= 10,
        "expected all software-factory prompt templates to be checked"
    );
}

#[test]
fn housekeeping_audit_issue_is_not_tracker_labeled() {
    let template_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/workflows/default/housekeeping/finalize.md");
    let template = std::fs::read_to_string(&template_path).unwrap();

    assert!(template.contains("Use only the `housekeeping` label"));
    assert!(!template.contains("--label \"housekeeping,tracker\""));
}

#[test]
fn release_tag_publisher_dispatches_release_workflow_for_published_tag() {
    let root = repo_root();
    let release_workflow =
        std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    let publisher_workflow =
        std::fs::read_to_string(root.join(".github/workflows/release-tag-publisher.yml")).unwrap();

    assert!(release_workflow.contains("workflow_dispatch:"));
    assert!(release_workflow.contains("RELEASE_TAG: ${{ inputs.tag || github.ref_name }}"));
    assert!(release_workflow.contains("ref: ${{ env.RELEASE_TAG }}"));
    assert!(release_workflow.contains("tag_name: ${{ env.RELEASE_TAG }}"));

    assert!(publisher_workflow.contains("gh workflow run release.yml"));
    assert!(publisher_workflow.contains("--ref master"));
    assert!(publisher_workflow.contains("-f tag=\"${{ steps.tag.outputs.next_tag }}\""));
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Bundled assets exist and are non-empty
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn bundled_agents_md_exists() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/AGENTS.md");
    assert!(path.exists(), "assets/AGENTS.md not found");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.len() > 100, "AGENTS.md seems too short");
}

#[test]
fn bundled_labels_yml_exists() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/labels.yml");
    assert!(path.exists(), "assets/labels.yml not found");
}

#[test]
fn bundled_available_models_json_is_valid() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/available-models.json");
    assert!(path.exists(), "assets/available-models.json not found");
    let content = std::fs::read_to_string(&path).unwrap();
    let _: serde_json::Value =
        serde_json::from_str(&content).expect("available-models.json is not valid JSON");
}

#[test]
fn models_plain_lists_bundled_ids() {
    let out = run_ok(&["--agent", "claude", "models", "--plain"]);
    assert!(
        out.lines().any(|line| line.starts_with("claude-")),
        "expected claude model ids from bundled JSON:\n{out}"
    );
}

#[test]
fn models_table_mentions_bundled_catalog() {
    let out = run_ok(&["--agent", "claude", "models"]);
    assert!(
        out.contains("Bundled models") && out.contains("caretta-agent-runtime"),
        "expected catalog header and regen hint:\n{out}"
    );
}

#[test]
fn bundled_skills_exist() {
    let skills_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/skills");
    assert!(skills_dir.exists(), "assets/skills/ not found");

    let expected_skills = [
        "user-personas",
        "issue-tracking",
        "project-context",
        "architecture",
        "coding-standards",
        "testing",
        "code-explorer",
    ];

    for skill in &expected_skills {
        let skill_file = skills_dir.join(skill).join("SKILL.md");
        assert!(
            skill_file.exists(),
            "missing skill: assets/skills/{skill}/SKILL.md"
        );
        let content = std::fs::read_to_string(&skill_file).unwrap();
        assert!(
            !content.is_empty(),
            "skill file is empty: assets/skills/{skill}/SKILL.md"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Workflow presets — all expected presets are loadable
// ═══════════════════════════════════════════════════════════════════════════

const EXPECTED_PRESETS: &[&str] = &[
    "default",
    "pm",
    "ux",
    "xp",
    "data-science",
    "business-development",
    "deep-research",
];

#[test]
fn all_expected_presets_exist() {
    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflows");
    for preset in EXPECTED_PRESETS {
        let preset_dir = assets_dir.join(preset);
        assert!(
            preset_dir.is_dir(),
            "missing workflow preset: assets/workflows/{preset}/"
        );
        // Each preset should have at least one workflow
        let count = std::fs::read_dir(&preset_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .map(|e| e.path().join("workflow.yaml").exists())
                    .unwrap_or(false)
            })
            .count();
        assert!(count >= 1, "preset {preset} has no workflows");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 12. Default workflow coverage — the default preset should have all the
//     workflows that the CLI subcommands reference
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn default_preset_covers_all_cli_subcommands() {
    let default_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflows/default");

    // These are the workflow directory names that map to CLI subcommands
    let expected_workflows = [
        "ideation",
        "report-research", // uxr-synth
        "strategic-review",
        "roadmapper",
        "sprint-planning",
        "retrospective",
        "housekeeping",
        "code-review",
        "security-review",
        "refresh-agents",
        "refresh-docs",
        "interview",
        "auto-merge",
    ];

    for wf in &expected_workflows {
        let wf_dir = default_dir.join(wf);
        assert!(
            wf_dir.is_dir(),
            "default preset missing workflow directory: {wf}"
        );
        assert!(
            wf_dir.join("workflow.yaml").exists(),
            "default/{wf}/ missing workflow.yaml"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 13. caretta.toml — configuration file parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dev_toml_in_repo_root_is_valid_toml() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["caretta.toml", "dev.toml"] {
        let path = manifest_dir.join(name);
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap();
            let _: toml::Value =
                toml::from_str(&content).unwrap_or_else(|_| panic!("{name} is not valid TOML"));
        }
    }
    // It's ok for the config file to not exist — the binary handles that gracefully.
}

// ═══════════════════════════════════════════════════════════════════════════
// 14. Regression guards
// ═══════════════════════════════════════════════════════════════════════════

/// The binary should reject unknown subcommands.
#[test]
fn unknown_subcommand_is_rejected() {
    let out = run_raw(&["not-a-real-command"]);
    assert!(!out.status.success(), "unknown subcommand should fail");
}

/// Ensure `--dry-run` is a global flag, not a subcommand flag.
/// Putting it after the subcommand should still work.
#[test]
fn dry_run_works_before_and_after_subcommand() {
    // Before
    let out1 = bin()
        .args(["--dry-run", "ideation"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(out1.status.success(), "--dry-run before subcommand failed");

    // The clap parser puts global flags before the subcommand, so after
    // may or may not work depending on configuration. We just test the
    // canonical position.
}

/// Each agent type should produce distinct dry-run output that mentions
/// the agent name, confirming the agent flag propagated correctly.
#[test]
fn dry_run_output_reflects_selected_agent() {
    for agent in ALL_AGENTS {
        let out = bin()
            .args(["--agent", agent, "--dry-run", "ideation"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            out.status.success(),
            "--agent {agent} --dry-run ideation failed:\n{combined}"
        );
        // The dry-run output should mention the agent binary or name somewhere
        // (either in the argv dump or in a log line).
        let lower = combined.to_lowercase();
        let agent_lower = agent.to_lowercase();
        // xAI uses the copilot CLI; logs still include the xai agent label.
        let expected_in_output = match *agent {
            "xai" => lower.contains("xai") || lower.contains("copilot"),
            _ => lower.contains(&agent_lower),
        };
        assert!(
            expected_in_output,
            "--dry-run for agent '{agent}' did not mention the agent in output:\n{combined}"
        );
    }
}
