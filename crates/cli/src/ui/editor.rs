use crate::agent::types::{
    AgentEvent, ChangedFile, Config, ContentBlock, FileChangeKind, InterviewTurn, PricingConfig,
    RichAction, Workflow, should_use_event_model,
};
use crate::ui::components::EventRow;
use crate::ui::discovery::{DiscoveryPanel, DiscoveryWorkspace};
use crate::ui::personas::PersonasPanel;
use crate::ui::security::{SecurityFinding, SecurityPanel};
use dioxus::document::eval as js_eval;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Build a map of tool_use_id -> tool_name from all events so far.
fn build_tool_names(events: &[AgentEvent]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for ev in events {
        if let AgentEvent::Rich(RichAction::Assistant { message }) = ev {
            for block in &message.content {
                if let ContentBlock::ToolUse { id, name, .. } = block {
                    map.insert(id.clone(), name.clone());
                }
            }
        }
    }
    map
}

#[derive(Clone, Copy, PartialEq)]
enum EditorTab {
    Output,
    Files,
    Discovery,
    Personas,
    Security,
    Interview,
    Chat,
}

#[derive(Clone, Copy, PartialEq)]
enum FileViewMode {
    Interacted,
    Browser,
}

fn find_content_for_path(events: &[AgentEvent], path: &str) -> Option<String> {
    for ev in events.iter().rev() {
        match ev {
            AgentEvent::Rich(RichAction::Assistant { message }) => {
                for block in &message.content {
                    if let ContentBlock::ToolUse { name, input, .. } = block
                        && (name == "Write" || name == "Edit")
                        && input.get("file_path").and_then(|v| v.as_str()) == Some(path)
                        && let Some(content) = input.get("content").and_then(|v| v.as_str())
                    {
                        return Some(content.to_string());
                    }
                }
            }
            AgentEvent::Rich(RichAction::User { message }) => {
                for block in &message.content {
                    if let ContentBlock::ToolResult { id, content } = block {
                        for ev_inner in events {
                            if let AgentEvent::Rich(RichAction::Assistant { message: msg_inner }) =
                                ev_inner
                            {
                                for block_inner in &msg_inner.content {
                                    if let ContentBlock::ToolUse {
                                        id: id_inner,
                                        name: name_inner,
                                        input: input_inner,
                                    } = block_inner
                                        && id_inner == id
                                        && name_inner == "Read"
                                        && input_inner.get("file_path").and_then(|v| v.as_str())
                                            == Some(path)
                                    {
                                        return Some(content.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

#[component]
pub fn Editor(
    events: Signal<Vec<AgentEvent>>,
    changed_files: Signal<Vec<ChangedFile>>,
    all_files: Signal<Vec<String>>,
    security_findings: Signal<Vec<SecurityFinding>>,
    interview_turns: Signal<Vec<InterviewTurn>>,
    interview_active: Signal<bool>,
    interview_done: Signal<bool>,
    chat_turns: Signal<Vec<InterviewTurn>>,
    chat_active: Signal<bool>,
    awaiting_feedback: Signal<Option<Workflow>>,
    is_working: Signal<bool>,
    feedback_text: Signal<String>,
    discovery_workspace: Signal<DiscoveryWorkspace>,
    submit_feedback: EventHandler<MouseEvent>,
    root: Signal<String>,
    config: Signal<Config>,
    persona_skill_path: Signal<String>,
    follow_mode: Signal<bool>,
    expand_all: Signal<bool>,
    usage_model: Option<String>,
    pricing: PricingConfig,
    bottom_el: Signal<Option<std::rc::Rc<MountedData>>>,
) -> Element {
    let mut active_tab = use_signal(|| EditorTab::Output);
    let mut selected_file = use_signal(|| None::<String>);
    let mut file_view_mode = use_signal(|| FileViewMode::Interacted);
    let mut file_search = use_signal(String::new);
    let output_events = {
        let mut active_usage_model = usage_model.clone();
        let has_configured_model = active_usage_model.is_some();
        events
            .read()
            .iter()
            .enumerate()
            .map(|(i, event)| {
                if let AgentEvent::Rich(RichAction::System {
                    model: Some(model), ..
                }) = event
                    && should_use_event_model(model, has_configured_model)
                {
                    active_usage_model = Some(model.trim().to_string());
                }

                (i, event.clone(), active_usage_model.clone())
            })
            .collect::<Vec<_>>()
    };
    let output_empty = output_events.is_empty();

    use_effect(move || {
        if *active_tab.read() == EditorTab::Files {
            js_eval(
                r#"
                require.config({ paths: { vs: 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.44.0/min/vs' } });
                require(['vs/editor/editor.main'], function () {
                    if (!window.monacoEditor) {
                        window.monacoEditor = monaco.editor.create(document.getElementById('monaco-container'), {
                            value: '',
                            language: 'rust',
                            theme: 'vs-dark',
                            automaticLayout: true,
                            readOnly: true,
                            minimap: { enabled: false },
                        });
                    }
                });
            "#,
            );
        }
    });

    let mut on_file_click = move |path: String| {
        selected_file.set(Some(path.clone()));
        let content = find_content_for_path(&events.read(), &path).unwrap_or_else(|| {
            let full = std::path::Path::new(&*root.read()).join(&path);
            std::fs::read_to_string(full).unwrap_or_default()
        });

        let lang = match std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
        {
            Some("rs") => "rust",
            Some("md") => "markdown",
            Some("toml") => "toml",
            Some("json") => "json",
            Some("yaml") | Some("yml") => "yaml",
            Some("js") => "javascript",
            Some("ts") => "typescript",
            Some("html") => "html",
            Some("css") => "css",
            _ => "text",
        };

        js_eval(&format!(
            "if (window.monacoEditor) {{
                window.monacoEditor.setValue({:?});
                monaco.editor.setModelLanguage(window.monacoEditor.getModel(), {:?});
            }}",
            content, lang
        ));
    };

    let tool_names = build_tool_names(&events.read());

    let files = changed_files.read();
    let created_count = files
        .iter()
        .filter(|f| f.kind == FileChangeKind::Created)
        .count();
    let modified_count = files
        .iter()
        .filter(|f| f.kind == FileChangeKind::Modified)
        .count();
    let read_count = files
        .iter()
        .filter(|f| f.kind == FileChangeKind::Read)
        .count();
    let file_count = files.len();
    drop(files);

    rsx! {
        div { class: "editor",
            // Tab bar
            div { class: "tab-bar",
                div {
                    class: if *active_tab.read() == EditorTab::Output { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set(EditorTab::Output),
                    "Agent Output"
                }
                div {
                    class: if *active_tab.read() == EditorTab::Files { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set(EditorTab::Files),
                    "Files ({file_count})"
                }
                div {
                    class: if *active_tab.read() == EditorTab::Discovery { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set(EditorTab::Discovery),
                    "Discovery"
                }
                div {
                    class: if *active_tab.read() == EditorTab::Personas { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set(EditorTab::Personas),
                    "Personas"
                }
                div {
                    class: if *active_tab.read() == EditorTab::Security { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set(EditorTab::Security),
                    "Security ({security_findings.read().len()})"
                }
                if *interview_active.read() || !interview_turns.read().is_empty() {
                    div {
                        class: if *active_tab.read() == EditorTab::Interview { "tab tab-active" } else { "tab" },
                        onclick: move |_| active_tab.set(EditorTab::Interview),
                        "Interview ({interview_turns.read().len()})"
                    }
                }
                {
                    let chat_count = chat_turns.read().len();
                    let chat_label = if chat_count == 0 { "Chat".to_string() } else { format!("Chat ({chat_count})") };
                    rsx! {
                        div {
                            class: if *active_tab.read() == EditorTab::Chat { "tab tab-active" } else { "tab" },
                            onclick: move |_| active_tab.set(EditorTab::Chat),
                            "{chat_label}"
                        }
                    }
                }
                div { class: "tab-actions",
                    if *active_tab.read() == EditorTab::Output {
                        label { class: "tab-check",
                            input {
                                r#type: "checkbox",
                                checked: *follow_mode.read(),
                                onchange: move |evt| follow_mode.set(evt.value().parse::<bool>().unwrap_or(false)),
                            }
                            span { "Follow" }
                        }
                        label { class: "tab-check",
                            input {
                                r#type: "checkbox",
                                checked: *expand_all.read(),
                                onchange: move |evt| expand_all.set(evt.value().parse::<bool>().unwrap_or(false)),
                            }
                            span { "Expand All" }
                        }
                    }
                }
            }

            match *active_tab.read() {
                EditorTab::Output => rsx! {
                    div { class: "editor-content",
                        for (i , event, event_usage_model) in output_events {
                            EventRow {
                                key: "{i}",
                                event,
                                expand_all: *expand_all.read(),
                                tool_names: tool_names.clone(),
                                usage_model: event_usage_model,
                                pricing: pricing.clone(),
                            }
                        }
                        if output_empty {
                            div { class: "text-muted editor-empty", "Waiting for activity..." }
                        }
                        div {
                            onmounted: move |cx| bottom_el.set(Some(cx.data())),
                        }
                    }
                },
                EditorTab::Files => rsx! {
                    div { class: "editor-content editor-layout", style: "padding: 0;",
                        div { class: "file-list",
                            div { class: "file-list-header",
                                div { class: "file-list-tabs",
                                    div {
                                        class: if *file_view_mode.read() == FileViewMode::Interacted { "file-list-tab file-list-tab-active" } else { "file-list-tab" },
                                        onclick: move |_| file_view_mode.set(FileViewMode::Interacted),
                                        "Interacted"
                                    }
                                    div {
                                        class: if *file_view_mode.read() == FileViewMode::Browser { "file-list-tab file-list-tab-active" } else { "file-list-tab" },
                                        onclick: move |_| file_view_mode.set(FileViewMode::Browser),
                                        "Browser"
                                    }
                                }
                                if *file_view_mode.read() == FileViewMode::Browser {
                                    div { class: "file-search",
                                        input {
                                            r#type: "text",
                                            placeholder: "Search files...",
                                            value: "{file_search}",
                                            oninput: move |evt| file_search.set(evt.value()),
                                        }
                                    }
                                }
                            }

                            div { class: "file-list-container",
                                match *file_view_mode.read() {
                                    FileViewMode::Interacted => rsx! {
                                        if file_count == 0 {
                                            div { class: "text-muted editor-empty", "No file activity yet..." }
                                        } else {
                                            div { class: "files-summary", style: "padding: 8px; border-bottom: 1px solid var(--border); margin: 0; gap: 8px; flex-wrap: wrap;",
                                                if created_count > 0 {
                                                    span { class: "file-stat file-stat-created", "+{created_count}" }
                                                }
                                                if modified_count > 0 {
                                                    span { class: "file-stat file-stat-modified", "~{modified_count}" }
                                                }
                                                if read_count > 0 {
                                                    span { class: "file-stat file-stat-read", "{read_count}r" }
                                                }
                                            }
                                            ul { style: "list-style: none; padding: 0; margin: 0;",
                                                for (i , file) in changed_files.read().iter().enumerate() {
                                                    li {
                                                        key: "{i}",
                                                        class: if selected_file.read().as_ref() == Some(&file.path) { "file-entry file-entry-active" } else { "file-entry" },
                                                        onclick: {
                                                            let path = file.path.clone();
                                                            move |_| on_file_click(path.clone())
                                                        },
                                                        span {
                                                            class: match file.kind {
                                                                FileChangeKind::Created => "file-kind file-kind-created",
                                                                FileChangeKind::Modified => "file-kind file-kind-modified",
                                                                FileChangeKind::Deleted => "file-kind file-kind-deleted",
                                                                FileChangeKind::Read => "file-kind file-kind-read",
                                                            },
                                                            match file.kind {
                                                                FileChangeKind::Created => "+",
                                                                FileChangeKind::Modified => "~",
                                                                FileChangeKind::Deleted => "-",
                                                                FileChangeKind::Read => "r",
                                                            }
                                                        }
                                                        span { class: "file-path", "{file.path}" }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    FileViewMode::Browser => rsx! {
                                        ul { style: "list-style: none; padding: 0; margin: 0;",
                                            for path in all_files.read().iter().filter(|p| p.to_lowercase().contains(&file_search.read().to_lowercase())) {
                                                li {
                                                    key: "{path}",
                                                    class: if selected_file.read().as_ref() == Some(path) { "file-entry file-entry-active" } else { "file-entry" },
                                                    onclick: {
                                                        let path = path.clone();
                                                        move |_| on_file_click(path.clone())
                                                    },
                                                    span { class: "file-path", style: "margin-left: 4px;", "{path}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { id: "monaco-container" }
                    }
                },
                EditorTab::Discovery => rsx! {
                    DiscoveryPanel {
                        root,
                        workspace: discovery_workspace,
                        config,
                    }
                },
                EditorTab::Security => rsx! {
                    SecurityPanel {
                        findings: security_findings,
                        root,
                    }
                },
                EditorTab::Personas => rsx! {
                    PersonasPanel {
                        root,
                        skill_path: persona_skill_path,
                    }
                },
                EditorTab::Interview => rsx! {
                    div { class: "interview-panel",
                        if interview_turns.read().is_empty() {
                            div { class: "interview-empty", "Interview not started yet." }
                        }
                        for (i , turn) in interview_turns.read().iter().enumerate() {
                            if turn.is_agent {
                                div { key: "{i}", class: "interview-turn interview-turn-agent",
                                    div { class: "interview-role interview-role-agent", "Agent" }
                                    div { class: "interview-bubble interview-bubble-agent",
                                        "{turn.content}"
                                    }
                                }
                            } else {
                                div { key: "{i}", class: "interview-turn interview-turn-user",
                                    div { class: "interview-role interview-role-user", "You" }
                                    div { class: "interview-bubble interview-bubble-user",
                                        "{turn.content}"
                                    }
                                }
                            }
                        }
                        if *interview_done.read() && !interview_turns.read().is_empty() {
                            div { class: "interview-summary-card",
                                div { class: "interview-summary-title", "Interview Complete" }
                                div { class: "interview-summary-body",
                                    "The structured summary has been generated above. You can use it as input for other workflows (Sprint Planning, Roadmapper, etc.)."
                                }
                            }
                        }
                        if *interview_active.read() && !*interview_done.read() {
                            div { class: "interview-status",
                                "Awaiting your response in the Feedback section..."
                            }
                        }
                    }
                },
                EditorTab::Chat => rsx! {
                    div { class: "chat-panel",
                        div { class: "chat-messages",
                            if chat_turns.read().is_empty() && !*is_working.read() {
                                div { class: "chat-empty",
                                    "Start a conversation with the agent. Ask questions, discuss ideas, or get help with anything — no workflow required."
                                }
                            }
                            for (i , turn) in chat_turns.read().iter().enumerate() {
                                if turn.is_agent {
                                    div { key: "{i}", class: "interview-turn interview-turn-agent",
                                        div { class: "interview-role interview-role-agent", "Agent" }
                                        div { class: "interview-bubble interview-bubble-agent",
                                            "{turn.content}"
                                        }
                                    }
                                } else {
                                    div { key: "{i}", class: "interview-turn interview-turn-user",
                                        div { class: "interview-role interview-role-user", "You" }
                                        div { class: "interview-bubble interview-bubble-user",
                                            "{turn.content}"
                                        }
                                    }
                                }
                            }
                            if *is_working.read() && *chat_active.read() {
                                div { class: "chat-typing", "Agent is thinking..." }
                            }
                        }
                        div { class: "chat-input-area",
                            textarea {
                                class: "chat-input",
                                placeholder: "Type a message...",
                                disabled: *is_working.read(),
                                value: "{feedback_text.read()}",
                                oninput: move |evt| feedback_text.set(evt.value()),
                            }
                            button {
                                class: "btn btn-sm btn-go chat-send-btn",
                                disabled: feedback_text.read().trim().is_empty() || *is_working.read(),
                                onclick: move |evt| {
                                    // Ensure chat state is active so submit_feedback
                                    // dispatches to run_chat_send. This makes the Chat
                                    // tab self-contained — no sidebar click required.
                                    if *awaiting_feedback.read() != Some(Workflow::Chat) {
                                        awaiting_feedback.set(Some(Workflow::Chat));
                                        chat_active.set(true);
                                    }
                                    submit_feedback.call(evt);
                                },
                                "Send"
                            }
                        }
                    }
                },
            }
        }
    }
}
