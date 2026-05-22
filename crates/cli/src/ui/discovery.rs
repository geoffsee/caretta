use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssumptionRecord {
    pub status: String,
    pub confidence: String,
    pub evidence: String,
    pub owner: String,
    pub validation_next_step: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameComparisonRow {
    pub frame: String,
    pub framing: String,
    pub evidence: String,
    pub tradeoffs: String,
    pub recommendation: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionRecord {
    pub gate: String,
    pub rationale: String,
    pub rejected_alternatives: String,
    pub reversibility: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskRow {
    pub likelihood: String,
    pub impact: String,
    pub trigger: String,
    pub mitigation: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstraintLink {
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryWorkspace {
    pub problem: String,
    pub stakeholders: String,
    pub evidence: String,
    pub desired_outcome: String,
    pub constraints: String,
    pub dependencies: String,
    pub existing_systems: String,
    pub hypotheses: String,
    pub success_metrics: String,
    pub tradeoffs: String,
    pub risks: String,
    pub assumption_summary: String,
    pub frame_notes: String,
    pub decision: String,
    pub decision_rationale: String,
    pub risk_accepted: bool,
    pub risk_notes: String,
    pub assumptions: Vec<AssumptionRecord>,
    pub frame_comparisons: Vec<FrameComparisonRow>,
    pub decision_log: Vec<DecisionRecord>,
    pub risk_dashboard: Vec<RiskRow>,
    pub dependency_graph: Vec<ConstraintLink>,
}

impl DiscoveryWorkspace {
    pub fn is_empty(&self) -> bool {
        self.problem.trim().is_empty()
            && self.stakeholders.trim().is_empty()
            && self.evidence.trim().is_empty()
            && self.desired_outcome.trim().is_empty()
            && self.constraints.trim().is_empty()
            && self.dependencies.trim().is_empty()
            && self.existing_systems.trim().is_empty()
            && self.hypotheses.trim().is_empty()
            && self.success_metrics.trim().is_empty()
            && self.tradeoffs.trim().is_empty()
            && self.risks.trim().is_empty()
            && self.assumption_summary.trim().is_empty()
            && self.frame_notes.trim().is_empty()
            && self.decision.trim().is_empty()
            && self.decision_rationale.trim().is_empty()
            && self.risk_notes.trim().is_empty()
            && !self.risk_accepted
            && self.assumptions.is_empty()
            && self.frame_comparisons.is_empty()
            && self.decision_log.is_empty()
            && self.risk_dashboard.is_empty()
            && self.dependency_graph.is_empty()
    }

    fn format_assumption_rows(&self) -> String {
        if self.assumptions.is_empty() {
            return "- No assumptions recorded.".to_string();
        }

        self.assumptions
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                format!(
                    "{idx}. status={}; confidence={}; evidence={}; owner={}; validation_next_step={};",
                    item.status, item.confidence, item.evidence, item.owner, item.validation_next_step
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_frame_rows(&self) -> String {
        if self.frame_comparisons.is_empty() {
            return "- No frame entries yet.".to_string();
        }

        let mut lines = Vec::new();
        for item in &self.frame_comparisons {
            lines.push(format!(
                "- {} | {} | {} | {} | {}",
                item.frame, item.framing, item.evidence, item.tradeoffs, item.recommendation
            ));
        }
        lines.join("\n")
    }

    fn format_decision_rows(&self) -> String {
        if self.decision_log.is_empty() {
            return "- No decision log yet.".to_string();
        }

        self.decision_log
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                format!(
                    "{idx}. gate={} | rationale={} | rejected_alternatives={} | reversibility={}",
                    item.gate, item.rationale, item.rejected_alternatives, item.reversibility
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_risk_rows(&self) -> String {
        if self.risk_dashboard.is_empty() {
            return "- No risks captured.".to_string();
        }

        self.risk_dashboard
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                format!(
                    "{idx}. likelihood={} | impact={} | trigger={} | mitigation={}",
                    item.likelihood, item.impact, item.trigger, item.mitigation
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_constraint_rows(&self) -> String {
        if self.dependency_graph.is_empty() {
            return "- No dependency links yet.".to_string();
        }

        self.dependency_graph
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                format!(
                    "{idx}. {} -> {} ({})",
                    item.from, item.to, item.reason
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn export_markdown(&self) -> String {
        format!(
            r#"## Discovery & Framing Workspace

## Problem
{problem}

## Stakeholders
{stakeholders}

## Evidence
{evidence}

## Desired Outcome
{desired_outcome}

## Context Mapping
- Constraints: {constraints}
- Dependencies: {dependencies}
- Existing Systems: {existing_systems}

## Context Notes
{assumption_summary}

## Root Cause Working Set
{hypotheses}

## Success Metrics / Tradeoffs
- Metrics: {success_metrics}
- Tradeoffs: {tradeoffs}

## Frames
{frame_notes}

## Assumption Table
{assumptions}

## Frame comparison matrix
{frames}

## Decision gate
- Current gate: {decision}
- Current rationale: {decision_rationale}
- Risk accepted: {risk_accepted}
- Risk note: {risk_notes}

## Decision log
{decision_log}

## Risk dashboard
{risk_rows}

## Risks
{risks}

## Constraint/dependency graph
{dependencies_graph}
"#,
            problem = self.problem,
            stakeholders = self.stakeholders,
            evidence = self.evidence,
            desired_outcome = self.desired_outcome,
            constraints = self.constraints,
            dependencies = self.dependencies,
            existing_systems = self.existing_systems,
            assumption_summary = self.assumption_summary,
            hypotheses = self.hypotheses,
            success_metrics = self.success_metrics,
            tradeoffs = self.tradeoffs,
            frame_notes = self.frame_notes,
            assumptions = self.format_assumption_rows(),
            frames = self.format_frame_rows(),
            decision = self.decision,
            decision_rationale = self.decision_rationale,
            risk_accepted = self.risk_accepted,
            risk_notes = self.risk_notes,
            decision_log = self.format_decision_rows(),
            risk_rows = self.format_risk_rows(),
            risks = self.risks,
            dependencies_graph = self.format_constraint_rows(),
        )
    }
}

fn workspace_path(root: &str) -> PathBuf {
    Path::new(root)
        .join(".caretta")
        .join("discovery")
        .join("workspace.json")
}

pub fn load_discovery_workspace(root: &str) -> DiscoveryWorkspace {
    let path = workspace_path(root);
    let Some(raw) = std::fs::read_to_string(path).ok() else {
        return DiscoveryWorkspace::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_discovery_workspace(root: &str, workspace: &DiscoveryWorkspace) -> Result<(), String> {
    let path = workspace_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create {}: {err}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(workspace)
        .map_err(|err| format!("Failed to serialize discovery workspace: {err}"))?;
    std::fs::write(path, serialized)
        .map_err(|err| format!("Failed to write discovery workspace: {err}"))
}

#[component]
pub fn DiscoveryPanel(
    root: Signal<String>,
    workspace: Signal<DiscoveryWorkspace>,
) -> Element {
    let mut status = use_signal(|| None::<String>);
    let mut import_text = use_signal(String::new);

    let sync_preview_from_workspace = move || {
        let ws = workspace.read();
        serde_json::to_string_pretty(&*ws)
            .unwrap_or_else(|_| "{}".to_string())
    };

    rsx! {
        div { class: "discovery-panel",
            div { class: "discovery-header",
                h2 { class: "discovery-title", "Discovery & Framing Workspace" }
                div { class: "discovery-subtitle",
                    "Capture problem context once, then reuse it across discovery, planning, and risk workflows."
                }
            }

            div { class: "discovery-grid",
                div { class: "discovery-card",
                    div { class: "discovery-section-title", "Problem Intake" }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "What is the problem?",
                        value: "{workspace.read().problem}",
                        oninput: move |evt| workspace.write().problem = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Stakeholders and owners",
                        value: "{workspace.read().stakeholders}",
                        oninput: move |evt| workspace.write().stakeholders = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Evidence (files, links, metrics, incidents)",
                        value: "{workspace.read().evidence}",
                        oninput: move |evt| workspace.write().evidence = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Desired outcome / north star",
                        value: "{workspace.read().desired_outcome}",
                        oninput: move |evt| workspace.write().desired_outcome = evt.value(),
                    }
                }

                div { class: "discovery-card discovery-card-wide",
                    div { class: "discovery-section-title", "Assumption Table" }
                    div { class: "discovery-table-controls",
                        button {
                            class: "btn btn-xs btn-action",
                            onclick: move |_| {
                                let mut ws = workspace.read().clone();
                                ws.assumptions.push(AssumptionRecord::default());
                                workspace.set(ws);
                            },
                            "Add assumption"
                        }
                    }
                    div { class: "discovery-table",
                        div { class: "discovery-table-head",
                            div { class: "discovery-cell-head", "Status" }
                            div { class: "discovery-cell-head", "Confidence" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Evidence" }
                            div { class: "discovery-cell-head", "Owner" }
                            div { class: "discovery-cell-head discovery-cell-step", "Next validation step" }
                            div { class: "discovery-cell-head discovery-cell-action", "" }
                        }
                        if workspace.read().assumptions.is_empty() {
                            div { class: "discovery-empty", "Add rows to capture assumptions and status confidence for each claim." }
                        }
                        for row_idx in 0..workspace.read().assumptions.len() {
                            div { key: "assumption-{row_idx}", class: "discovery-table-row",
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().assumptions[row_idx].status}",
                                    placeholder: "Status",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.assumptions.get_mut(row_idx) {
                                            row.status = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().assumptions[row_idx].confidence}",
                                    placeholder: "Confidence",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.assumptions.get_mut(row_idx) {
                                            row.confidence = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().assumptions[row_idx].evidence}",
                                    placeholder: "Evidence",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.assumptions.get_mut(row_idx) {
                                            row.evidence = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().assumptions[row_idx].owner}",
                                    placeholder: "Owner",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.assumptions.get_mut(row_idx) {
                                            row.owner = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-step",
                                    rows: "2",
                                    value: "{workspace.read().assumptions[row_idx].validation_next_step}",
                                    placeholder: "Validation next step",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.assumptions.get_mut(row_idx) {
                                            row.validation_next_step = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-discovery",
                                    onclick: move |_| {
                                        let mut ws = workspace.read().clone();
                                        if row_idx < ws.assumptions.len() {
                                            ws.assumptions.remove(row_idx);
                                        }
                                        workspace.set(ws);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }

                div { class: "discovery-card",
                    div { class: "discovery-section-title", "Context Mapping" }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Constraints (compliance, policy, budget, platform)",
                        value: "{workspace.read().constraints}",
                        oninput: move |evt| workspace.write().constraints = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Dependencies and integration boundaries",
                        value: "{workspace.read().dependencies}",
                        oninput: move |evt| workspace.write().dependencies = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Existing systems and touchpoints",
                        value: "{workspace.read().existing_systems}",
                        oninput: move |evt| workspace.write().existing_systems = evt.value(),
                    }
                }

                div { class: "discovery-card discovery-card-wide",
                    div { class: "discovery-section-title", "Frame comparison matrix" }
                    div { class: "discovery-table-controls",
                        button {
                            class: "btn btn-xs btn-action",
                            onclick: move |_| {
                                let mut ws = workspace.read().clone();
                                ws.frame_comparisons.push(FrameComparisonRow::default());
                                workspace.set(ws);
                            },
                            "Add frame"
                        }
                    }
                    div { class: "discovery-table discovery-table-frame",
                        div { class: "discovery-table-head",
                            div { class: "discovery-cell-head", "Frame" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Framing" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Evidence" }
                            div { class: "discovery-cell-head", "Tradeoffs" }
                            div { class: "discovery-cell-head", "Recommendation" }
                            div { class: "discovery-cell-head discovery-cell-action", "" }
                        }
                        if workspace.read().frame_comparisons.is_empty() {
                            div { class: "discovery-empty", "Add competing frames to compare across technical, incentive, operational, UX, and coordination views." }
                        }
                        for row_idx in 0..workspace.read().frame_comparisons.len() {
                            div { key: "frame-{row_idx}", class: "discovery-table-row",
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().frame_comparisons[row_idx].frame}",
                                    placeholder: "Frame label",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.frame_comparisons.get_mut(row_idx) {
                                            row.frame = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().frame_comparisons[row_idx].framing}",
                                    placeholder: "Framing",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.frame_comparisons.get_mut(row_idx) {
                                            row.framing = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().frame_comparisons[row_idx].evidence}",
                                    placeholder: "Evidence",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.frame_comparisons.get_mut(row_idx) {
                                            row.evidence = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell",
                                    rows: "2",
                                    value: "{workspace.read().frame_comparisons[row_idx].tradeoffs}",
                                    placeholder: "Tradeoffs",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.frame_comparisons.get_mut(row_idx) {
                                            row.tradeoffs = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell",
                                    rows: "2",
                                    value: "{workspace.read().frame_comparisons[row_idx].recommendation}",
                                    placeholder: "Recommendation",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.frame_comparisons.get_mut(row_idx) {
                                            row.recommendation = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-discovery",
                                    onclick: move |_| {
                                        let mut ws = workspace.read().clone();
                                        if row_idx < ws.frame_comparisons.len() {
                                            ws.frame_comparisons.remove(row_idx);
                                        }
                                        workspace.set(ws);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }

                div { class: "discovery-card discovery-card-wide",
                    div { class: "discovery-section-title", "Decision Log" }
                    div { class: "discovery-table-controls",
                        button {
                            class: "btn btn-xs btn-action",
                            onclick: move |_| {
                                let mut ws = workspace.read().clone();
                                ws.decision_log.push(DecisionRecord::default());
                                workspace.set(ws);
                            },
                            "Add decision entry"
                        }
                    }
                    label { class: "control-row",
                        span { class: "control-label", "Current gate" }
                        input {
                            class: "text-input",
                            r#type: "text",
                            value: "{workspace.read().decision}",
                            placeholder: "proceed | experiment | reframe | kill",
                            oninput: move |evt| workspace.write().decision = evt.value(),
                        }
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Current decision rationale and owner notes",
                        value: "{workspace.read().decision_rationale}",
                        oninput: move |evt| workspace.write().decision_rationale = evt.value(),
                    }
                    label { class: "control-row",
                        input {
                            r#type: "checkbox",
                            checked: workspace.read().risk_accepted,
                            onchange: move |evt| workspace.write().risk_accepted = evt.value().parse::<bool>().unwrap_or(false),
                        }
                        span { "Risk acceptable for next step" }
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Accepted risk notes",
                        value: "{workspace.read().risk_notes}",
                        oninput: move |evt| workspace.write().risk_notes = evt.value(),
                    }
                    div { class: "discovery-table discovery-table-decision",
                        div { class: "discovery-table-head",
                            div { class: "discovery-cell-head", "Gate" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Rationale" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Rejected alternatives" }
                            div { class: "discovery-cell-head", "Reversibility" }
                            div { class: "discovery-cell-head discovery-cell-action", "" }
                        }
                        if workspace.read().decision_log.is_empty() {
                            div { class: "discovery-empty", "Add log rows to keep historical decision context." }
                        }
                        for row_idx in 0..workspace.read().decision_log.len() {
                            div { key: "decision-{row_idx}", class: "discovery-table-row",
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().decision_log[row_idx].gate}",
                                    placeholder: "Gate",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.decision_log.get_mut(row_idx) {
                                            row.gate = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().decision_log[row_idx].rationale}",
                                    placeholder: "Rationale",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.decision_log.get_mut(row_idx) {
                                            row.rationale = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().decision_log[row_idx].rejected_alternatives}",
                                    placeholder: "Rejected alternatives",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.decision_log.get_mut(row_idx) {
                                            row.rejected_alternatives = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell",
                                    rows: "2",
                                    value: "{workspace.read().decision_log[row_idx].reversibility}",
                                    placeholder: "Reversibility",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.decision_log.get_mut(row_idx) {
                                            row.reversibility = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-discovery",
                                    onclick: move |_| {
                                        let mut ws = workspace.read().clone();
                                        if row_idx < ws.decision_log.len() {
                                            ws.decision_log.remove(row_idx);
                                        }
                                        workspace.set(ws);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }

                div { class: "discovery-card discovery-card-wide",
                    div { class: "discovery-section-title", "Risk Dashboard" }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Additional high-level risks",
                        value: "{workspace.read().risks}",
                        oninput: move |evt| workspace.write().risks = evt.value(),
                    }
                    div { class: "discovery-table-controls",
                        button {
                            class: "btn btn-xs btn-action",
                            onclick: move |_| {
                                let mut ws = workspace.read().clone();
                                ws.risk_dashboard.push(RiskRow::default());
                                workspace.set(ws);
                            },
                            "Add risk"
                        }
                    }
                    div { class: "discovery-table discovery-table-risk",
                        div { class: "discovery-table-head",
                            div { class: "discovery-cell-head", "Likelihood" }
                            div { class: "discovery-cell-head", "Impact" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Trigger" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Mitigation" }
                            div { class: "discovery-cell-head discovery-cell-action", "" }
                        }
                        if workspace.read().risk_dashboard.is_empty() {
                            div { class: "discovery-empty", "Add risk entries to track likelihood/impact with mitigation and trigger signals." }
                        }
                        for row_idx in 0..workspace.read().risk_dashboard.len() {
                            div { key: "risk-{row_idx}", class: "discovery-table-row",
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().risk_dashboard[row_idx].likelihood}",
                                    placeholder: "Likelihood",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.risk_dashboard.get_mut(row_idx) {
                                            row.likelihood = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().risk_dashboard[row_idx].impact}",
                                    placeholder: "Impact",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.risk_dashboard.get_mut(row_idx) {
                                            row.impact = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().risk_dashboard[row_idx].trigger}",
                                    placeholder: "Trigger",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.risk_dashboard.get_mut(row_idx) {
                                            row.trigger = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().risk_dashboard[row_idx].mitigation}",
                                    placeholder: "Mitigation",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(row) = ws.risk_dashboard.get_mut(row_idx) {
                                            row.mitigation = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-discovery",
                                    onclick: move |_| {
                                        let mut ws = workspace.read().clone();
                                        if row_idx < ws.risk_dashboard.len() {
                                            ws.risk_dashboard.remove(row_idx);
                                        }
                                        workspace.set(ws);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }

                div { class: "discovery-card",
                    div { class: "discovery-section-title", "Root Cause and Hypotheses" }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Hypotheses, failure modes, and leverage points",
                        value: "{workspace.read().hypotheses}",
                        oninput: move |evt| workspace.write().hypotheses = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "What does success look like this quarter?",
                        value: "{workspace.read().assumption_summary}",
                        oninput: move |evt| workspace.write().assumption_summary = evt.value(),
                    }
                }

                div { class: "discovery-card",
                    div { class: "discovery-section-title", "Frame notes" }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "technical / incentive / operational / UX / coordination frames",
                        value: "{workspace.read().frame_notes}",
                        oninput: move |evt| workspace.write().frame_notes = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Success metrics and measurable targets",
                        value: "{workspace.read().success_metrics}",
                        oninput: move |evt| workspace.write().success_metrics = evt.value(),
                    }
                    textarea {
                        class: "discovery-textarea",
                        placeholder: "Tradeoffs and conflicts",
                        value: "{workspace.read().tradeoffs}",
                        oninput: move |evt| workspace.write().tradeoffs = evt.value(),
                    }
                }

                div { class: "discovery-card discovery-card-wide",
                    div { class: "discovery-section-title", "Constraint / Dependency Graph" }
                    div { class: "discovery-table-controls",
                        button {
                            class: "btn btn-xs btn-action",
                            onclick: move |_| {
                                let mut ws = workspace.read().clone();
                                ws.dependency_graph.push(ConstraintLink::default());
                                workspace.set(ws);
                            },
                            "Add link"
                        }
                    }
                    div { class: "discovery-table discovery-table-compact",
                        div { class: "discovery-table-head",
                            div { class: "discovery-cell-head", "From" }
                            div { class: "discovery-cell-head", "To" }
                            div { class: "discovery-cell-head discovery-cell-evidence", "Reason" }
                            div { class: "discovery-cell-head discovery-cell-action", "" }
                        }
                        for row_idx in 0..workspace.read().dependency_graph.len() {
                            div { key: "dep-{row_idx}", class: "discovery-table-row",
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().dependency_graph[row_idx].from}",
                                    placeholder: "From",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(item) = ws.dependency_graph.get_mut(row_idx) {
                                            item.from = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                input {
                                    class: "discovery-cell",
                                    r#type: "text",
                                    value: "{workspace.read().dependency_graph[row_idx].to}",
                                    placeholder: "To",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(item) = ws.dependency_graph.get_mut(row_idx) {
                                            item.to = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                textarea {
                                    class: "discovery-cell discovery-cell-evidence",
                                    rows: "2",
                                    value: "{workspace.read().dependency_graph[row_idx].reason}",
                                    placeholder: "Reason / coupling",
                                    oninput: move |evt| {
                                        let mut ws = workspace.read().clone();
                                        if let Some(item) = ws.dependency_graph.get_mut(row_idx) {
                                            item.reason = evt.value();
                                        }
                                        workspace.set(ws);
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-discovery",
                                    onclick: move |_| {
                                        let mut ws = workspace.read().clone();
                                        if row_idx < ws.dependency_graph.len() {
                                            ws.dependency_graph.remove(row_idx);
                                        }
                                        workspace.set(ws);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "discovery-actions",
                button {
                    class: "btn btn-sm btn-go",
                    onclick: move |_| {
                        let ws = workspace.read().clone();
                        let cfg_root = root.read().clone();
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            status.set(match save_discovery_workspace(&cfg_root, &ws) {
                                Ok(()) => Some("Discovery workspace saved.".to_string()),
                                Err(err) => Some(format!("Save failed: {err}")),
                            });
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            status.set(Some("Save is unavailable in web mode. Use desktop build.".to_string()));
                        }
                    },
                    "Save Workspace"
                }
                button {
                    class: "btn btn-sm btn-action",
                    onclick: move |_| {
                        import_text.set(sync_preview_from_workspace());
                    },
                    "Export JSON"
                }
                button {
                    class: "btn btn-sm btn-security",
                    onclick: move |_| {
                        workspace.set(DiscoveryWorkspace::default());
                        import_text.set(String::new());
                        status.set(Some("Workspace reset.".to_string()));
                    },
                    "Reset"
                }
                button {
                    class: "btn btn-sm btn-go",
                    onclick: move |_| {
                        let text = import_text.read().clone();
                        if text.trim().is_empty() {
                            status.set(Some("Paste JSON into the box first.".to_string()));
                            return;
                        }
                        match serde_json::from_str::<DiscoveryWorkspace>(&text) {
                            Ok(ws) => {
                                workspace.set(ws);
                                status.set(Some("Workspace imported.".to_string()));
                            }
                            Err(err) => {
                                status.set(Some(format!("Invalid JSON: {err}")));
                            }
                        }
                    },
                    "Import JSON"
                }
            }

            if let Some(message) = status.read().clone() {
                div { class: "discovery-status", "{message}" }
            }

            div { class: "discovery-card",
                div { class: "discovery-section-title", "Workspace Snapshot (for prompts)" }
                textarea {
                    class: "discovery-textarea discovery-preview",
                    disabled: true,
                    value: "{workspace.read().export_markdown()}",
                }
            }
            textarea {
                class: "discovery-textarea discovery-import",
                rows: "8",
                placeholder: "Use this box to paste imported JSON",
                value: "{import_text.read()}",
                oninput: move |evt| import_text.set(evt.value()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/discovery-workspace.json")
    }

    fn read_fixture() -> String {
        std::fs::read_to_string(fixture_path()).expect("read discovery fixture")
    }

    fn temp_root() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn workspace_export_contains_sections() {
        let raw = read_fixture();
        let ws: DiscoveryWorkspace = serde_json::from_str(&raw).expect("parse fixture");
        let text = ws.export_markdown();

        assert!(text.contains("## Assumption Table"));
        assert!(text.contains("## Frame comparison matrix"));
        assert!(text.contains("## Decision log"));
        assert!(text.contains("## Risk dashboard"));
        assert!(text.contains("## Constraint/dependency graph"));
    }

    #[test]
    fn workspace_json_roundtrip_from_fixture() {
        let raw = read_fixture();
        let ws: DiscoveryWorkspace = serde_json::from_str(&raw).expect("parse fixture");
        let json = serde_json::to_string_pretty(&ws).expect("serialize");
        let parsed: DiscoveryWorkspace = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ws, parsed);
    }

    #[test]
    fn workspace_save_load_roundtrip() {
        let raw = read_fixture();
        let ws: DiscoveryWorkspace = serde_json::from_str(&raw).expect("parse fixture");
        let root = temp_root();
        let root_str = root.path().to_str().expect("root path");

        save_discovery_workspace(root_str, &ws).expect("save fixture workspace");
        let loaded = load_discovery_workspace(root_str);
        assert_eq!(ws, loaded);
    }
}
