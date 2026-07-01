// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::tracker::TrackerInfo;
use crate::agent::types::{AgentEvent, Config, RichAction, should_use_event_model};
use crate::ui::components::{format_cost_usd, format_token_count, format_tokens_per_second};
use dioxus::prelude::*;

#[derive(Default)]
struct UsageSummary {
    input_tokens: u32,
    output_tokens: u32,
    last_output_tokens_per_second: Option<String>,
    estimated_cost_usd: f64,
}

fn summarize_usage(events: &[AgentEvent], config: &Config) -> UsageSummary {
    let mut summary = UsageSummary::default();
    let mut active_model = config.pricing_model_key();
    let has_configured_model = active_model.is_some();

    for event in events {
        match event {
            AgentEvent::Rich(RichAction::System {
                model: Some(model), ..
            }) if should_use_event_model(model, has_configured_model) => {
                active_model = Some(model.trim().to_string());
            }
            AgentEvent::Rich(RichAction::Result {
                duration_ms,
                input_tokens,
                output_tokens,
                ..
            }) => {
                let input_tokens = input_tokens.unwrap_or(0);
                let output_tokens = output_tokens.unwrap_or(0);

                summary.input_tokens = summary.input_tokens.saturating_add(input_tokens);
                summary.output_tokens = summary.output_tokens.saturating_add(output_tokens);
                if let Some(rate) =
                    duration_ms.and_then(|ms| format_tokens_per_second(output_tokens, ms))
                {
                    summary.last_output_tokens_per_second = Some(rate);
                }
                if let Some(model) = &active_model
                    && let Some(cost) =
                        config
                            .pricing
                            .estimate_cost_usd(model, input_tokens, output_tokens)
                {
                    summary.estimated_cost_usd += cost;
                }
            }
            _ => {}
        }
    }

    summary
}

#[component]
pub fn Statusbar(
    config: Signal<Config>,
    tracker_ids: Signal<Vec<TrackerInfo>>,
    issues: Signal<Vec<crate::agent::tracker::PendingIssue>>,
    events: Signal<Vec<crate::agent::types::AgentEvent>>,
    is_working: Signal<bool>,
    theme_name: String,
) -> Element {
    let working = *is_working.read();
    let issue_count = issues.read().len();
    let event_count = events.read().len();
    let usage = summarize_usage(&events.read(), &config.read());
    let total_tokens = usage.input_tokens.saturating_add(usage.output_tokens);
    let total_token_label = format_token_count(total_tokens);
    let estimated_cost_label =
        (usage.estimated_cost_usd > 0.0).then(|| format_cost_usd(usage.estimated_cost_usd));
    let tracker_nums = tracker_ids
        .read()
        .iter()
        .map(|t| format!("#{}", t.number))
        .collect::<Vec<_>>()
        .join(", ");

    rsx! {
        div { class: "statusbar",
            div { class: "statusbar-left",
                span { class: if working { "status-dot status-dot-active" } else { "status-dot status-dot-idle" } }
                span { if working { "Working" } else { "Ready" } }
                if !tracker_nums.is_empty() {
                    span { class: "status-sep", "|" }
                    span { "{tracker_nums}" }
                }
            }
            div { class: "statusbar-right",
                span { "{issue_count} issues" }
                span { class: "status-sep", "|" }
                span { "{event_count} events" }
                span { class: "status-sep", "|" }
                if total_tokens > 0 {
                    span { "{total_token_label} tok" }
                    span { class: "status-sep", "|" }
                }
                if let Some(rate) = usage.last_output_tokens_per_second {
                    span { "{rate} out tok/s" }
                    span { class: "status-sep", "|" }
                }
                if let Some(cost) = estimated_cost_label {
                    span { "{cost}" }
                    span { class: "status-sep", "|" }
                }
                span { "{config.read().agent}" }
                span { class: "status-sep", "|" }
                span { "{theme_name}" }
            }
        }
    }
}
