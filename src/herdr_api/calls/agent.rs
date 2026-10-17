//! Agent calls: `agent.list`, `agent.get`, `agent.focus`, `agent.prompt`
//! and `agent.start`.

use std::time::Duration;

use serde_json::{Value, json};

use super::call::Call;
use super::reply::{agent_info, echo, list, s, unit};
use crate::herdr_api::error::HerdrError;
use crate::herdr_api::ids::PaneId;
use crate::herdr_api::sanitize::Sanitized;
use crate::herdr_api::types::AgentInfo;
use crate::herdr_api::wire;

/// The error code herdr gives when a target has no agent.
pub const AGENT_NOT_FOUND: &str = "agent_not_found";

/// `agent.get`: no agent there is `None`, not an error (herdr-inbox
/// `live_agent`, panorama `agent_exists`).
fn maybe_agent(r: Result<Value, HerdrError>) -> Result<Option<AgentInfo>, HerdrError> {
    match r {
        Err(e) if e.is_code(AGENT_NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
        Ok(mut v) => match v.get_mut("agent").map(Value::take) {
            Some(a) if wire::truthy(&a) => serde_json::from_value(a)
                .map(Some)
                .map_err(|e| HerdrError::Garbled(format!("`agent`: {e}"))),
            _ => Ok(None),
        },
    }
}

/// `agent.list` (herdr-inbox alerts).
pub fn agents() -> Call<Vec<AgentInfo>> {
    Call::new("agent.list", json!({}), |r| Ok(list(r?, "agents")))
}

/// `agent.get` of the agent in a pane; `None` when there is none
/// (herdr-inbox `live_agent`).
pub fn agent(pane: &PaneId) -> Call<Option<AgentInfo>> {
    Call::new(
        "agent.get",
        json!({"target": s(pane.as_str())}),
        maybe_agent,
    )
}

/// `agent.get` by the name given at `agent start` (panorama `agent_exists`).
pub fn agent_named(name: &str) -> Call<Option<AgentInfo>> {
    Call::new("agent.get", json!({"target": s(name)}), maybe_agent)
}

/// `agent.focus` (both plugins' `focus_agent`).
pub fn focus_agent(pane: &PaneId) -> Call<Option<AgentInfo>> {
    Call::new("agent.focus", json!({"target": s(pane.as_str())}), |r| {
        echo(r, "agent")
    })
}

/// `agent.prompt`: type `text` into the agent as one prompt, without waiting
/// (both plugins' `prompt_agent`).
pub fn prompt(pane: &PaneId, text: &Sanitized) -> Call<()> {
    Call::new(
        "agent.prompt",
        json!({"target": s(pane.as_str()), "text": s(text.as_str())}),
        unit,
    )
}

/// `agent.start` alone: herdr answers at once, with `launch_pending` set.
/// The clients' `start_agent` methods also wait for the agent to be ready,
/// as herdr's CLI does; use those. The call's own timeout is `timeout_ms` +
/// 5 s (panorama `start_agent`).
pub fn start_agent(
    name: &Sanitized,
    kind: &str,
    pane: &PaneId,
    timeout_ms: u32,
) -> Call<AgentInfo> {
    let mut call = Call::new(
        "agent.start",
        json!({"name": s(name.as_str()), "kind": s(kind), "pane_id": s(pane.as_str()),
               "timeout_ms": timeout_ms}),
        agent_info,
    );
    call.timeout = Some(Duration::from_millis(u64::from(timeout_ms)) + Duration::from_secs(5));
    call
}

/// `start_agent` gave up waiting for the agent to be ready.
pub(crate) fn not_ready(timeout_ms: u32) -> HerdrError {
    HerdrError::NoReply(format!("the agent was not ready within {timeout_ms} ms"))
}
