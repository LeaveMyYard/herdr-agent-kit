//! The session's structure: workspaces, tabs, panes, their layouts, and
//! the [`Snapshot`] of them all (`session.snapshot`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Extra;
use super::agent::{AgentInfo, AgentSession, AgentStatus};
use super::lenient::{lenient, lenient_vec};
use crate::herdr_api::ids::{PaneId, TabId, WorkspaceId};

/// A pane (`pane.get`, `pane.list`, snapshot `panes`, a plugin pane).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaneInfo {
    /// The pane.
    #[serde(default, deserialize_with = "lenient")]
    pub pane_id: Option<PaneId>,
    /// Its tab.
    #[serde(default, deserialize_with = "lenient")]
    pub tab_id: Option<TabId>,
    /// Its workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub workspace_id: Option<WorkspaceId>,
    /// Whether it has the focus.
    #[serde(default, deserialize_with = "lenient")]
    pub focused: bool,
    /// Its label, if it has one.
    #[serde(default, deserialize_with = "lenient")]
    pub label: Option<String>,
    /// The agent kind running in it, if any.
    #[serde(default, deserialize_with = "lenient")]
    pub agent: Option<String>,
    /// The agent's state.
    #[serde(default, deserialize_with = "lenient")]
    pub agent_status: AgentStatus,
    /// The session its agent reported.
    #[serde(default, deserialize_with = "lenient")]
    pub agent_session: Option<AgentSession>,
    /// Its metadata tokens.
    #[serde(default, deserialize_with = "lenient")]
    pub tokens: Map<String, Value>,
    /// Everything else (`cwd`, `terminal_id`, `scroll`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

/// A tab (`tab.get`, `tab.list`, `tab.create`, snapshot `tabs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TabInfo {
    /// The tab.
    #[serde(default, deserialize_with = "lenient")]
    pub tab_id: Option<TabId>,
    /// Its workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub workspace_id: Option<WorkspaceId>,
    /// Its label.
    #[serde(default, deserialize_with = "lenient")]
    pub label: Option<String>,
    /// Whether it has the focus.
    #[serde(default, deserialize_with = "lenient")]
    pub focused: bool,
    /// Everything else (`number`, `pane_count`, `agent_status`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

/// A workspace (`workspace.list`, snapshot `workspaces`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorkspaceInfo {
    /// The workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub workspace_id: Option<WorkspaceId>,
    /// Its label.
    #[serde(default, deserialize_with = "lenient")]
    pub label: Option<String>,
    /// Whether it has the focus.
    #[serde(default, deserialize_with = "lenient")]
    pub focused: bool,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// `session.snapshot`: the whole session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Snapshot {
    /// herdr's version.
    #[serde(default, deserialize_with = "lenient")]
    pub version: Option<String>,
    /// The socket protocol.
    #[serde(default, deserialize_with = "lenient")]
    pub protocol: Option<u64>,
    /// The focused workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub focused_workspace_id: Option<WorkspaceId>,
    /// The focused tab.
    #[serde(default, deserialize_with = "lenient")]
    pub focused_tab_id: Option<TabId>,
    /// The focused pane.
    #[serde(default, deserialize_with = "lenient")]
    pub focused_pane_id: Option<PaneId>,
    /// Every workspace.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub workspaces: Vec<WorkspaceInfo>,
    /// Every tab.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub tabs: Vec<TabInfo>,
    /// Every pane.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub panes: Vec<PaneInfo>,
    /// Every tab's layout, as herdr sent it.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub layouts: Vec<Value>,
    /// Every agent.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub agents: Vec<AgentInfo>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A tab's layout (`layout.export`, `layout.apply`, `layout.set_split_ratio`,
/// `pane.layout`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LayoutInfo {
    /// The tab.
    #[serde(default, deserialize_with = "lenient")]
    pub tab_id: Option<TabId>,
    /// Its workspace.
    #[serde(default, deserialize_with = "lenient")]
    pub workspace_id: Option<WorkspaceId>,
    /// The tab's focused pane.
    #[serde(default, deserialize_with = "lenient")]
    pub focused_pane_id: Option<PaneId>,
    /// The split tree (`layout.export`, `layout.apply`), as herdr sent it.
    #[serde(default)]
    pub root: Option<Value>,
    /// Everything else (`zoomed`, `area`, `panes`, `splits`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}
