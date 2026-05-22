use crate::agent::types::{AgentEvent, ClaudeEvent, ContentBlock, PricingConfig};
use dioxus::prelude::*;
use std::collections::HashMap;

pub const BASE_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { height: 100%; overflow: hidden; }

body {
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    font-size: 13px;
}

/* ── IDE shell ── */
.ide {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
}

/* ── Title bar ── */
.titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding: 0 12px;
    background: var(--bg-tertiary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    -webkit-app-region: drag;
}
.titlebar-left { display: flex; align-items: center; gap: 8px; }
.titlebar-icon {
    color: var(--blue);
    font-family: monospace;
    font-weight: 700;
    font-size: 14px;
}
.titlebar-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-secondary);
    letter-spacing: 0.3px;
}
.titlebar-right { -webkit-app-region: no-drag; }
.titlebar-select {
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-secondary);
    color: var(--fg-muted);
    font-size: 11px;
    cursor: pointer;
    outline: none;
}

/* ── IDE body ── */
.ide-body {
    display: flex;
    flex: 1;
    min-height: 0;
}

/* ── Sidebar ── */
.sidebar {
    width: 220px;
    height: 100%;
    flex-shrink: 0;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
}

.sidebar-section {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
}
.sidebar-section-grow {
    flex: 3;
    min-height: 150px;
    border-bottom: none;
    overflow-y: auto;
}

.section-header {
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
    letter-spacing: 0.8px;
    text-transform: uppercase;
    margin-bottom: 8px;
}

.section-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
}
.section-header-row .section-header { margin-bottom: 0; }

.sidebar-controls { display: flex; flex-direction: column; gap: 6px; }

.control-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: var(--fg-secondary);
}
.control-label { font-size: 12px; color: var(--fg-muted); }

.checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--fg-secondary);
    cursor: pointer;
}
.checkbox-row input { accent-color: var(--blue); }

.select {
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-tertiary);
    color: var(--fg-primary);
    font-size: 12px;
    outline: none;
}

.text-input {
    width: 100%;
    padding: 4px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-tertiary);
    color: var(--fg-primary);
    font-size: 12px;
    outline: none;
}
.text-input::placeholder { color: var(--fg-muted); }
.text-input:focus,
.select:focus { border-color: var(--blue); }

.advanced-controls {
    overflow: hidden;
    max-height: 0;
    opacity: 0;
    margin-top: 0;
    pointer-events: none;
    transition: max-height 0.16s ease, opacity 0.12s ease, margin-top 0.16s ease;
}
.advanced-controls-open {
    max-height: 320px;
    opacity: 1;
    margin-top: 4px;
    pointer-events: auto;
}
.advanced-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: color-mix(in srgb, var(--bg-tertiary) 82%, transparent);
}
.advanced-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
}
.advanced-hint {
    font-size: 10px;
    color: var(--fg-muted);
    line-height: 1.4;
}

.tracker-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    margin-bottom: 4px;
}
.tracker-info {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    font-size: 12px;
    color: var(--fg-primary);
    font-family: monospace;
}
.tracker-num { font-weight: 600; flex-shrink: 0; }
.tracker-title {
    color: var(--fg-muted);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.sidebar-buttons { display: flex; gap: 4px; }

.btn {
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--bg-tertiary);
    color: var(--fg-secondary);
    cursor: pointer;
    font-size: 11px;
    transition: background 0.1s;
}
.btn:hover { background: var(--border); color: var(--fg-primary); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn-sm { padding: 3px 10px; }
.btn-xs { padding: 1px 6px; font-size: 10px; flex-shrink: 0; }
.btn-go { border-color: var(--green); color: var(--green); }
.btn-go:hover { background: var(--green); color: var(--bg-primary); }
.btn-strategy { border-color: var(--cyan); color: var(--cyan); width: 100%; }
.btn-strategy:hover { background: var(--cyan); color: var(--bg-primary); }
.btn-action { border-color: var(--purple); color: var(--purple); width: 100%; }
.btn-action:hover { background: var(--purple); color: var(--bg-primary); }
.btn-retro { border-color: var(--yellow); color: var(--yellow); width: 100%; }
.btn-retro:hover { background: var(--yellow); color: var(--bg-primary); }
.btn-ideation { border-color: var(--magenta); color: var(--magenta); width: 100%; }
.btn-ideation:hover { background: var(--magenta); color: var(--bg-primary); }
.btn-report { border-color: var(--blue); color: var(--blue); width: 100%; }
.btn-report:hover { background: var(--blue); color: var(--bg-primary); }
.btn-security { border-color: var(--orange); color: var(--orange); width: 100%; }
.btn-security:hover { background: var(--orange); color: var(--bg-primary); }
.btn-discovery { border-color: var(--green); color: var(--green); width: 100%; }
.btn-discovery:hover { background: var(--green); color: var(--bg-primary); }
.btn-interview { border-color: var(--cyan); color: var(--cyan); width: 100%; }
.btn-interview:hover { background: var(--cyan); color: var(--bg-primary); }
.btn-merge { border-color: var(--green); color: var(--green); width: 100%; }
.btn-merge:hover { background: var(--green); color: var(--bg-primary); }
.btn-merge-active { background: var(--green); color: var(--bg-primary); opacity: 0.9; }
.btn-stop { border-color: var(--red); color: var(--red); width: 100%; }
.btn-stop:hover { background: var(--red); color: var(--bg-primary); }
.sidebar-buttons-col { flex-direction: column; }
.sidebar-buttons-divider {
    border: none;
    border-top: 1px solid var(--border);
    margin: 6px 2px;
    width: auto;
}

/* ── Feedback ── */
.feedback-hint {
    font-size: 11px;
    color: var(--fg-muted);
    margin-bottom: 6px;
    line-height: 1.4;
}
.feedback-input {
    width: 100%;
    min-height: 80px;
    padding: 6px 8px;
    border-radius: 3px;
    border: 1px solid var(--cyan);
    background: var(--bg-tertiary);
    color: var(--fg-primary);
    font-family: inherit;
    font-size: 12px;
    resize: vertical;
    outline: none;
    margin-bottom: 6px;
}
.feedback-input::placeholder { color: var(--fg-muted); }
.feedback-input:focus { border-color: var(--blue); }

/* ── Issue comment reminder ── */
.issue-comment-details {
    position: relative;
    display: inline-block;
}
.issue-comment-summary {
    list-style: none;
    cursor: pointer;
    font-size: 12px;
    opacity: 0.6;
    transition: opacity 0.1s;
    user-select: none;
}
.issue-comment-summary::-webkit-details-marker { display: none; }
.issue-comment-summary:hover { opacity: 1; }

.issue-comment-details[open] .issue-comment-reminder {
    display: block;
}

.issue-comment-reminder {
    position: absolute;
    top: 20px;
    right: 0;
    width: 200px;
    z-index: 100;
    border: 1px solid color-mix(in srgb, var(--yellow) 28%, var(--border));
    border-radius: 4px;
    background: var(--bg-secondary);
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    padding: 8px 9px;
    display: none;
}
.issue-comment-reminder-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--yellow);
    margin-bottom: 4px;
}
.issue-comment-reminder-copy,
.issue-comment-reminder-footer {
    font-size: 11px;
    line-height: 1.4;
    color: var(--fg-secondary);
}
.issue-comment-reminder-list {
    margin: 6px 0 0 16px;
    padding: 0;
}
.issue-comment-reminder-list li {
    margin-bottom: 4px;
    font-size: 11px;
    line-height: 1.3;
    color: var(--fg-secondary);
}
.issue-comment-reminder-footer {
    margin-top: 6px;
    color: var(--fg-muted);
}

/* ── Bot setup collapsible ── */
.bot-setup-details { width: 100%; }
.bot-setup-details summary { list-style: none; cursor: pointer; }
.bot-setup-details summary::-webkit-details-marker { display: none; }
.bot-setup-details[open] > :not(summary) { margin-top: 8px; }

/* ── Issue tree ── */
.issue-tree { list-style: none; }
.issue-node {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 0;
    font-size: 12px;
    color: var(--fg-secondary);
    min-width: 0;
    flex-wrap: wrap;
}
.issue-start { margin-left: auto; }
.dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
}
.dot-ready   { background: var(--green); }
.dot-blocked { background: var(--red); }
.issue-num { font-family: monospace; font-weight: 600; color: var(--fg-primary); }
.issue-title {
    font-size: 11px;
    color: var(--fg-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
}
.issue-pr {
    font-size: 10px;
    font-family: monospace;
    color: var(--cyan);
    background: color-mix(in srgb, var(--cyan) 12%, transparent);
    border-radius: 3px;
    padding: 0 4px;
    flex-shrink: 0;
}
.pr-thread-count {
    font-size: 10px;
    font-family: monospace;
    color: var(--orange, var(--red));
    flex-shrink: 0;
}
.issue-blockers {
    font-size: 10px;
    color: var(--red);
    font-family: monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.issue-blockers-label {
    color: var(--fg-muted);
    font-style: italic;
    font-family: inherit;
}
.issue-status { font-size: 10px; color: var(--fg-muted); font-style: italic; }
.pr-author {
    font-size: 10px;
    color: var(--fg-muted);
    font-family: monospace;
    font-style: italic;
    white-space: nowrap;
}

/* ── Editor area ── */
.editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
}

.tab-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    padding: 0 8px 0 0;
}
.tab {
    padding: 0 16px;
    height: 100%;
    display: flex;
    align-items: center;
    font-size: 12px;
    color: var(--fg-muted);
    border-right: 1px solid var(--border);
    cursor: default;
}
.tab-active {
    background: var(--bg-primary);
    color: var(--fg-primary);
    border-bottom: 2px solid var(--blue);
}

.tab-actions { display: flex; gap: 12px; }
.tab-check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--fg-muted);
    cursor: pointer;
}
.tab-check input { accent-color: var(--blue); }

.editor-content {
    flex: 1;
    overflow-y: auto;
    padding: 10px 14px 40px 14px;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", "Menlo", monospace;
    font-size: 12px;
    line-height: 1.5;
}

.editor-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-style: italic;
}

.editor-layout { display: flex; height: 100%; min-height: 0; }
.file-list {
    width: 250px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}
.file-list-container {
    flex: 1;
    overflow-y: auto;
}
.file-list-header {
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
}
.file-list-tabs {
    display: flex;
    height: 28px;
    border-bottom: 1px solid var(--border);
}
.file-list-tab {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--fg-muted);
    cursor: pointer;
    border-right: 1px solid var(--border);
}
.file-list-tab:last-child { border-right: none; }
.file-list-tab:hover { color: var(--fg-primary); }
.file-list-tab-active {
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-weight: 600;
}
.file-search {
    padding: 6px;
}
.file-search input {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 3px 6px;
    color: var(--fg-primary);
    font-size: 11px;
    outline: none;
}
.file-search input:focus { border-color: var(--blue); }
.file-entry {
    padding: 4px 8px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
}
.file-entry:hover { background: var(--bg-secondary); }
.file-entry-active { background: var(--bg-secondary); border-left: 2px solid var(--blue); }
.file-kind {
    width: 14px;
    text-align: center;
    font-weight: 700;
    flex-shrink: 0;
}
.file-kind-created { color: var(--green); }
.file-kind-modified { color: var(--yellow); }
.file-kind-deleted { color: var(--red); }
.file-kind-read { color: var(--fg-muted); }
.file-path {
    color: var(--fg-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
#monaco-container { flex: 1; height: 100%; min-width: 0; }

.text-muted { color: var(--fg-muted); }

/* ── Status bar ── */
.statusbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 24px;
    padding: 0 10px;
    background: var(--blue);
    color: var(--bg-primary);
    font-size: 11px;
    font-weight: 500;
    flex-shrink: 0;
}
.statusbar-left, .statusbar-right { display: flex; align-items: center; gap: 6px; }
.status-sep { opacity: 0.5; }
.status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
}
.status-dot-idle   { background: var(--bg-primary); opacity: 0.6; }
.status-dot-active { background: var(--green); animation: pulse 1.2s ease-in-out infinite; }
@keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
}

/* ── Event rows ── */
.ev-log { color: var(--green); margin-bottom: 2px; }
.ev-log .tag { color: var(--fg-muted); margin-right: 6px; }

.ev-system {
    border-left: 2px solid var(--blue);
    margin-bottom: 6px;
    padding: 3px 8px;
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--blue) 8%, transparent);
}
.ev-system .label { color: var(--blue); font-weight: 700; font-size: 11px; }
.ev-system .meta  { font-size: 10px; color: var(--fg-muted); }

.ev-assistant {
    border-left: 2px solid var(--green);
    margin-bottom: 6px;
    padding: 3px 8px;
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--green) 5%, transparent);
}
.ev-assistant .label { color: var(--green); font-weight: 700; font-size: 11px; margin-bottom: 3px; }

.ev-user {
    border-left: 2px solid var(--yellow);
    margin-bottom: 6px;
    padding: 3px 8px;
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--yellow) 5%, transparent);
}
.ev-user .label { color: var(--yellow); font-weight: 700; font-size: 11px; margin-bottom: 3px; }

.ev-result {
    border-left: 2px solid var(--purple);
    margin-bottom: 6px;
    padding: 3px 8px;
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--purple) 8%, transparent);
}
.ev-result .label { color: var(--purple); font-weight: 700; font-size: 11px; }
.ev-result .summary { margin-top: 2px; font-size: 11px; font-style: italic; color: var(--fg-secondary); }
.usage-metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
    font-size: 10px;
    color: var(--fg-secondary);
}
.usage-metric {
    padding: 1px 5px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--purple) 12%, transparent);
    white-space: nowrap;
}
.usage-cost {
    color: var(--green);
    background: color-mix(in srgb, var(--green) 12%, transparent);
}

/* ── Content blocks ── */
.block-text { white-space: pre-wrap; margin-bottom: 6px; }

.block-thinking {
    margin-bottom: 6px;
    background: var(--bg-tertiary);
    padding: 4px 8px;
    border-radius: 3px;
}
.block-thinking summary {
    color: var(--fg-muted);
    cursor: pointer;
    font-size: 11px;
}
.block-thinking summary:hover { color: var(--fg-secondary); }
.block-thinking .content {
    color: var(--fg-muted);
    margin-top: 3px;
    font-style: italic;
    font-size: 11px;
}

.block-tool-use {
    margin-bottom: 6px;
    background: color-mix(in srgb, var(--magenta) 6%, transparent);
    padding: 4px 8px;
    border-radius: 3px;
    border: 1px solid color-mix(in srgb, var(--magenta) 15%, transparent);
}
.block-tool-use summary {
    color: var(--magenta);
    cursor: pointer;
    font-size: 10px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
}
.block-tool-use summary:hover { color: var(--fg-secondary); }
.block-tool-use .tool-badge {
    display: inline-block;
    background: color-mix(in srgb, var(--magenta) 20%, transparent);
    border-radius: 2px;
    padding: 0 4px;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
}
.block-tool-use .tool-target {
    color: var(--fg-secondary);
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.block-tool-use pre {
    font-size: 11px;
    color: var(--fg-secondary);
    overflow-x: auto;
    white-space: pre-wrap;
    margin-top: 3px;
}

.block-tool-result {
    margin-bottom: 6px;
    background: color-mix(in srgb, var(--cyan) 5%, transparent);
    padding: 4px 8px;
    border-radius: 3px;
    border: 1px solid color-mix(in srgb, var(--cyan) 12%, transparent);
}
.block-tool-result summary {
    color: var(--cyan);
    cursor: pointer;
    font-size: 10px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
}
.block-tool-result summary:hover { color: var(--fg-secondary); }
.block-tool-result .result-badge {
    display: inline-block;
    border-radius: 2px;
    padding: 0 4px;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
}
.block-tool-result .result-badge-ok {
    background: color-mix(in srgb, var(--green) 20%, transparent);
    color: var(--green);
}
.block-tool-result .result-badge-err {
    background: color-mix(in srgb, var(--red) 20%, transparent);
    color: var(--red);
}
.block-tool-result .result-meta {
    color: var(--fg-muted);
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.block-tool-result pre {
    font-size: 11px;
    color: var(--fg-muted);
    margin-top: 3px;
    overflow-x: auto;
    white-space: pre-wrap;
}

/* ── Changed files ── */
.files-summary {
    display: flex;
    gap: 12px;
    padding: 6px 0;
    margin-bottom: 8px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
}
.file-stat {
    font-weight: 600;
    font-family: monospace;
}
.file-stat-created { color: var(--green); }
.file-stat-modified { color: var(--yellow); }
.file-stat-read { color: var(--fg-muted); }

.tab { cursor: pointer; user-select: none; }
.tab:hover { color: var(--fg-primary); }

/* ── Persona studio ── */
.persona-studio {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    overflow: hidden;
    font-family: inherit;
}
.persona-nav {
    min-width: 0;
    border-right: 1px solid var(--border);
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}
.persona-nav-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 12px 8px;
    border-bottom: 1px solid var(--border);
}
.persona-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--fg-muted);
}
.persona-count {
    min-width: 22px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    background: color-mix(in srgb, var(--cyan) 12%, transparent);
    color: var(--cyan);
    font-size: 10px;
    font-weight: 700;
    font-family: monospace;
}
.persona-generate {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
}
.persona-seed-input {
    width: 100%;
    min-height: 72px;
    max-height: 150px;
    padding: 7px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-family: inherit;
    font-size: 12px;
    line-height: 1.45;
    resize: vertical;
    outline: none;
}
.persona-seed-input:focus { border-color: var(--blue); }
.persona-seed-input::placeholder { color: var(--fg-muted); }
.persona-nav-actions {
    display: flex;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
}
.persona-nav-actions .btn { flex: 1; }
.persona-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px;
}
.persona-list-item {
    padding: 8px 9px;
    border-radius: 4px;
    border: 1px solid transparent;
    cursor: pointer;
    margin-bottom: 6px;
}
.persona-list-item:hover {
    background: color-mix(in srgb, var(--bg-tertiary) 72%, transparent);
}
.persona-list-item-active {
    background: var(--bg-primary);
    border-color: color-mix(in srgb, var(--blue) 45%, var(--border));
}
.persona-list-name {
    font-size: 12px;
    font-weight: 700;
    color: var(--fg-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.persona-list-role,
.persona-list-cue {
    margin-top: 3px;
    font-size: 11px;
    line-height: 1.35;
    color: var(--fg-muted);
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
}
.persona-list-role { color: var(--cyan); }
.persona-empty {
    padding: 24px 10px;
    color: var(--fg-muted);
    font-size: 12px;
    text-align: center;
}
.persona-editor {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-primary);
}
.persona-editor-toolbar {
    min-height: 38px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
}
.persona-path {
    min-width: 0;
    color: var(--fg-muted);
    font-family: monospace;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.persona-toolbar-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
}
.btn-danger { border-color: var(--red); color: var(--red); }
.btn-danger:hover { background: var(--red); color: var(--bg-primary); }
.persona-status {
    margin: 10px 14px 0;
    padding: 6px 8px;
    border-left: 2px solid var(--cyan);
    background: color-mix(in srgb, var(--cyan) 6%, transparent);
    color: var(--fg-secondary);
    font-size: 11px;
}
.persona-editor-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--fg-muted);
    font-style: italic;
}
.persona-form {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px;
}
.persona-form-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
}
.persona-form-grid-facts {
    align-items: stretch;
}
.persona-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    color: var(--fg-muted);
    font-size: 11px;
    font-weight: 600;
}
.persona-field-wide {
    margin-top: 10px;
}
.persona-textarea {
    width: 100%;
    min-height: 118px;
    padding: 7px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-tertiary);
    color: var(--fg-primary);
    font-family: inherit;
    font-size: 12px;
    line-height: 1.45;
    resize: vertical;
    outline: none;
}
.persona-textarea-sm { min-height: 68px; }
.persona-textarea:focus { border-color: var(--blue); }
.persona-section-label {
    margin: 14px 0 8px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--fg-muted);
}
@media (max-width: 760px) {
    .persona-studio {
        grid-template-columns: minmax(0, 1fr);
        grid-template-rows: minmax(220px, 42vh) minmax(0, 1fr);
    }
    .persona-nav {
        border-right: none;
        border-bottom: 1px solid var(--border);
    }
    .persona-editor-toolbar {
        align-items: stretch;
        flex-direction: column;
    }
    .persona-toolbar-actions .btn { flex: 1; }
    .persona-form-grid { grid-template-columns: minmax(0, 1fr); }
}

/* ── Security panel ── */
.security-panel { font-family: inherit; }

.sec-summary {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    margin-bottom: 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    flex-wrap: wrap;
}
.sec-stat { font-weight: 600; font-family: monospace; }
.sec-export { margin-left: auto; }

.score-badge {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 3px;
    letter-spacing: 0.3px;
}
.score-good { background: color-mix(in srgb, var(--green) 20%, transparent); color: var(--green); }
.score-mixed { background: color-mix(in srgb, var(--yellow) 20%, transparent); color: var(--yellow); }
.score-bad { background: color-mix(in srgb, var(--red) 20%, transparent); color: var(--red); }

.sec-category { margin-bottom: 14px; }
.sec-category-header {
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
    letter-spacing: 0.8px;
    text-transform: uppercase;
    margin-bottom: 6px;
    padding-bottom: 3px;
    border-bottom: 1px solid var(--border);
}

.sec-finding {
    margin-bottom: 6px;
    padding: 6px 8px;
    border-radius: 3px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
}
.sec-finding-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
}
.sec-finding-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-primary);
}
.sec-finding-desc {
    font-size: 11px;
    color: var(--fg-secondary);
    line-height: 1.5;
}

.sec-sev-badge, .sec-status-badge {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 2px;
    letter-spacing: 0.5px;
    flex-shrink: 0;
}
.sev-critical { background: color-mix(in srgb, var(--red) 20%, transparent); color: var(--red); }
.sev-high { background: color-mix(in srgb, var(--orange) 20%, transparent); color: var(--orange); }
.sev-medium { background: color-mix(in srgb, var(--yellow) 20%, transparent); color: var(--yellow); }
.sev-low { background: color-mix(in srgb, var(--blue) 20%, transparent); color: var(--blue); }
.sev-info { background: color-mix(in srgb, var(--fg-muted) 15%, transparent); color: var(--fg-muted); }

.status-pass { background: color-mix(in srgb, var(--green) 20%, transparent); color: var(--green); }
.status-fail { background: color-mix(in srgb, var(--red) 20%, transparent); color: var(--red); }
.status-warn { background: color-mix(in srgb, var(--yellow) 20%, transparent); color: var(--yellow); }

.sec-remediation {
    margin-top: 4px;
    padding: 4px 6px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--cyan) 6%, transparent);
    border-left: 2px solid var(--cyan);
    font-size: 11px;
    color: var(--fg-secondary);
    line-height: 1.4;
}
.sec-remediation-label {
    font-weight: 600;
    color: var(--cyan);
}

/* ── Scrollbar ── */
.editor-content::-webkit-scrollbar, .sidebar::-webkit-scrollbar { width: 8px; }
.editor-content::-webkit-scrollbar-track, .sidebar::-webkit-scrollbar-track {
    background: transparent;
}
.editor-content::-webkit-scrollbar-thumb, .sidebar::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 4px;
}
.editor-content::-webkit-scrollbar-thumb:hover, .sidebar::-webkit-scrollbar-thumb:hover {
    background: var(--fg-muted);
}

/* ── Interview dialog ── */
.interview-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    padding: 16px 20px;
    gap: 12px;
}
.interview-empty {
    color: var(--fg-muted);
    text-align: center;
    padding: 32px 16px;
    font-style: italic;
}
.interview-section-header {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--cyan);
    padding: 8px 0 4px;
    border-bottom: 1px solid var(--border);
    margin-top: 8px;
}
.interview-turn {
    display: flex;
    flex-direction: column;
    max-width: 85%;
}
.interview-turn-agent {
    align-self: flex-start;
}
.interview-turn-user {
    align-self: flex-end;
}
.interview-bubble {
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-wrap: break-word;
}
.interview-bubble-agent {
    background: color-mix(in srgb, var(--cyan) 8%, var(--bg-secondary));
    border-left: 3px solid var(--cyan);
    color: var(--fg-primary);
}
.interview-bubble-user {
    background: color-mix(in srgb, var(--purple) 10%, var(--bg-secondary));
    border-right: 3px solid var(--purple);
    color: var(--fg-primary);
    text-align: left;
}
.interview-role {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    margin-bottom: 4px;
    color: var(--fg-muted);
}
.interview-role-agent { color: var(--cyan); }
.interview-role-user { color: var(--purple); text-align: right; }
.interview-summary-card {
    margin-top: 16px;
    padding: 16px;
    border-radius: 6px;
    border: 1px solid var(--cyan);
    background: color-mix(in srgb, var(--cyan) 5%, var(--bg-secondary));
}
.interview-summary-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--cyan);
    margin-bottom: 8px;
    letter-spacing: 0.3px;
}
.interview-summary-body {
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    word-wrap: break-word;
    color: var(--fg-primary);
}
.interview-status {
    text-align: center;
    padding: 8px;
    font-size: 11px;
    color: var(--fg-muted);
    font-style: italic;
}

/* ── Chat panel ── */
.chat-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
}
.chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
}
.chat-empty {
    color: var(--fg-muted);
    text-align: center;
    padding: 32px 16px;
    font-style: italic;
    line-height: 1.6;
}
.chat-typing {
    text-align: center;
    padding: 8px;
    font-size: 11px;
    color: var(--fg-muted);
    font-style: italic;
}
.chat-input-area {
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    background: var(--bg-secondary);
    flex-shrink: 0;
    align-items: flex-end;
}
.chat-input {
    flex: 1;
    min-height: 36px;
    max-height: 120px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-family: inherit;
    font-size: 13px;
    line-height: 1.4;
    resize: none;
    outline: none;
}
.chat-input:focus {
    border-color: var(--blue);
}
.chat-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
.chat-send-btn {
    flex-shrink: 0;
    height: 36px;
    padding: 0 16px;
}

.discovery-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 14px 20px;
    overflow-y: auto;
}
.discovery-header {
    display: flex;
    flex-direction: column;
    gap: 3px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 10px;
}
.discovery-title {
    font-size: 13px;
    color: var(--fg-primary);
}
.discovery-subtitle {
    font-size: 11px;
    color: var(--fg-muted);
}
.discovery-grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
}
.discovery-card {
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-secondary);
    min-width: 0;
}
.discovery-card-wide {
    grid-column: 1 / -1;
}
.discovery-section-title {
    margin-bottom: 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.3px;
    color: var(--fg-muted);
    text-transform: uppercase;
}
.discovery-textarea {
    width: 100%;
    min-height: 68px;
    padding: 6px 7px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-size: 11px;
    resize: vertical;
    outline: none;
    margin-bottom: 6px;
}
.discovery-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
}
.discovery-status {
    color: var(--fg-muted);
    font-size: 11px;
    font-style: italic;
}
.discovery-preview {
    min-height: 180px;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", "Menlo", monospace;
    font-size: 11px;
}
.discovery-import {
    min-height: 96px;
    font-size: 11px;
    font-family: inherit;
}
.discovery-table-controls {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 6px;
}
.discovery-table {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
}
.discovery-table-head,
.discovery-table-row {
    display: grid;
    gap: 6px;
    align-items: flex-start;
    grid-template-columns: 1fr 1fr 2fr 1fr 2fr 92px;
}
.discovery-table-frame .discovery-table-head,
.discovery-table-frame .discovery-table-row {
    grid-template-columns: 0.9fr 1.3fr 1.3fr 1fr 1fr 92px;
}
.discovery-table-decision .discovery-table-head,
.discovery-table-decision .discovery-table-row {
    grid-template-columns: 1fr 2fr 2fr 1fr 92px;
}
.discovery-table-risk .discovery-table-head,
.discovery-table-risk .discovery-table-row {
    grid-template-columns: 1fr 1fr 2fr 2fr 92px;
}
.discovery-table-compact .discovery-table-head,
.discovery-table-compact .discovery-table-row {
    grid-template-columns: 1fr 1fr 2fr 92px;
}
.discovery-cell-head,
.discovery-cell {
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-primary);
    color: var(--fg-primary);
    font-size: 11px;
    padding: 4px 6px;
    min-width: 0;
}
.discovery-cell {
    width: 100%;
}
.discovery-cell-head {
    background: var(--bg-tertiary);
    border-color: var(--border);
    color: var(--fg-muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.2px;
    text-transform: uppercase;
}
.discovery-cell-evidence {
    min-height: 52px;
}
.discovery-cell-step {
    min-height: 44px;
}
.discovery-cell-action {
    display: flex;
    justify-content: center;
}
.discovery-table-controls .btn,
.discovery-cell-action .btn,
.discovery-actions .btn {
    width: auto;
}
.discovery-empty {
    padding: 6px 4px;
    color: var(--fg-muted);
    font-size: 11px;
    font-style: italic;
}
"#;

#[component]
pub fn EventRow(
    event: AgentEvent,
    expand_all: bool,
    tool_names: HashMap<String, String>,
    usage_model: Option<String>,
    pricing: PricingConfig,
) -> Element {
    match event {
        AgentEvent::Log(msg) => rsx! {
            div { class: "ev-log",
                span { class: "tag", "[LOG]" }
                "{msg}"
            }
        },
        AgentEvent::Claude(ev) => rsx! {
            ClaudeEventRow { ev, expand_all, tool_names, usage_model, pricing }
        },
        AgentEvent::Done | AgentEvent::AwaitingFeedback(_) | AgentEvent::TrackerUpdate(_) => {
            rsx! {}
        }
    }
}

#[component]
pub fn ClaudeEventRow(
    ev: ClaudeEvent,
    expand_all: bool,
    tool_names: HashMap<String, String>,
    usage_model: Option<String>,
    pricing: PricingConfig,
) -> Element {
    match ev {
        ClaudeEvent::System {
            subtype,
            model,
            description,
            ..
        } => rsx! {
            div { class: "ev-system",
                div { class: "label", "SYSTEM: {subtype}" }
                if let Some(m) = model { div { class: "meta", "Model: {m}" } }
                if let Some(d) = description { div { "{d}" } }
            }
        },
        ClaudeEvent::Assistant { message } => rsx! {
            div { class: "ev-assistant",
                div { class: "label", "ASSISTANT" }
                for block in message.content {
                    ContentBlockRow { block, expand_all, tool_names: tool_names.clone() }
                }
            }
        },
        ClaudeEvent::User { message } => {
            let is_tool_result = message
                .content
                .iter()
                .all(|b| matches!(b, ContentBlock::ToolResult { .. }));
            let label = if is_tool_result { "TOOL" } else { "USER" };
            rsx! {
                div { class: "ev-user",
                    div { class: "label", "{label}" }
                    for block in message.content {
                        ContentBlockRow { block, expand_all, tool_names: tool_names.clone() }
                    }
                }
            }
        }
        ClaudeEvent::Result {
            status,
            summary,
            duration_ms,
            input_tokens,
            output_tokens,
        } => {
            let input_label = input_tokens.map(format_token_count);
            let output_label = output_tokens.map(format_token_count);
            let total_tokens = input_tokens
                .unwrap_or(0)
                .saturating_add(output_tokens.unwrap_or(0));
            let total_label = (total_tokens > 0).then(|| format_token_count(total_tokens));
            let duration_label = duration_ms.map(format_duration_ms);
            let output_rate_label = output_tokens
                .zip(duration_ms)
                .and_then(|(tokens, ms)| format_tokens_per_second(tokens, ms))
                .map(|rate| format!("{rate} out tok/s"));
            let total_rate_label = (total_tokens > 0)
                .then_some(total_tokens)
                .zip(duration_ms)
                .and_then(|(tokens, ms)| format_tokens_per_second(tokens, ms))
                .map(|rate| format!("{rate} total tok/s"));
            let cost_label = usage_model
                .as_deref()
                .and_then(|model| {
                    pricing.estimate_cost_usd(
                        model,
                        input_tokens.unwrap_or(0),
                        output_tokens.unwrap_or(0),
                    )
                })
                .map(format_cost_usd);

            rsx! {
                div { class: "ev-result",
                    div { class: "label", "RESULT: {status}" }
                    div { class: "usage-metrics",
                        if let Some(input) = input_label { span { class: "usage-metric", "input {input}" } }
                        if let Some(output) = output_label { span { class: "usage-metric", "output {output}" } }
                        if let Some(total) = total_label { span { class: "usage-metric", "total {total}" } }
                        if let Some(duration) = duration_label { span { class: "usage-metric", "{duration}" } }
                        if let Some(rate) = output_rate_label { span { class: "usage-metric", "{rate}" } }
                        if let Some(rate) = total_rate_label { span { class: "usage-metric", "{rate}" } }
                        if let Some(cost) = cost_label { span { class: "usage-metric usage-cost", "{cost}" } }
                    }
                    if let Some(s) = summary { div { class: "summary", "{s}" } }
                }
            }
        }
        ClaudeEvent::ContentBlockDelta { .. } => rsx! {
            div { class: "ev-system",
                div { class: "label", "STREAMING..." }
            }
        },
    }
}

pub fn format_token_count(tokens: u32) -> String {
    if tokens >= 1_000_000 {
        format_compact(tokens as f64 / 1_000_000.0, "M")
    } else if tokens >= 10_000 {
        format_compact(tokens as f64 / 1_000.0, "k")
    } else {
        tokens.to_string()
    }
}

pub fn format_tokens_per_second(tokens: u32, duration_ms: u64) -> Option<String> {
    if tokens == 0 || duration_ms == 0 {
        return None;
    }
    let rate = tokens as f64 * 1000.0 / duration_ms as f64;
    if rate >= 100.0 {
        Some(format!("{rate:.0}"))
    } else {
        Some(trim_float(format!("{rate:.1}")))
    }
}

pub fn format_duration_ms(duration_ms: u64) -> String {
    if duration_ms >= 60_000 {
        format!(
            "{}m {}s",
            duration_ms / 60_000,
            (duration_ms % 60_000) / 1000
        )
    } else if duration_ms >= 1000 {
        format!(
            "{}s",
            trim_float(format!("{:.1}", duration_ms as f64 / 1000.0))
        )
    } else {
        format!("{duration_ms}ms")
    }
}

pub fn format_cost_usd(cost: f64) -> String {
    if cost >= 1.0 {
        format!("${cost:.2} est")
    } else if cost >= 0.01 {
        format!("${cost:.4} est")
    } else {
        format!("${cost:.6} est")
    }
}

fn format_compact(value: f64, suffix: &str) -> String {
    format!("{}{}", trim_float(format!("{value:.1}")), suffix)
}

fn trim_float(value: String) -> String {
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// Generate a one-line summary for a tool use based on name + input params.
fn tool_use_summary(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Read" => {
            let path = input
                .get("file_path")
                .or(input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let basename = path.rsplit('/').next().unwrap_or(path);
            if let Some(limit) = input.get("limit").and_then(|v| v.as_u64()) {
                format!("{basename} ({limit} lines)")
            } else {
                basename.to_string()
            }
        }
        "Write" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            path.rsplit('/').next().unwrap_or(path).to_string()
        }
        "Edit" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            path.rsplit('/').next().unwrap_or(path).to_string()
        }
        "Bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let truncated: String = cmd.chars().take(60).collect();
            if cmd.len() > 60 {
                format!("{truncated}...")
            } else {
                truncated
            }
        }
        "Grep" | "Search" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            let truncated: String = pattern.chars().take(40).collect();
            if pattern.len() > 40 {
                format!("/{truncated}.../")
            } else {
                format!("/{truncated}/")
            }
        }
        "Glob" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            pattern.to_string()
        }
        "Agent" => {
            let desc = input
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("subagent");
            desc.to_string()
        }
        _ => {
            // Generic: show first string value found
            if let Some(obj) = input.as_object() {
                for val in obj.values() {
                    if let Some(s) = val.as_str() {
                        let truncated: String = s.chars().take(50).collect();
                        return if s.len() > 50 {
                            format!("{truncated}...")
                        } else {
                            truncated
                        };
                    }
                }
            }
            String::new()
        }
    }
}

/// Generate a one-line summary for a tool result based on content + tool name.
fn tool_result_summary(tool_name: &str, content: &str) -> (String, bool) {
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let is_error = content.contains("ERROR")
        || content.contains("Error:")
        || content.contains("FAILED")
        || content.contains("error[E")
        || content.contains("Permission denied")
        || content.contains("No such file");

    let summary = match tool_name {
        "Read" => format!("{line_count} lines"),
        "Edit" => {
            if is_error {
                "failed".to_string()
            } else {
                "applied".to_string()
            }
        }
        "Bash" => {
            let first = lines.first().map(|l| l.trim()).unwrap_or("");
            let truncated: String = first.chars().take(50).collect();
            if line_count > 1 {
                format!("{truncated}... ({line_count} lines)")
            } else {
                truncated
            }
        }
        "Grep" | "Glob" | "Search" => {
            format!("{line_count} lines")
        }
        "Write" => {
            if is_error {
                "failed".to_string()
            } else {
                "written".to_string()
            }
        }
        _ => {
            if line_count > 1 {
                format!("{line_count} lines")
            } else {
                let truncated: String = content.chars().take(50).collect();
                if content.len() > 50 {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            }
        }
    };

    (summary, is_error)
}

#[component]
pub fn ContentBlockRow(
    block: ContentBlock,
    expand_all: bool,
    tool_names: HashMap<String, String>,
) -> Element {
    match block {
        ContentBlock::Text { text } => rsx! {
            div { class: "block-text", "{text}" }
        },
        ContentBlock::Thinking { thinking } => rsx! {
            details { open: expand_all, class: "block-thinking",
                summary { "Thinking..." }
                div { class: "content", "{thinking}" }
            }
        },
        ContentBlock::ToolUse { id: _, name, input } => {
            let summary = tool_use_summary(&name, &input);
            rsx! {
                details { open: expand_all, class: "block-tool-use",
                    summary {
                        span { class: "tool-badge", "{name}" }
                        if !summary.is_empty() {
                            span { class: "tool-target", "{summary}" }
                        }
                    }
                    pre { "{serde_json::to_string_pretty(&input).unwrap_or_default()}" }
                }
            }
        }
        ContentBlock::ToolResult { id, content } => {
            let tool_name = tool_names.get(&id).map(|s| s.as_str()).unwrap_or("");
            let (summary, is_error) = tool_result_summary(tool_name, &content);
            let badge_class = if is_error {
                "result-badge result-badge-err"
            } else {
                "result-badge result-badge-ok"
            };
            let badge_text = if !tool_name.is_empty() {
                tool_name.to_string()
            } else {
                "Result".to_string()
            };
            rsx! {
                details { open: expand_all, class: "block-tool-result",
                    summary {
                        span { class: "{badge_class}", "{badge_text}" }
                        span { class: "result-meta", "{summary}" }
                    }
                    pre { "{content}" }
                }
            }
        }
    }
}
