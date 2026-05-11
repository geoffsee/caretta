use crate::agent::types::{AgentEvent, ClaudeEvent, EVENT_SENDER};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_CHILD_PID: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
static RUN_EVENT_CAPTURE: OnceLock<Mutex<Option<Vec<AgentEvent>>>> = OnceLock::new();

fn run_event_capture_slot() -> &'static Mutex<Option<Vec<AgentEvent>>> {
    RUN_EVENT_CAPTURE.get_or_init(|| Mutex::new(None))
}

/// Begin collecting all emitted events into an in-memory buffer.
/// Call [`drain_run_capture`] after the agent run to retrieve them.
///
/// # Invariant
/// Only one capture may be active at a time per process — this module uses a
/// single global slot. Calling `start_run_capture` while a prior capture is
/// active silently discards the buffered events from the earlier capture.
pub fn start_run_capture() {
    if let Ok(mut capture) = run_event_capture_slot().lock() {
        *capture = Some(Vec::new());
    }
}

/// Stop collecting events and return whatever was accumulated since the last
/// [`start_run_capture`] call. Returns an empty Vec if no capture was active.
pub fn drain_run_capture() -> Vec<AgentEvent> {
    if let Ok(mut capture) = run_event_capture_slot().lock() {
        capture.take().unwrap_or_default()
    } else {
        Vec::new()
    }
}

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
    if let Ok(mut capture) = run_event_capture_slot().lock()
        && let Some(events) = capture.as_mut()
        && matches!(
            &ev,
            AgentEvent::Claude(ClaudeEvent::System { .. })
                | AgentEvent::Claude(ClaudeEvent::Assistant { .. })
                | AgentEvent::Claude(ClaudeEvent::Result { .. })
        )
    {
        events.push(ev.clone());
    }
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(ev);
    }
}
