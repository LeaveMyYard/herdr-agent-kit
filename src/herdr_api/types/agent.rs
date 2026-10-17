//! Agents: [`AgentInfo`] (`agent.get`, `agent.list`), its
//! [`AgentStatus`] and the [`AgentSession`] it reported.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Extra;
use super::lenient::lenient;
use crate::herdr_api::ids::{PaneId, TabId, WorkspaceId};

/// An agent's state as herdr reports it. Anything herdr adds later reads as
/// [`AgentStatus::Unknown`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AgentStatus {
    /// At its prompt, waiting for input.
    Idle,
    /// Working on a turn.
    Working,
    /// Waiting on the operator (a permission prompt).
    Blocked,
    /// Finished.
    Done,
    /// herdr does not know, or said something this crate does not know.
    #[default]
    #[serde(other)]
    Unknown,
}

/// `agent_session`: the session herdr was told the agent runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentSession {
    /// The kind of value (`id`); absent means `id`.
    #[serde(default, deserialize_with = "lenient")]
    pub kind: Option<String>,
    /// The value: for Claude Code, its session id, as reported (not validated).
    #[serde(default, deserialize_with = "lenient")]
    pub value: Option<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

impl AgentSession {
    /// The value whatever its kind, if non-empty: herdr-inbox `live_agent`.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref().filter(|v| !v.is_empty())
    }

    /// The value only when it is a session id (`kind` is `id` or absent):
    /// panorama `machines.session_ids`.
    pub fn id(&self) -> Option<&str> {
        match self.kind.as_deref() {
            None | Some("id") => self.value(),
            Some(_) => None,
        }
    }
}

/// An agent (`agent.get`, `agent.list`, snapshot `agents`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentInfo {
    /// Its pane.
    #[serde(default, deserialize_with = "lenient")]
    pub pane_id: Option<PaneId>,
    /// Its tab.
    #[serde(default, deserialize_with = "lenient")]
    pub tab_id: Option<TabId>,
    /// Its workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub workspace_id: Option<WorkspaceId>,
    /// Its kind (`claude`).
    #[serde(default, deserialize_with = "lenient")]
    pub agent: Option<String>,
    /// The name given at `agent start`.
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// Its state.
    #[serde(default, deserialize_with = "lenient")]
    pub agent_status: AgentStatus,
    /// The session it reported.
    #[serde(default, deserialize_with = "lenient")]
    pub agent_session: Option<AgentSession>,
    /// Whether its pane has the focus.
    #[serde(default, deserialize_with = "lenient")]
    pub focused: bool,
    /// Its metadata tokens (`inbox_q`).
    #[serde(default, deserialize_with = "lenient")]
    pub tokens: Map<String, Value>,
    /// Everything else (`cwd`, `terminal_id`, `revision`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

impl AgentInfo {
    /// Ready for a prompt: herdr says `interactive_ready`, or no longer says
    /// `launch_pending` (`agent.start` answers with `launch_pending: true`;
    /// the agent is ready once `agent.get` drops it for `interactive_ready`).
    pub fn is_ready(&self) -> bool {
        let flag = |key: &str| self.extra.get(key).and_then(Value::as_bool);
        flag("interactive_ready") == Some(true) || flag("launch_pending") != Some(true)
    }

    /// The reported session value, whatever its kind (herdr-inbox).
    pub fn session_value(&self) -> Option<&str> {
        self.agent_session.as_ref().and_then(AgentSession::value)
    }
}
