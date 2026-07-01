// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::types::{AgentEvent, EVENT_SENDER};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_CHILD_PID: OnceLock<Mutex<Option<u32>>> = OnceLock::new();

pub fn active_child_pid_slot() -> &'static Mutex<Option<u32>> {
    ACTIVE_CHILD_PID.get_or_init(|| Mutex::new(None))
}

pub fn set_active_child_pid(pid: Option<u32>) {
    if let Ok(mut slot) = active_child_pid_slot().lock() {
        *slot = pid;
    }
}

pub fn active_child_pid() -> Option<u32> {
    active_child_pid_slot().lock().ok().and_then(|slot| *slot)
}

pub fn clear_stop_request() {
    STOP_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn stop_requested() -> bool {
    STOP_REQUESTED.load(Ordering::SeqCst)
}

pub fn request_stop() {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
    if let Some(pid) = active_child_pid() {
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .status();
    }
}

pub fn emit_event(ev: AgentEvent) {
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(ev);
    }
}
