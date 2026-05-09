//! # freq-ai
//!
//! Library entry point for the freq-ai dev agent. The standalone `freq-ai`
//! binary is a thin shim around [`run_with_overrides`]; library consumers
//! (e.g. project-specific shims that want to inject a custom skill layout)
//! should call [`run_with_overrides`] with a closure that mutates the
//! [`Config`] before dispatch.
//!
//! ## Example
//!
//! ```no_run
//! use freq_ai::SkillPaths;
//!
//! freq_ai::run_with_overrides(|config| {
//!     config.skill_paths = SkillPaths {
//!         user_personas: "/custom/skills/user-personas/SKILL.md".into(),
//!         issue_tracking: "/custom/skills/issue-tracking/SKILL.md".into(),
//!     };
//! });
//! ```
#![allow(non_snake_case)]

pub mod agent;
pub mod custom_themes;
pub mod ui;

pub use agent::types::{Agent, Config, SkillPaths};

use agent::actions::{ActionContext, lookup_action};
use agent::config_store::{
    clear_bot_private_key_pem, clear_bot_token, clear_local_inference_api_key,
    store_bot_private_key_pem, store_bot_token, store_local_inference_api_key,
};
use agent::shell::{
    clear_stop_request, list_all_files, parse_args, preflight, record_agent_response, request_stop,
    reset_chat_history, run_chat_send, run_code_review, run_interview_draft, run_interview_respond,
    run_loop, run_pr_review_fix, run_refresh_agents, run_refresh_docs, run_security_code_review,
    run_single_issue, run_tracker_matrix, run_workflow_draft,
};
use agent::tracker::{
    DEFAULT_REVIEW_BOT_LOGIN, PendingIssue, PrSummary, TrackerInfo, current_branch_pr,
    enable_auto_merge, fetch_unresolved_thread_counts, find_tracker, get_tracker_body,
    is_auto_merge_enabled, list_open_prs, open_pr_map_from, parse_pending,
};
use agent::types::{
    AgentEvent, BotAuthMode, ChangedFile, ClaudeEvent, ContentBlock, EVENT_SENDER, FileChangeKind,
    InterviewTurn, Workflow, save_dev_config,
};
use agent::workflow::{list_presets, load_sidebar_entries, load_workflows};
use clap::{Parser, Subcommand};
use custom_themes::Theme;
use dioxus::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::info;
use ui::components::BASE_CSS;
use ui::security::{SecurityFinding, run_security_scan};
use ui::{Editor, Sidebar, Statusbar};

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct WorkflowPresetsResponse {
    presets: Vec<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct WorkflowEntriesResponse {
    workflows: Vec<crate::agent::workflow::WorkflowEntry>,
}

#[derive(Parser)]
#[command(
    name = "freq-ai",
    about = "Distributed application runtime agent",
    long_about = "freq-ai runs agent-powered project workflows from the command line or launches the desktop UI when no subcommand is given.",
    after_help = "Examples:\n  freq-ai\n  freq-ai --agent codex code-review\n  freq-ai --dry-run refresh-docs\n  freq-ai --preset software-factory run backlog-curation\n  freq-ai tracker-matrix 51 --json\n  freq-ai models\n  freq-ai --agent codex models --plain\n  freq-ai serve --port 3000",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Agent CLI adapter to use when running workflows
    #[arg(long, default_value = "claude")]
    agent: agent::types::Agent,

    /// Pass adapter-specific flags that reduce permission prompts
    #[arg(long)]
    auto: bool,

    /// Print planned prompts and actions without making supported changes
    #[arg(long)]
    dry_run: bool,

    /// Workflow preset to use (overrides `workflow_preset` in freq-ai.toml).
    /// See `freq-ai presets` for the list of available presets.
    #[arg(long, value_name = "NAME")]
    preset: Option<String>,

    /// Agent model for this invocation (overrides persisted freq-ai dev config).
    /// `FREQ_AI_MODEL` applies the same override when set.
    /// Hint: `freq-ai --agent … models` lists bundled IDs from `assets/available-models.json`.
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,

    /// Write the bundled label taxonomy to .github/labels.yml and exit
    #[arg(long)]
    create_labels: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the GUI (default)
    Gui,
    /// Address review threads on a PR
    FixPr {
        /// Pull request number to inspect and update
        #[arg(value_name = "PR")]
        pr: u32,
    },
    /// Run ideation draft
    Ideation,
    /// Run UXR synthesis draft
    UxrSynth,
    /// Run strategic review draft
    StrategicReview,
    /// Run roadmapper draft
    Roadmapper,
    /// Run sprint planning draft
    SprintPlanning,
    /// Run retrospective draft
    Retrospective,
    /// Run housekeeping draft
    Housekeeping,
    /// Run user interview
    Interview,
    /// Run code review
    CodeReview,
    /// Run security code review
    SecurityReview,
    /// Refresh agent files
    RefreshAgents,
    /// Refresh project documentation
    RefreshDocs,
    /// Run a single issue
    Issue {
        /// Tracker issue whose body lists this work item (for blocker → branch chaining)
        #[arg(long, value_name = "TRACKER_ISSUE")]
        tracker: Option<u32>,
        /// Issue number to run
        #[arg(value_name = "NUMBER")]
        number: u32,
    },
    /// Run the main loop for a tracker
    Loop {
        /// Tracker issue number to process
        #[arg(value_name = "TRACKER")]
        tracker: u32,
    },
    /// Print pending issue numbers for a tracker (for CI matrix generation)
    TrackerMatrix {
        /// Tracker issue number to read
        #[arg(value_name = "TRACKER")]
        tracker: u32,
        /// Emit a JSON array of issue numbers on stdout
        #[arg(long)]
        json: bool,
    },
    /// Serve the web UI via a local HTTP server
    Serve {
        /// Port for the local HTTP server
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    /// List available workflow presets, or the workflows inside one preset
    Presets {
        /// If given, list the workflows inside this preset instead of all presets
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Run any workflow from the active preset by ID (accepts hyphen or underscore form)
    Run {
        /// Workflow ID, e.g. `backlog-curation` or `backlog_curation`.
        /// See `freq-ai presets <NAME>` for available IDs.
        #[arg(value_name = "WORKFLOW")]
        workflow: String,
    },
    /// List bundled model IDs and labels for `--model` / `FREQ_AI_MODEL`
    ///
    /// Data comes from assets/available-models.json (regenerate: cargo build -p freq-ai-agent-runtime).
    Models {
        /// Print only model IDs, one per line (for shell completion).
        #[arg(long)]
        plain: bool,
        /// List every adapter; with `--plain`, deduplicate IDs across adapters.
        #[arg(long)]
        all: bool,
    },
}

/// Standalone entry point — equivalent to `run_with_overrides(|_| {})`.
/// Used by the `freq-ai` binary.
pub fn run() {
    run_with_overrides(|_| {});
}

/// Library entry point for consumers that want to inject custom `Config`
/// fields (e.g. a project-specific skill layout) before the agent runs.
///
/// The closure receives a mutable `Config` populated from CLI args, env vars,
/// and `freq-ai.toml`. Mutate it however you need; freq-ai then dispatches to
/// either the GUI or the requested CLI subcommand.
pub fn run_with_overrides<F>(overrides: F)
where
    F: FnOnce(&mut Config),
{
    #[cfg(target_arch = "wasm32")]
    {
        let mut config = parse_args();
        overrides(&mut config);
        CONFIG_OVERRIDE
            .set(config)
            .expect("failed to set CONFIG_OVERRIDE in wasm32");
        dioxus::launch(App);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        tracing_subscriber::fmt().with_env_filter(env_filter).init();

        let cli = Cli::parse();

        if cli.create_labels {
            let config = parse_args();
            let content =
                agent::assets::LABELS_YML.replace("{{project_name}}", &config.project_name);
            let dir = std::path::Path::new(".github");
            let _ = std::fs::create_dir_all(dir);
            let dest = dir.join("labels.yml");
            std::fs::write(&dest, content).expect("failed to write .github/labels.yml");
            println!("wrote {}", dest.display());
            return;
        }

        let mut config = parse_args();
        config.agent = cli.agent;
        // Load persisted model for the selected agent.
        let dev_cfg = agent::types::load_dev_config(&config.root);
        config.model = dev_cfg
            .agent_models
            .get(&config.agent.to_string())
            .cloned()
            .unwrap_or_default();
        config.auto_mode = cli.auto;
        config.dry_run = cli.dry_run;
        overrides(&mut config);

        apply_freq_ai_model_env_and_cli(&mut config, cli.model.as_deref());

        // CLI `--preset` wins over freq-ai.toml and library overrides — fail fast
        // with the available list if the name doesn't match a real preset dir.
        if let Some(preset) = &cli.preset {
            let available = list_presets(&config.root);
            if !available.iter().any(|p| p == preset) {
                eprintln!(
                    "unknown preset: {preset}\navailable presets: {}",
                    available.join(", ")
                );
                std::process::exit(2);
            }
            config.workflow_preset = preset.clone();
        }

        // Eagerly populate the issue-comment trigger cache from the (possibly
        // overridden) skill path so the sidebar reminder is non-empty before the
        // first render. Done after `overrides` so consumers' custom paths win.
        ui::sidebar::init_issue_comment_triggers(std::path::Path::new(
            &config.skill_paths.issue_tracking,
        ));

        match cli.command {
            Some(Commands::FixPr { pr }) => {
                run_pr_review_fix(&config, pr);
            }
            Some(Commands::Ideation) => run_workflow_draft(&config, "ideation"),
            Some(Commands::UxrSynth) => run_workflow_draft(&config, "report_research"),
            Some(Commands::StrategicReview) => run_workflow_draft(&config, "strategic_review"),
            Some(Commands::Roadmapper) => run_workflow_draft(&config, "roadmapper"),
            Some(Commands::SprintPlanning) => run_workflow_draft(&config, "sprint_planning"),
            Some(Commands::Retrospective) => run_workflow_draft(&config, "retrospective"),
            Some(Commands::Housekeeping) => run_workflow_draft(&config, "housekeeping"),
            Some(Commands::Interview) => run_interview_draft(&config),
            Some(Commands::CodeReview) => run_code_review(&config),
            Some(Commands::SecurityReview) => run_security_code_review(&config),
            Some(Commands::RefreshAgents) => run_refresh_agents(&config),
            Some(Commands::RefreshDocs) => run_refresh_docs(&config),
            Some(Commands::Issue { number, tracker }) => {
                let tracker_num = tracker
                    .or_else(|| find_tracker().first().map(|t| t.number))
                    .unwrap_or(0);
                let blockers = if tracker_num != 0 {
                    let body = get_tracker_body(tracker_num);
                    parse_pending(&body)
                        .into_iter()
                        .find(|p| p.number == number)
                        .map(|p| p.blockers)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                run_single_issue(&config, tracker_num, number, &blockers);
            }
            Some(Commands::Loop { tracker }) => run_loop(&config, tracker),
            Some(Commands::TrackerMatrix { tracker, json }) => {
                run_tracker_matrix(&config, tracker, json);
            }
            Some(Commands::Run { workflow }) => {
                let workflows = load_workflows(&config.root, &config.workflow_preset);
                let normalized = workflow.replace('-', "_");
                let resolved = workflows
                    .get(workflow.as_str())
                    .or_else(|| workflows.get(normalized.as_str()));
                match resolved {
                    Some(wf) => {
                        let id = wf.id.clone();
                        run_workflow_draft(&config, &id);
                    }
                    None => {
                        let mut ids: Vec<&str> =
                            workflows.values().map(|w| w.id.as_str()).collect();
                        ids.sort();
                        eprintln!(
                            "unknown workflow: {workflow}\npreset: {}\navailable workflows: {}",
                            config.workflow_preset,
                            ids.join(", ")
                        );
                        std::process::exit(2);
                    }
                }
            }
            Some(Commands::Models { plain, all }) => {
                agent::models_catalog::run_models_list(cli.agent, plain, all);
            }
            Some(Commands::Presets { name }) => match name {
                None => {
                    let active = &config.workflow_preset;
                    for preset in list_presets(&config.root) {
                        if &preset == active {
                            println!("{preset} (active)");
                        } else {
                            println!("{preset}");
                        }
                    }
                }
                Some(preset) => {
                    let available = list_presets(&config.root);
                    if !available.iter().any(|p| p == &preset) {
                        eprintln!(
                            "unknown preset: {preset}\navailable presets: {}",
                            available.join(", ")
                        );
                        std::process::exit(2);
                    }
                    let workflows = load_workflows(&config.root, &preset);
                    if workflows.is_empty() {
                        eprintln!("preset {preset} has no workflows");
                        std::process::exit(1);
                    }
                    let mut entries: Vec<_> = workflows.values().collect();
                    entries.sort_by(|a, b| {
                        a.ui.category
                            .cmp(&b.ui.category)
                            .then_with(|| a.id.cmp(&b.id))
                    });
                    let id_w = entries.iter().map(|w| w.id.len()).max().unwrap_or(0);
                    let cat_w = entries
                        .iter()
                        .map(|w| w.ui.category.len())
                        .max()
                        .unwrap_or(0);
                    for wf in entries {
                        let hidden = if wf.ui.visible { "" } else { " (hidden)" };
                        let desc = if wf.description.is_empty() {
                            wf.name.as_str()
                        } else {
                            wf.description.as_str()
                        };
                        println!(
                            "{:id_w$}  {:cat_w$}  {desc}{hidden}",
                            wf.id,
                            wf.ui.category,
                            id_w = id_w,
                            cat_w = cat_w,
                        );
                    }
                }
            },
            Some(Commands::Serve { port }) => {
                info!(
                    "Launching API/web server for root={} with requested_port={}",
                    config.root, port
                );
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = ui::server::serve(config.root.clone(), port).await {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                });
            }
            Some(Commands::Gui) | None => {
                // Stash the finalised Config so the Dioxus App component can pick
                // it up via `parse_args` (which already loads from freq-ai.toml). The
                // App's own use of `parse_args()` would otherwise lose the
                // overrides — but since the overrides are also persisted via the
                // explicit init_issue_comment_triggers call above, the only place
                // overrides matter inside the GUI is the next `parse_args` call,
                // which already reads `freq-ai.toml`. Library consumers who need to
                // inject overrides that aren't expressible in `freq-ai.toml` should
                // use the CLI subcommands instead of the GUI.
                CONFIG_OVERRIDE
                    .set(config)
                    .expect("CONFIG_OVERRIDE set twice");
                dioxus::launch(App);
            }
        }
    }
}

/// Process-wide handoff for `run_with_overrides` → `App`. The Dioxus App
/// component reads from this on first render so library consumers' overrides
/// (e.g. custom `skill_paths`) survive into the GUI rather than being
/// re-derived from `freq-ai.toml` alone.
static CONFIG_OVERRIDE: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

fn apply_nonempty_model_trim(into: &mut String, candidate: Option<&str>) {
    if let Some(m) = candidate {
        let m = m.trim();
        if !m.is_empty() {
            *into = m.to_string();
        }
    }
}

fn apply_freq_ai_model_env_and_cli(config: &mut Config, cli_model: Option<&str>) {
    apply_nonempty_model_trim(
        &mut config.model,
        std::env::var("FREQ_AI_MODEL").ok().as_deref(),
    );
    apply_nonempty_model_trim(&mut config.model, cli_model);
}

fn ensure_default_workflow_preset_first(mut presets: Vec<String>) -> Vec<String> {
    if !presets.iter().any(|preset| preset == "default") {
        presets.push("default".to_string());
    }
    if let Some(default_pos) = presets.iter().position(|preset| preset == "default") {
        presets.remove(default_pos);
    }
    presets.insert(0, "default".to_string());
    presets.dedup();
    presets
}

#[component]
fn App() -> Element {
    let mut config = use_signal(|| {
        // Prefer the override stashed by `run_with_overrides` so any custom
        // `skill_paths` / `bootstrap_agent_files` survive into the GUI.
        CONFIG_OVERRIDE.get().cloned().unwrap_or_else(parse_args)
    });
    let mut tracker_ids = use_signal(Vec::<TrackerInfo>::new);
    let mut issues = use_signal(Vec::<PendingIssue>::new);
    let mut is_working = use_signal(|| false);
    let mut awaiting_feedback = use_signal(|| None::<Workflow>);
    let mut feedback_text = use_signal(String::new);
    let mut events = use_signal(Vec::<AgentEvent>::new);
    let mut changed_files = use_signal(Vec::<ChangedFile>::new);
    let mut pr_map_sig = use_signal(HashMap::<u32, u32>::new);
    let mut pull_requests = use_signal(Vec::<PrSummary>::new);
    let mut security_findings = use_signal(Vec::<SecurityFinding>::new);
    let mut interview_turns = use_signal(Vec::<InterviewTurn>::new);
    let mut interview_active = use_signal(|| false);
    let mut interview_done = use_signal(|| false);
    let mut interview_agent_buf = use_signal(String::new);
    let mut chat_turns = use_signal(Vec::<InterviewTurn>::new);
    let mut chat_active = use_signal(|| false);
    let mut chat_agent_buf = use_signal(String::new);
    let mut settings_status = use_signal(|| None::<String>);
    let root_sig = use_signal(|| config.read().root.clone());
    let persona_skill_path_sig = use_signal(|| config.read().skill_paths.user_personas.clone());
    let mut all_files = use_signal(Vec::<String>::new);

    #[cfg(not(target_arch = "wasm32"))]
    let _ = use_resource(move || async move {
        let r = root_sig.read().clone();
        let files = tokio::task::spawn_blocking(move || list_all_files(&r))
            .await
            .unwrap_or_default();
        all_files.set(files);
    });

    let mut auto_merge_enabled = use_signal(|| false);
    let expand_all = use_signal(|| false);
    let follow_mode = use_signal(|| true);
    let bottom_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut theme = use_signal(Theme::tokyo_night);
    let mut presets = use_signal(|| vec!["default".to_string()]);
    let mut workflow_entries = use_signal(Vec::<crate::agent::workflow::WorkflowEntry>::new);

    use_effect(move || {
        #[cfg(not(target_arch = "wasm32"))]
        {
            preflight(&config.read());
        }

        // Initialize channel if not already done
        if EVENT_SENDER.get().is_none() {
            let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
            let _ = EVENT_SENDER.set(tx);

            #[cfg(not(target_arch = "wasm32"))]
            {
                // Initialize auto-merge state from actual PR (desktop only)
                spawn(async move {
                    let enabled = tokio::task::spawn_blocking(|| {
                        current_branch_pr()
                            .map(|pr| is_auto_merge_enabled(pr.number))
                            .unwrap_or(false)
                    })
                    .await
                    .unwrap_or(false);
                    auto_merge_enabled.set(enabled);
                });
            }

            // Spawn task to listen for events, update UI, and auto-scroll
            spawn(async move {
                while let Some(ev) = rx.recv().await {
                    match &ev {
                        AgentEvent::Done => {
                            // Flush interview agent buffer if active.
                            if *interview_active.peek() {
                                let buf = interview_agent_buf.peek().clone();
                                if !buf.trim().is_empty() {
                                    interview_turns.write().push(InterviewTurn {
                                        is_agent: true,
                                        content: buf,
                                    });
                                }
                                interview_agent_buf.set(String::new());
                                interview_done.set(true);
                                interview_active.set(false);
                            }
                            // Flush chat agent buffer if active.
                            if *chat_active.peek() {
                                let buf = chat_agent_buf.peek().clone();
                                if !buf.trim().is_empty() {
                                    record_agent_response(&buf);
                                    chat_turns.write().push(InterviewTurn {
                                        is_agent: true,
                                        content: buf,
                                    });
                                }
                                chat_agent_buf.set(String::new());
                                chat_active.set(false);
                            }
                            is_working.set(false);
                            awaiting_feedback.set(None);
                            clear_stop_request();
                            continue;
                        }
                        AgentEvent::AwaitingFeedback(wf) => {
                            // Flush interview agent buffer as a dialog turn.
                            if *wf == Workflow::Interview {
                                let buf = interview_agent_buf.peek().clone();
                                if !buf.trim().is_empty() {
                                    interview_turns.write().push(InterviewTurn {
                                        is_agent: true,
                                        content: buf,
                                    });
                                }
                                interview_agent_buf.set(String::new());
                            }
                            // Flush chat agent buffer as a dialog turn.
                            if *wf == Workflow::Chat {
                                let buf = chat_agent_buf.peek().clone();
                                if !buf.trim().is_empty() {
                                    record_agent_response(&buf);
                                    chat_turns.write().push(InterviewTurn {
                                        is_agent: true,
                                        content: buf,
                                    });
                                }
                                chat_agent_buf.set(String::new());
                            }
                            is_working.set(false);
                            awaiting_feedback.set(Some(*wf));
                            feedback_text.set(String::new());
                            clear_stop_request();
                            continue;
                        }
                        AgentEvent::TrackerUpdate(pending) => {
                            issues.set(pending.clone());
                            continue;
                        }
                        _ => {}
                    }
                    // Accumulate agent text into the interview buffer.
                    if *interview_active.peek()
                        && let AgentEvent::Claude(ClaudeEvent::Assistant { ref message }) = ev
                    {
                        for block in &message.content {
                            if let ContentBlock::Text { text } = block {
                                let mut buf = interview_agent_buf.write();
                                if !buf.is_empty() {
                                    buf.push('\n');
                                }
                                buf.push_str(text);
                            }
                        }
                    }
                    // Accumulate agent text into the chat buffer.
                    if *chat_active.peek()
                        && let AgentEvent::Claude(ClaudeEvent::Assistant { ref message }) = ev
                    {
                        for block in &message.content {
                            if let ContentBlock::Text { text } = block {
                                let mut buf = chat_agent_buf.write();
                                if !buf.is_empty() {
                                    buf.push('\n');
                                }
                                buf.push_str(text);
                            }
                        }
                    }
                    // Extract file changes from tool use events
                    if let AgentEvent::Claude(ClaudeEvent::Assistant { ref message }) = ev {
                        for block in &message.content {
                            if let ContentBlock::ToolUse { name, input, .. } = block {
                                let (path, kind) = match name.as_str() {
                                    "Read" => (
                                        input.get("file_path").and_then(|v| v.as_str()),
                                        FileChangeKind::Read,
                                    ),
                                    "Write" => (
                                        input.get("file_path").and_then(|v| v.as_str()),
                                        FileChangeKind::Created,
                                    ),
                                    "Edit" => (
                                        input.get("file_path").and_then(|v| v.as_str()),
                                        FileChangeKind::Modified,
                                    ),
                                    _ => (None, FileChangeKind::Read),
                                };
                                if let Some(p) = path {
                                    let mut files = changed_files.write();
                                    // Update existing entry or add new
                                    if let Some(existing) = files.iter_mut().find(|f| f.path == p) {
                                        // Upgrade: Read -> Modified/Created
                                        if kind != FileChangeKind::Read {
                                            existing.kind = kind;
                                        }
                                    } else {
                                        files.push(ChangedFile {
                                            path: p.to_string(),
                                            kind,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    events.write().push(ev);
                    if *follow_mode.peek()
                        && let Some(el) = bottom_el.peek().as_ref()
                    {
                        let _ = el.scroll_to(ScrollBehavior::Instant).await;
                    }
                }
            });
        }
    });

    // Load presets and workflows on app start
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let mut config_signal = config;
            let mut preset_signal = presets;
            // Fetch presets from API
            spawn(async move {
                if let Ok(response) = gloo_net::http::Request::get("/api/workflows/presets")
                    .send()
                    .await
                {
                    if let Ok(text) = response.text().await {
                        if let Ok(mut json) = serde_json::from_str::<WorkflowPresetsResponse>(&text)
                        {
                            let mut values = std::mem::take(&mut json.presets);
                            let presets = if values.is_empty() {
                                vec!["default".to_string()]
                            } else {
                                ensure_default_workflow_preset_first(values)
                            };
                            let default_preset = presets
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "default".to_string());
                            {
                                let current = config_signal.read().workflow_preset.clone();
                                if !presets.iter().any(|p| p == &current) {
                                    config_signal.write().workflow_preset = default_preset.clone();
                                }
                            }
                            preset_signal.set(presets);
                        }
                    }
                }
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let cfg = config.read().clone();
            let preset_list = ensure_default_workflow_preset_first(list_presets(&cfg.root));
            let default_preset = preset_list
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            if !preset_list.iter().any(|p| p == &cfg.workflow_preset) {
                config.write().workflow_preset = default_preset;
            }
            presets.set(preset_list);
        }
    });

    // Fetch workflows from API in web mode, or from filesystem in desktop mode
    use_effect(move || {
        let preset = config.read().workflow_preset.clone();

        #[cfg(target_arch = "wasm32")]
        {
            spawn(async move {
                if let Ok(response) =
                    gloo_net::http::Request::get(&format!("/api/workflows/{}", preset))
                        .send()
                        .await
                {
                    if let Ok(text) = response.text().await {
                        if let Ok(json) = serde_json::from_str::<WorkflowEntriesResponse>(&text) {
                            workflow_entries.set(json.workflows);
                        }
                    }
                }
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let cfg = config.read().clone();
            let entries = load_sidebar_entries(&cfg.root, &preset);
            workflow_entries.set(entries);
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    let refresh_tracker = move |_: MouseEvent| {
        info!("Refreshing trackers...");
        let infos = find_tracker();
        tracker_ids.set(infos.clone());
        let mut all_pending = Vec::new();
        for info in &infos {
            let body = get_tracker_body(info.number);
            all_pending.extend(parse_pending(&body));
        }
        all_pending.sort_by_key(|i| i.number);
        all_pending.dedup_by_key(|i| i.number);
        let mut prs = list_open_prs();
        let counts = fetch_unresolved_thread_counts(DEFAULT_REVIEW_BOT_LOGIN);
        for pr in &mut prs {
            pr.unresolved_thread_count = counts.get(&pr.number).copied().unwrap_or(0);
        }
        let pr_map = open_pr_map_from(&prs);
        pr_map_sig.set(pr_map);
        pull_requests.set(prs);
        issues.set(all_pending);
    };

    #[cfg(target_arch = "wasm32")]
    let refresh_tracker = move |_: MouseEvent| {
        info!("Tracker refresh not available in web mode");
    };

    #[cfg(not(target_arch = "wasm32"))]
    let start_work = move |tracker_num: u32| {
        clear_stop_request();
        is_working.set(true);
        changed_files.write().clear();
        let cfg = config.read().clone();
        tokio::spawn(async move {
            run_loop(&cfg, tracker_num);
        });
    };

    #[cfg(target_arch = "wasm32")]
    let start_work = move |_tracker_num: u32| {
        info!("Tracker work not available in web mode");
    };

    #[cfg(not(target_arch = "wasm32"))]
    let start_single_issue = move |issue_num: u32| {
        clear_stop_request();
        is_working.set(true);
        changed_files.write().clear();
        let cfg = config.read().clone();
        tokio::spawn(async move {
            let trackers = find_tracker();
            let tracker_num = trackers.first().map(|t| t.number).unwrap_or(0);
            let blockers = if tracker_num != 0 {
                let body = get_tracker_body(tracker_num);
                parse_pending(&body)
                    .into_iter()
                    .find(|p| p.number == issue_num)
                    .map(|p| p.blockers)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            run_single_issue(&cfg, tracker_num, issue_num, &blockers);
        });
    };

    #[cfg(target_arch = "wasm32")]
    let start_single_issue = move |_issue_num: u32| {
        info!("Single issue work not available in web mode");
    };

    #[cfg(not(target_arch = "wasm32"))]
    let start_pr_fix = move |pr_num: u32| {
        clear_stop_request();
        is_working.set(true);
        changed_files.write().clear();
        let cfg = config.read().clone();
        tokio::spawn(async move {
            run_pr_review_fix(&cfg, pr_num);
        });
    };

    #[cfg(target_arch = "wasm32")]
    let start_pr_fix = move |_pr_num: u32| {
        info!("PR fix work not available in web mode");
    };

    #[cfg(not(target_arch = "wasm32"))]
    let on_preset_change = move |preset: String| {
        config.write().workflow_preset = preset.clone();
        workflow_entries.set(load_sidebar_entries(&config.read().root, &preset));
    };

    #[cfg(target_arch = "wasm32")]
    let on_preset_change = move |preset: String| {
        config.write().workflow_preset = preset.clone();
        // Re-fetch workflows from API with new preset
        let preset_clone = preset.clone();
        spawn(async move {
            if let Ok(response) =
                gloo_net::http::Request::get(&format!("/api/workflows/{}", preset_clone))
                    .send()
                    .await
            {
                if let Ok(text) = response.text().await {
                    if let Ok(json) = serde_json::from_str::<WorkflowEntriesResponse>(&text) {
                        workflow_entries.set(json.workflows);
                    }
                }
            }
        });
    };

    #[cfg(not(target_arch = "wasm32"))]
    let on_start_workflow = move |workflow_id: String| {
        clear_stop_request();
        let cfg = config.read().clone();

        // Chat mode: free-form conversation, no workflow commitment.
        if workflow_id == "chat" {
            // Reset if starting fresh (not continuing).
            if *awaiting_feedback.read() != Some(Workflow::Chat) {
                reset_chat_history();
                chat_turns.write().clear();
                chat_agent_buf.set(String::new());
            }
            chat_active.set(true);
            // Signal the UI to show the chat tab with the input ready.
            awaiting_feedback.set(Some(Workflow::Chat));
            feedback_text.set(String::new());
            return;
        }

        // Interview has special state management.
        if workflow_id == "interview" {
            is_working.set(true);
            interview_turns.write().clear();
            interview_agent_buf.set(String::new());
            interview_active.set(true);
            interview_done.set(false);
            tokio::spawn(async move {
                run_interview_draft(&cfg);
            });
            return;
        }

        // Security scan (local, non-agent).
        if workflow_id == "security_scan" {
            let root = cfg.root.clone();
            let targets = cfg.scan_targets.clone();
            info!("Running security review scan...");
            spawn(async move {
                let findings =
                    tokio::task::spawn_blocking(move || run_security_scan(&root, &targets))
                        .await
                        .unwrap_or_default();
                info!("Security scan complete: {} findings", findings.len());
                security_findings.set(findings);
            });
            return;
        }

        // Auto merge (special).
        if workflow_id == "auto_merge" {
            return;
        }

        is_working.set(true);

        // Look up a registered action runner, otherwise use the generic YAML runner.
        if let Some(action) = lookup_action(&workflow_id) {
            let action = *action;
            tokio::spawn(async move {
                let mut ctx = ActionContext::new(&workflow_id);
                if let Err(e) = action(&cfg, &mut ctx) {
                    agent::shell::log(&format!("Workflow '{}' failed: {e}", ctx.workflow_id));
                }
            });
        } else {
            tokio::spawn(async move { run_workflow_draft(&cfg, &workflow_id) });
        }
    };

    #[cfg(target_arch = "wasm32")]
    let on_start_workflow = move |workflow_id: String| {
        info!(
            "Workflow execution not available in web mode: {}",
            workflow_id
        );
    };

    #[cfg(not(target_arch = "wasm32"))]
    let save_settings = move |_: MouseEvent| {
        let cfg = config.read().clone();
        settings_status.set(Some("Saving configuration...".into()));
        spawn(async move {
            let root = cfg.root.clone();
            let result = tokio::task::spawn_blocking(move || {
                save_dev_config(&root, &cfg)?;

                match cfg.bot_settings.mode {
                    BotAuthMode::Token => {
                        let token = cfg.bot_settings.token.trim();
                        if token.is_empty() {
                            clear_bot_token(&root).map_err(|e| e.to_string())?;
                        } else {
                            store_bot_token(&root, token).map_err(|e| e.to_string())?;
                        }
                        clear_bot_private_key_pem(&root).map_err(|e| e.to_string())?;
                    }
                    BotAuthMode::GitHubApp => {
                        clear_bot_token(&root).map_err(|e| e.to_string())?;
                        let pem = cfg.bot_settings.private_key_pem.trim();
                        if pem.is_empty() {
                            clear_bot_private_key_pem(&root).map_err(|e| e.to_string())?;
                        } else {
                            store_bot_private_key_pem(&root, pem).map_err(|e| e.to_string())?;
                        }
                    }
                    BotAuthMode::Disabled => {
                        clear_bot_token(&root).map_err(|e| e.to_string())?;
                        clear_bot_private_key_pem(&root).map_err(|e| e.to_string())?;
                    }
                }

                let api_key = cfg.local_inference.api_key.trim();
                if api_key.is_empty() {
                    clear_local_inference_api_key(&root).map_err(|e| e.to_string())?;
                } else {
                    store_local_inference_api_key(&root, api_key).map_err(|e| e.to_string())?;
                }

                Ok::<(), String>(())
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);

            match result {
                Ok(()) => settings_status.set(Some(
                    "Configuration saved. Secrets use the OS credential vault.".into(),
                )),
                Err(err) => {
                    settings_status.set(Some(format!("Failed to save configuration: {err}")))
                }
            }
        });
    };

    #[cfg(target_arch = "wasm32")]
    let save_settings = move |_: MouseEvent| {
        info!("Configuration saving not available in web mode");
    };

    #[cfg(not(target_arch = "wasm32"))]
    let submit_feedback = move |_: MouseEvent| {
        let fb = feedback_text.read().clone();
        if fb.trim().is_empty() {
            return;
        }
        clear_stop_request();
        let wf = *awaiting_feedback.read();
        awaiting_feedback.set(None);
        is_working.set(true);

        // Record user answer as interview turn.
        if wf == Some(Workflow::Interview) {
            interview_turns.write().push(InterviewTurn {
                is_agent: false,
                content: fb.clone(),
            });
        }

        // Record user message as chat turn.
        if wf == Some(Workflow::Chat) {
            chat_turns.write().push(InterviewTurn {
                is_agent: false,
                content: fb.clone(),
            });
        }

        let cfg = config.read().clone();
        tokio::spawn(async move {
            match wf {
                Some(Workflow::Interview) => run_interview_respond(&cfg, &fb),
                Some(Workflow::Chat) => run_chat_send(&cfg, &fb),
                Some(w) => {
                    use agent::shell::run_workflow_finalize;
                    run_workflow_finalize(&cfg, w.to_id(), &fb);
                }
                None => {}
            }
        });
    };

    #[cfg(target_arch = "wasm32")]
    let submit_feedback = move |_: MouseEvent| {
        info!("Feedback submission not available in web mode");
    };

    let stop_work = move |_: MouseEvent| {
        request_stop();
        is_working.set(false);
    };

    #[cfg(not(target_arch = "wasm32"))]
    let on_auto_merge = move |_: MouseEvent| {
        auto_merge_enabled.set(true); // Optimistic guard against double-click
        spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                if let Some(pr) = current_branch_pr() {
                    if is_auto_merge_enabled(pr.number) {
                        return true;
                    }
                    return enable_auto_merge(pr.number);
                }
                false
            })
            .await;
            match result {
                Ok(true) => auto_merge_enabled.set(true),
                Ok(false) => {
                    info!("Failed to enable auto-merge");
                    auto_merge_enabled.set(false);
                }
                Err(e) => {
                    info!("Auto-merge task failed: {e}");
                    auto_merge_enabled.set(false);
                }
            }
        });
    };

    #[cfg(target_arch = "wasm32")]
    let on_auto_merge = move |_: MouseEvent| {
        info!("Auto-merge not available in web mode");
    };

    let css = format!("{vars}\n{BASE_CSS}", vars = theme.read().to_css_vars());

    rsx! {
        style { "{css}" }

        div { class: "ide",
            // ── Title bar ──
            div { class: "titlebar",
                div { class: "titlebar-left",
                    span { class: "titlebar-icon", ">" }
                    span { class: "titlebar-name", "{config.read().project_name} Dev Agent" }
                }
                div { class: "titlebar-right",
                    select {
                        class: "titlebar-select",
                        value: "{theme.read().name}",
                        onchange: move |evt| {
                            if let Some(t) = Theme::by_name(&evt.value()) {
                                theme.set(t);
                            }
                        },
                        for t in Theme::all() {
                            option { value: "{t.name}", "{t.name}" }
                        }
                    }
                }
            }

            // ── Main body: sidebar + editor ──
            div { class: "ide-body",
                Sidebar {
                    config,
                    tracker_ids,
                    issues,
                    pull_requests,
                    pr_map: pr_map_sig,
                    is_working,
                    awaiting_feedback,
                    feedback_text,
                    auto_merge_enabled,
                    settings_status,
                    refresh_tracker,
                    start_work,
                    start_single_issue,
                    start_pr_fix,
                    workflow_entries,
                    presets,
                    on_preset_change,
                    on_start_workflow,
                    save_settings,
                    stop_work,
                    submit_feedback,
                    on_auto_merge,
                }

                Editor {
                    events,
                    changed_files,
                    all_files,
                    security_findings,
                    interview_turns,
                    interview_active,
                    interview_done,
                    chat_turns,
                    chat_active,
                    awaiting_feedback,
                    is_working,
                    feedback_text,
                    submit_feedback,
                    root: root_sig,
                    persona_skill_path: persona_skill_path_sig,
                    follow_mode,
                    expand_all,
                    usage_model: config.read().pricing_model_key(),
                    pricing: config.read().pricing.clone(),
                    bottom_el,
                }
            }

            // ── Status bar ──
            Statusbar {
                config,
                tracker_ids,
                issues,
                events,
                is_working,
                theme_name: theme.read().name.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod model_trim_tests {
    use super::apply_nonempty_model_trim;

    #[test]
    fn successive_nonempty_candidates_override() {
        let mut m = "first".into();
        apply_nonempty_model_trim(&mut m, Some("  second  "));
        apply_nonempty_model_trim(&mut m, Some("third"));
        assert_eq!(m, "third");
    }

    #[test]
    fn skips_empty_or_whitespace_only() {
        let mut m = "keep".into();
        apply_nonempty_model_trim(&mut m, Some("   "));
        apply_nonempty_model_trim(&mut m, Some(""));
        apply_nonempty_model_trim(&mut m, None);
        assert_eq!(m, "keep");
    }
}
