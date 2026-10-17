//! What herdr answers, typed. Lenient on purpose, like the Python clients
//! that read these with `.get()`: a missing or mistyped field becomes its
//! default (or `None`), a list item that is not an object is skipped, and
//! every field not named here is kept in `extra`.

use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::ids::{PaneId, TabId, WorkspaceId};

/// Fields this crate does not name, as herdr sent them.
pub type Extra = Map<String, Value>;

/// Any value that does not fit `T` becomes `T::default()`.
fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

/// A list whose items that do not fit `T` are skipped; anything but a list
/// is empty.
fn lenient_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| T::deserialize(v).ok())
            .collect(),
        _ => Vec::new(),
    })
}

/// An `Option<Vec<T>>` that keeps "absent" (`None`) apart from "empty".
fn lenient_opt_vec<'de, D, T>(d: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => Some(
            items
                .into_iter()
                .filter_map(|v| T::deserialize(v).ok())
                .collect(),
        ),
        _ => None,
    })
}

/// argv as panorama reads it: strings and integers, as strings; anything
/// else dropped (`[str(a) for a in argv if isinstance(a, (str, int))]`).
fn argv<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s),
                Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

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

/// One foreground process of a pane.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ForegroundProc {
    /// Its pid.
    #[serde(default, deserialize_with = "lenient")]
    pub pid: Option<i64>,
    /// Its process name.
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// Its argv (strings and integers, as strings).
    #[serde(default, deserialize_with = "argv")]
    pub argv: Vec<String>,
    /// Everything else (`cmdline`, `cwd`).
    #[serde(flatten)]
    pub extra: Extra,
}

/// `pane.process_info`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProcessInfo {
    /// The pane.
    #[serde(default, deserialize_with = "lenient")]
    pub pane_id: Option<PaneId>,
    /// The pane's shell.
    #[serde(default, deserialize_with = "lenient")]
    pub shell_pid: Option<i64>,
    /// The foreground processes. `None` when herdr gave no list (mid-exec or
    /// mid-restore: unknown, not "nothing").
    #[serde(default, deserialize_with = "lenient_opt_vec")]
    pub foreground_processes: Option<Vec<ForegroundProc>>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// `pane.move`'s `move_result`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MoveResult {
    /// Whether anything moved.
    #[serde(default, deserialize_with = "lenient")]
    pub changed: bool,
    /// The pane's id before the move.
    #[serde(default, deserialize_with = "lenient")]
    pub previous_pane_id: Option<PaneId>,
    /// The pane after the move: its `pane_id` is the pane's id from now on.
    #[serde(default, deserialize_with = "lenient")]
    pub pane: Option<PaneInfo>,
    /// Everything else (`target_layout`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

/// `pane.swap`'s `swap`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SwapResult {
    /// Whether the panes were swapped. herdr says `false` with a `reason`
    /// (`cross_tab`) instead of an error.
    #[serde(default, deserialize_with = "lenient")]
    pub changed: Option<bool>,
    /// Why not.
    #[serde(default, deserialize_with = "lenient")]
    pub reason: Option<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

impl SwapResult {
    /// panorama `swap_panes`: swapped unless herdr said `changed: false`.
    pub fn swapped(&self) -> bool {
        self.changed.unwrap_or(true)
    }
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

/// A plugin as `plugin.list` lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PluginInfo {
    /// Its id (`leavemyyard.herdr-inbox`).
    #[serde(default, deserialize_with = "lenient")]
    pub plugin_id: Option<String>,
    /// Its version.
    #[serde(default, deserialize_with = "lenient")]
    pub version: Option<String>,
    /// Where herdr runs it from.
    #[serde(default, deserialize_with = "lenient")]
    pub plugin_root: Option<String>,
    /// Whether it is enabled.
    #[serde(default, deserialize_with = "lenient")]
    pub enabled: bool,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// `notification.show`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NotificationResult {
    /// Whether it was shown.
    #[serde(default, deserialize_with = "lenient")]
    pub shown: Option<bool>,
    /// Why not (`disabled`).
    #[serde(default, deserialize_with = "lenient")]
    pub reason: Option<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A saved SSH machine (`herdr machine list --json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Machine {
    /// Its stable id.
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    /// Its label, for display.
    #[serde(default, deserialize_with = "lenient")]
    pub label: Option<String>,
    /// Its ssh target, unchecked: whether it is safe to pass to ssh is the
    /// caller's rule (panorama `safe_target`, herdr-inbox `SAFE_TARGET`).
    #[serde(default, deserialize_with = "lenient")]
    pub target: Option<String>,
    /// Whether it is enabled.
    #[serde(default, deserialize_with = "lenient")]
    pub enabled: bool,
    /// Everything else (`session`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_statuses_and_mistyped_fields_fall_back() {
        let a: AgentInfo = serde_json::from_value(json!({
            "pane_id": "w1:p2", "agent_status": "napping", "focused": null,
            "agent_session": {"value": "abc"}, "revision": 3
        }))
        .unwrap();
        assert_eq!(a.agent_status, AgentStatus::Unknown);
        assert!(!a.focused);
        assert_eq!(a.session_value(), Some("abc"));
        assert_eq!(a.extra["revision"], 3);
    }

    #[test]
    fn a_session_of_another_kind_is_not_an_id() {
        let s: AgentSession =
            serde_json::from_value(json!({"kind": "name", "value": "x"})).unwrap();
        assert_eq!(s.id(), None);
        assert_eq!(s.value(), Some("x"));
    }

    #[test]
    fn bad_list_items_are_skipped_and_argv_keeps_strings_and_ints() {
        let s: Snapshot =
            serde_json::from_value(json!({"agents": [1, {"agent": "claude"}], "panes": "x"}))
                .unwrap();
        assert_eq!(s.agents.len(), 1);
        assert!(s.panes.is_empty());
        let p: ProcessInfo = serde_json::from_value(json!({"foreground_processes": [
            {"pid": 7, "argv": ["claude", 3, null, 1.5]}]}))
        .unwrap();
        assert_eq!(p.foreground_processes.unwrap()[0].argv, ["claude", "3"]);
        let p: ProcessInfo = serde_json::from_value(json!({})).unwrap();
        assert_eq!(p.foreground_processes, None);
    }

    #[test]
    fn a_swap_is_done_unless_herdr_says_otherwise() {
        assert!(SwapResult::default().swapped());
        let s: SwapResult =
            serde_json::from_value(json!({"changed": false, "reason": "cross_tab"})).unwrap();
        assert!(!s.swapped());
    }
}
