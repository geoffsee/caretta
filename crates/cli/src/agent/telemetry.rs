// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Anonymous telemetry collection for usage analytics and IP protection.
//!
//! This module integrates with g-telemetry (https://github.com/geoffsee/g-telemetry)
//! to collect anonymous usage data that helps understand how the application is used
//! while protecting user privacy and intellectual property.
//!
//! The telemetry system is designed to be:
//! - **Anonymous by Design**: No PII, no IP logging, random instance IDs
//! - **Privacy First**: Respects `DO_NOT_TRACK=1` and app-specific opt-outs
//! - **Minimal Impact**: Events are buffered and sent in the background
//!
//! ## Opt-out Mechanisms
//!
//! Users can opt-out of telemetry by:
//! 1. Setting the environment variable `DO_NOT_TRACK=1`
//! 2. Setting `CARETTA_NO_TELEMETRY=1`
//! 3. Disabling telemetry in the configuration

use anon_telemetry::TelemetryClient;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Global telemetry client instance
static TELEMETRY_CLIENT: OnceCell<Arc<TelemetryClient>> = OnceCell::new();

/// Event names for standard telemetry events
pub struct EventNames;

impl EventNames {
    pub const APP_START: &str = "app_start";
    pub const APP_EXIT: &str = "app_exit";
    pub const COMMAND_EXECUTION: &str = "command_execution";
    pub const WORKFLOW_START: &str = "workflow_start";
    pub const WORKFLOW_COMPLETE: &str = "workflow_complete";
    pub const AGENT_INVOCATION: &str = "agent_invocation";
    pub const ERROR: &str = "error";
    pub const UI_LAUNCH: &str = "ui_launch";
    pub const CONFIG_LOAD: &str = "config_load";
}

/// Initialize the telemetry client with the given configuration
pub async fn initialize_telemetry(config: &crate::cli_common::TelemetryConfig) {
    if !config.enabled {
        return;
    }

    let client = TelemetryClient::new(&config.app_id, &config.endpoint).await;
    TELEMETRY_CLIENT.set(client).ok();
}

/// Initialize telemetry with default configuration
pub async fn initialize_telemetry_default() {
    let config = crate::cli_common::TelemetryConfig::default();
    initialize_telemetry(&config).await;
}

/// Get the global telemetry client, if available
pub fn get_telemetry_client() -> Option<&'static Arc<TelemetryClient>> {
    TELEMETRY_CLIENT.get()
}

/// Track a telemetry event with the given name and optional properties
pub fn track_event(event_name: &str, properties: Option<HashMap<String, serde_json::Value>>) {
    if let Some(client) = get_telemetry_client() {
        client.track(event_name, properties);
    }
}

/// Track a simple event with just a name
pub fn track_simple_event(event_name: &str) {
    track_event(event_name, None);
}

/// Track an event with a single property
pub fn track_event_with_property(event_name: &str, key: &str, value: serde_json::Value) {
    let mut properties = HashMap::new();
    properties.insert(key.to_string(), value);
    track_event(event_name, Some(properties));
}

/// Track an event with multiple properties using a builder pattern
pub struct EventBuilder {
    event_name: String,
    properties: HashMap<String, serde_json::Value>,
}

impl EventBuilder {
    /// Create a new event builder
    pub fn new(event_name: &str) -> Self {
        Self {
            event_name: event_name.to_string(),
            properties: HashMap::new(),
        }
    }

    /// Add a string property
    pub fn with_string(mut self, key: &str, value: &str) -> Self {
        self.properties.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        self
    }

    /// Add a boolean property
    pub fn with_bool(mut self, key: &str, value: bool) -> Self {
        self.properties
            .insert(key.to_string(), serde_json::Value::Bool(value));
        self
    }

    /// Add a numeric property
    pub fn with_number(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.properties.insert(key.to_string(), value.into());
        self
    }

    /// Add a property
    pub fn with_property(mut self, key: &str, value: serde_json::Value) -> Self {
        self.properties.insert(key.to_string(), value);
        self
    }

    /// Send the event
    pub fn send(self) {
        track_event(&self.event_name, Some(self.properties));
    }
}

/// Macro for convenient event tracking
#[macro_export]
macro_rules! telemetry {
    // Simple event
    ($event:expr) => {
        $crate::agent::telemetry::track_simple_event($event)
    };
    // Event with single property
    ($event:expr, $key:expr => $value:expr) => {
        $crate::agent::telemetry::track_event_with_property($event, $key, serde_json::Value::String($value.to_string()))
    };
    // Event with multiple properties
    ($event:expr, { $($key:expr => $value:expr),* $(,)? }) => {
        {
            let mut properties = std::collections::HashMap::new();
            $(properties.insert($key.to_string(), serde_json::Value::String($value.to_string()));)*
            $crate::agent::telemetry::track_event($event, Some(properties));
        }
    };
}

/// Record application start
pub fn record_app_start(version: &str, platform: &str) {
    let mut properties = HashMap::new();
    properties.insert(
        "version".to_string(),
        serde_json::Value::String(version.to_string()),
    );
    properties.insert(
        "platform".to_string(),
        serde_json::Value::String(platform.to_string()),
    );
    track_event(EventNames::APP_START, Some(properties));
}

/// Record application exit
pub fn record_app_exit() {
    track_simple_event(EventNames::APP_EXIT);
}

/// Record command execution
pub fn record_command_execution(command: &str, success: bool) {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    properties.insert("success".to_string(), serde_json::Value::Bool(success));
    track_event(EventNames::COMMAND_EXECUTION, Some(properties));
}

/// Record workflow start
pub fn record_workflow_start(workflow_name: &str, agent: &str) {
    let mut properties = HashMap::new();
    properties.insert(
        "workflow_name".to_string(),
        serde_json::Value::String(workflow_name.to_string()),
    );
    properties.insert(
        "agent".to_string(),
        serde_json::Value::String(agent.to_string()),
    );
    track_event(EventNames::WORKFLOW_START, Some(properties));
}

/// Record workflow completion
pub fn record_workflow_complete(workflow_name: &str, success: bool, duration_ms: u64) {
    let mut properties = HashMap::new();
    properties.insert(
        "workflow_name".to_string(),
        serde_json::Value::String(workflow_name.to_string()),
    );
    properties.insert("success".to_string(), serde_json::Value::Bool(success));
    properties.insert(
        "duration_ms".to_string(),
        serde_json::Value::Number(duration_ms.into()),
    );
    track_event(EventNames::WORKFLOW_COMPLETE, Some(properties));
}

/// Record agent invocation
pub fn record_agent_invocation(agent: &str, model: &str, action: &str) {
    let mut properties = HashMap::new();
    properties.insert(
        "agent".to_string(),
        serde_json::Value::String(agent.to_string()),
    );
    properties.insert(
        "model".to_string(),
        serde_json::Value::String(model.to_string()),
    );
    properties.insert(
        "action".to_string(),
        serde_json::Value::String(action.to_string()),
    );
    track_event(EventNames::AGENT_INVOCATION, Some(properties));
}

/// Record an error
pub fn record_error(error_type: &str, message: &str) {
    let mut properties = HashMap::new();
    properties.insert(
        "error_type".to_string(),
        serde_json::Value::String(error_type.to_string()),
    );
    properties.insert(
        "message".to_string(),
        serde_json::Value::String(message.to_string()),
    );
    track_event(EventNames::ERROR, Some(properties));
}

/// Record UI launch
pub fn record_ui_launch() {
    track_simple_event(EventNames::UI_LAUNCH);
}

/// Record configuration load
pub fn record_config_load(project_name: &str, workspace: Option<&str>) {
    let mut properties = HashMap::new();
    properties.insert(
        "project_name".to_string(),
        serde_json::Value::String(project_name.to_string()),
    );
    if let Some(ws) = workspace {
        properties.insert(
            "workspace".to_string(),
            serde_json::Value::String(ws.to_string()),
        );
    }
    track_event(EventNames::CONFIG_LOAD, Some(properties));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builder() {
        EventBuilder::new("test_event")
            .with_string("key1", "value1")
            .with_bool("key2", true)
            .with_number("key3", 42)
            .send();
        // This just tests that it compiles and doesn't panic
    }

    #[test]
    fn test_telemetry_config_defaults() {
        use crate::cli_common::TelemetryConfig;
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.app_id, "caretta");
        assert_eq!(config.endpoint, "https://telemetry.geoffsee.com/v1/events");
    }
}
