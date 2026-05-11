use crate::agent::types::{AgentEvent, AssistantMessage, ClaudeEvent, ContentBlock, EVENT_SENDER};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

// 512 B cap per tool-input arg to bound per-run memory growth from large Edit/Write inputs.
const CAPTURE_MAX_TOOL_INPUT_BYTES: usize = 512;

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
    {
        match &ev {
            AgentEvent::Claude(ClaudeEvent::System { .. })
            | AgentEvent::Claude(ClaudeEvent::Result { .. }) => {
                events.push(ev.clone());
            }
            AgentEvent::Claude(ClaudeEvent::Assistant { message }) => {
                // Truncate large ToolUse inputs before buffering to bound
                // per-run memory growth (large Edit/Write inputs can be tens of KB).
                let content = message
                    .content
                    .iter()
                    .map(|block| {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            let s = input.to_string();
                            if s.len() > CAPTURE_MAX_TOOL_INPUT_BYTES {
                                let mut end = CAPTURE_MAX_TOOL_INPUT_BYTES;
                                while !s.is_char_boundary(end) {
                                    end -= 1;
                                }
                                ContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: serde_json::Value::String(format!(
                                        "{}…[truncated]",
                                        &s[..end]
                                    )),
                                }
                            } else {
                                block.clone()
                            }
                        } else {
                            block.clone()
                        }
                    })
                    .collect();
                events.push(AgentEvent::Claude(ClaudeEvent::Assistant {
                    message: AssistantMessage { content },
                }));
            }
            _ => {}
        }
    }
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(ev);
    }
}
