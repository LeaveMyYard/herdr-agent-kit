//! One [`Call`] per socket method the plugins use: the method, its params in
//! the exact shape herdr's own CLI sends (checked against the recordings in
//! `fixtures/herdr`), and how to read the reply.
//!
//! A `Call` is plain data; [`Herdr`](super::Herdr) (async) and
//! [`blocking::Herdr`](super::blocking::Herdr) send it. Each has a method of
//! the same name for every constructor here.
//!
//! **One choke point.** Every string that goes into params passes through
//! [`clean`] (as every argument of the Python CLI path did), so no escape or
//! key sequence reaches herdr whatever the caller passed. Free text (prompts,
//! labels, titles) must already be [`Sanitized`]: that is the type rule, and
//! the choke point is the backstop. The one exception is
//! [`apply_layout`]'s `root` tree, passed as given (panorama's
//! `apply_panel_layout` sent it raw too).

use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::error::HerdrError;
use super::ids::{PaneId, TabId, WorkspaceId};
use super::sanitize::{MAX_ARG, Sanitized, clean};
use super::types::{
    AgentInfo, Extra, LayoutInfo, MoveResult, NotificationResult, PaneInfo, PluginInfo,
    ProcessInfo, Snapshot, SwapResult, TabInfo, WorkspaceInfo,
};

/// The error code herdr gives when a target has no agent.
pub const AGENT_NOT_FOUND: &str = "agent_not_found";

type Parse<T> = fn(Result<Value, HerdrError>) -> Result<T, HerdrError>;

/// One socket request and how to read its reply.
#[must_use = "a Call does nothing until a client sends it"]
pub struct Call<T> {
    method: &'static str,
    params: Value,
    timeout: Option<Duration>,
    parse: Parse<T>,
}

impl<T> Clone for Call<T> {
    fn clone(&self) -> Self {
        Self {
            method: self.method,
            params: self.params.clone(),
            timeout: self.timeout,
            parse: self.parse,
        }
    }
}

impl<T> std::fmt::Debug for Call<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("method", &self.method)
            .field("params", &self.params)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl<T> Call<T> {
    fn new(method: &'static str, params: Value, parse: Parse<T>) -> Self {
        Self {
            method,
            params,
            timeout: None,
            parse,
        }
    }

    /// The socket method.
    pub fn method(&self) -> &'static str {
        self.method
    }

    /// The params, as they go on the wire.
    pub fn params(&self) -> &Value {
        &self.params
    }

    /// This call's own timeout, when it needs longer than the client's.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Read a reply (or the transport's error) as this call's result.
    pub fn parse(&self, reply: Result<Value, HerdrError>) -> Result<T, HerdrError> {
        (self.parse)(reply)
    }
}

// -- params ------------------------------------------------------------------

/// A string param through the choke point.
fn s(text: &str) -> Value {
    Value::String(clean(text, MAX_ARG))
}

fn path(p: &Path) -> Value {
    s(&p.to_string_lossy())
}

// -- reading replies ---------------------------------------------------------

fn field<T: DeserializeOwned>(mut result: Value, key: &str) -> Result<T, HerdrError> {
    match result.get_mut(key).map(Value::take) {
        Some(v) if !v.is_null() => {
            serde_json::from_value(v).map_err(|e| HerdrError::Garbled(format!("`{key}`: {e}")))
        }
        _ => Err(HerdrError::Garbled(format!("no `{key}` in the reply"))),
    }
}

/// A list field; missing reads as empty (Python's `res.get(key, [])`), items
/// that do not fit are skipped.
fn list<T: DeserializeOwned>(mut result: Value, key: &str) -> Vec<T> {
    match result.get_mut(key).map(Value::take) {
        Some(Value::Array(items)) => items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn unit(r: Result<Value, HerdrError>) -> Result<(), HerdrError> {
    r.map(drop)
}

fn raw(r: Result<Value, HerdrError>) -> Result<Value, HerdrError> {
    r
}

fn extra(r: Result<Value, HerdrError>) -> Result<Extra, HerdrError> {
    match r? {
        Value::Object(m) => Ok(m),
        _ => Ok(Map::new()),
    }
}

/// What herdr echoes back for an action it did: `None` when the echo is
/// missing or does not fit. herdr confirmed the action either way (no
/// `error`), so this is never [`HerdrError::Garbled`] (which reads as "maybe
/// done"); the Python callers only checked for an error here.
fn echo<T: DeserializeOwned>(
    r: Result<Value, HerdrError>,
    key: &str,
) -> Result<Option<T>, HerdrError> {
    Ok(field(r?, key).ok())
}

fn tab_info(r: Result<Value, HerdrError>) -> Result<TabInfo, HerdrError> {
    field(r?, "tab")
}

fn pane_info(r: Result<Value, HerdrError>) -> Result<PaneInfo, HerdrError> {
    field(r?, "pane")
}

fn agent_info(r: Result<Value, HerdrError>) -> Result<AgentInfo, HerdrError> {
    field(r?, "agent")
}

fn layout(r: Result<Value, HerdrError>) -> Result<LayoutInfo, HerdrError> {
    field(r?, "layout")
}

/// `agent.get`: no agent there is `None`, not an error (herdr-inbox
/// `live_agent`, panorama `agent_exists`).
fn maybe_agent(r: Result<Value, HerdrError>) -> Result<Option<AgentInfo>, HerdrError> {
    match r {
        Err(e) if e.is_code(AGENT_NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
        Ok(mut v) => match v.get_mut("agent").map(Value::take) {
            Some(a) if super::wire::truthy(&a) => serde_json::from_value(a)
                .map(Some)
                .map_err(|e| HerdrError::Garbled(format!("`agent`: {e}"))),
            _ => Ok(None),
        },
    }
}

// -- the calls ---------------------------------------------------------------

/// `session.snapshot`: the whole session (panorama `snapshot`).
pub fn snapshot() -> Call<Snapshot> {
    Call::new("session.snapshot", json!({}), |r| field(r?, "snapshot"))
}

/// `workspace.list` (herdr-inbox `all_tabs`).
pub fn workspaces() -> Call<Vec<WorkspaceInfo>> {
    Call::new("workspace.list", json!({}), |r| Ok(list(r?, "workspaces")))
}

/// `tab.list` of one workspace (herdr-inbox `all_tabs`).
pub fn tabs(workspace: &WorkspaceId) -> Call<Vec<TabInfo>> {
    Call::new(
        "tab.list",
        json!({"workspace_id": s(workspace.as_str())}),
        |r| Ok(list(r?, "tabs")),
    )
}

/// `tab.get` (herdr-inbox `tab_label`).
pub fn tab(tab: &TabId) -> Call<TabInfo> {
    Call::new("tab.get", json!({"tab_id": s(tab.as_str())}), tab_info)
}

/// `tab.create`. Without `workspace` herdr picks one (panorama `create_tab`).
pub fn create_tab(
    label: &Sanitized,
    cwd: Option<&Path>,
    focus: bool,
    workspace: Option<&WorkspaceId>,
) -> Call<TabInfo> {
    let mut params = json!({"label": s(label.as_str()), "focus": focus});
    if let Some(cwd) = cwd {
        params["cwd"] = path(cwd);
    }
    if let Some(ws) = workspace {
        params["workspace_id"] = s(ws.as_str());
    }
    Call::new("tab.create", params, tab_info)
}

/// `tab.rename` (both plugins' `rename_tab`).
pub fn rename_tab(tab: &TabId, label: &Sanitized) -> Call<Option<TabInfo>> {
    Call::new(
        "tab.rename",
        json!({"tab_id": s(tab.as_str()), "label": s(label.as_str())}),
        |r| echo(r, "tab"),
    )
}

/// `tab.focus` (panorama `focus_tab`).
pub fn focus_tab(tab: &TabId) -> Call<Option<TabInfo>> {
    Call::new("tab.focus", json!({"tab_id": s(tab.as_str())}), |r| {
        echo(r, "tab")
    })
}

/// `tab.move` to `insert_index`; answers the workspace's tabs (panorama
/// `move_tab`).
pub fn move_tab(tab: &TabId, insert_index: u32) -> Call<Vec<TabInfo>> {
    Call::new(
        "tab.move",
        json!({"tab_id": s(tab.as_str()), "insert_index": insert_index}),
        |r| Ok(list(r?, "tabs")),
    )
}

/// `tab.close` (panorama `close_tab`).
pub fn close_tab(tab: &TabId) -> Call<()> {
    Call::new("tab.close", json!({"tab_id": s(tab.as_str())}), unit)
}

/// `pane.list` (herdr-inbox `pane_tabs`, `focused_pane`; panorama
/// `tab_root_pane`).
pub fn panes() -> Call<Vec<PaneInfo>> {
    Call::new("pane.list", json!({}), |r| Ok(list(r?, "panes")))
}

/// `pane.get` (herdr-inbox `pane_identity`).
pub fn pane(pane: &PaneId) -> Call<PaneInfo> {
    Call::new("pane.get", json!({"pane_id": s(pane.as_str())}), pane_info)
}

/// `pane.focus` (panorama `focus_pane`).
pub fn focus_pane(pane: &PaneId) -> Call<Option<PaneInfo>> {
    Call::new("pane.focus", json!({"pane_id": s(pane.as_str())}), |r| {
        echo(r, "pane")
    })
}

/// `pane.rename` (panorama `rename_pane`).
pub fn rename_pane(pane: &PaneId, label: &Sanitized) -> Call<Option<PaneInfo>> {
    Call::new(
        "pane.rename",
        json!({"pane_id": s(pane.as_str()), "label": s(label.as_str())}),
        |r| echo(r, "pane"),
    )
}

/// `pane.zoom`, toggled (panorama `zoom_pane`). Answers herdr's `zoom` object
/// (empty when herdr echoed none).
pub fn toggle_zoom(pane: &PaneId) -> Call<Extra> {
    Call::new(
        "pane.zoom",
        json!({"pane_id": s(pane.as_str()), "mode": "toggle"}),
        |r| extra(Ok(echo::<Value>(r, "zoom")?.unwrap_or_default())),
    )
}

/// Python's universal newlines: `\r\n` and a lone `\r` read as `\n`.
pub fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `pane.read` of the visible screen: the last `lines` lines, plain text or
/// with ANSI styling (herdr-inbox `panel_focused`, `composer_state`).
///
/// Line ends are read as the Python clients read the CLI's output (text
/// mode, [`universal_newlines`]): herdr's ANSI format ends lines with
/// `\r\n`, which `composer_state` saw as `\n`.
pub fn read_pane(pane: &PaneId, lines: u16, ansi: bool) -> Call<String> {
    Call::new(
        "pane.read",
        json!({"pane_id": s(pane.as_str()), "source": "visible", "lines": lines,
               "format": if ansi { "ansi" } else { "text" }, "strip_ansi": true}),
        |r| {
            let read: Value = field(r?, "read")?;
            field::<String>(read, "text").map(|t| universal_newlines(&t))
        },
    )
}

/// `pane.process_info` (panorama `runs_panorama`, `claude_pid`).
pub fn process_info(pane: &PaneId) -> Call<ProcessInfo> {
    Call::new(
        "pane.process_info",
        json!({"pane_id": s(pane.as_str())}),
        |r| field(r?, "process_info"),
    )
}

/// Where [`move_pane`] puts a pane.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MoveTo {
    /// Into a tab, split right (of `target_pane` when given). panorama
    /// `move_pane_to_tab`.
    Tab {
        /// The tab.
        tab: TabId,
        /// The pane to split.
        target_pane: Option<PaneId>,
    },
    /// Into a new tab (panorama `move_pane_new_tab`).
    NewTab {
        /// The workspace, or herdr's choice.
        workspace: Option<WorkspaceId>,
        /// The new tab's label.
        label: Sanitized,
    },
}

/// `pane.move`. The answer's `pane.pane_id` is the pane's id from now on.
pub fn move_pane(pane: &PaneId, to: &MoveTo, focus: bool) -> Call<MoveResult> {
    let destination = match to {
        MoveTo::Tab { tab, target_pane } => {
            let mut d = json!({"type": "tab", "tab_id": s(tab.as_str()), "split": "right"});
            if let Some(t) = target_pane {
                d["target_pane_id"] = s(t.as_str());
            }
            d
        }
        MoveTo::NewTab { workspace, label } => {
            let mut d = json!({"type": "new_tab", "label": s(label.as_str())});
            if let Some(ws) = workspace {
                d["workspace_id"] = s(ws.as_str());
            }
            d
        }
    };
    Call::new(
        "pane.move",
        json!({"pane_id": s(pane.as_str()), "destination": destination, "focus": focus}),
        |r| field(r?, "move_result"),
    )
}

/// `pane.swap` of two panes of one tab (panorama `swap_panes`). Across tabs
/// herdr does not refuse: it answers `changed: false` (see
/// [`SwapResult::swapped`]).
pub fn swap_panes(source: &PaneId, target: &PaneId) -> Call<SwapResult> {
    Call::new(
        "pane.swap",
        json!({"source_pane_id": s(source.as_str()), "target_pane_id": s(target.as_str())}),
        |r| field(r?, "swap"),
    )
}

/// `pane.layout`: the layout of the pane's tab (panorama `pane_layout`).
pub fn pane_layout(pane: &PaneId) -> Call<LayoutInfo> {
    Call::new("pane.layout", json!({"pane_id": s(pane.as_str())}), layout)
}

/// `layout.export` of a tab (panorama `layout_export`).
pub fn layout_export(tab: &TabId) -> Call<LayoutInfo> {
    Call::new("layout.export", json!({"tab_id": s(tab.as_str())}), layout)
}

/// `layout.set_split_ratio`: one split of the tab (`path`: `false` = first
/// child, `true` = second, from the root); `ratio` is the first child's
/// share (panorama `set_split_ratio`).
pub fn set_split_ratio(tab: &TabId, path: &[bool], ratio: f64) -> Call<Option<LayoutInfo>> {
    Call::new(
        "layout.set_split_ratio",
        json!({"tab_id": s(tab.as_str()), "path": path, "ratio": ratio}),
        |r| echo(r, "layout"),
    )
}

/// `layout.apply`: a new tab with the split tree `root`, passed as given
/// (panorama `apply_panel_layout`, which caps the label at 200).
pub fn apply_layout(tab_label: &Sanitized, focus: bool, root: Value) -> Call<LayoutInfo> {
    Call::new(
        "layout.apply",
        json!({"tab_label": s(tab_label.as_str()), "focus": focus, "root": root}),
        layout,
    )
}

/// `pane.report_metadata`: set the pane's token `key` to `value`, or clear it
/// with `None` (herdr-inbox `set_open_count`: `inbox_q` = `?N`).
pub fn set_token(pane: &PaneId, source: &str, key: &str, value: Option<&Sanitized>) -> Call<()> {
    let mut tokens = Map::new();
    tokens.insert(
        clean(key, MAX_ARG),
        value.map_or(Value::Null, |v| s(v.as_str())),
    );
    Call::new(
        "pane.report_metadata",
        json!({"pane_id": s(pane.as_str()), "source": s(source), "tokens": tokens,
               "clear_title": false, "clear_display_agent": false, "clear_state_labels": false}),
        unit,
    )
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

/// `plugin.list` (herdr-inbox `plugin_root`, radar; panorama `inbox_link`).
pub fn plugins() -> Call<Vec<PluginInfo>> {
    Call::new("plugin.list", json!({}), |r| Ok(list(r?, "plugins")))
}

/// `plugin.action.invoke` of `<plugin id>.<action>` (panorama
/// `invoke_action`: `leavemyyard.herdr-inbox.open`). The context is the one
/// herdr's CLI sends.
pub fn invoke_action(action_id: &str) -> Call<()> {
    Call::new(
        "plugin.action.invoke",
        json!({"action_id": s(action_id), "context": {"invocation_source": "cli"}}),
        unit,
    )
}

/// Where [`OpenPluginPane`] opens the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Placement {
    /// A popup (herdr answers no pane for it).
    Popup,
    /// A split of `target_pane`.
    Split,
    /// A tab of its own.
    Tab,
}

impl Placement {
    fn as_str(self) -> &'static str {
        match self {
            Self::Popup => "popup",
            Self::Split => "split",
            Self::Tab => "tab",
        }
    }
}

/// A split's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Direction {
    /// To the right.
    Right,
    /// Below.
    Down,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Down => "down",
        }
    }
}

/// `plugin.pane.open` of one of a plugin's entrypoints. `focus` defaults to
/// `true`, as herdr's CLI does.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct OpenPluginPane {
    /// The plugin (`leavemyyard.herdr-panorama`).
    pub plugin_id: String,
    /// Its entrypoint (`panorama`, `panel`, `new-agent`, `inbox`).
    pub entrypoint: String,
    /// Where; `None` is the entrypoint's own placement.
    pub placement: Option<Placement>,
    /// A split's direction.
    pub direction: Option<Direction>,
    /// The pane to split.
    pub target_pane: Option<PaneId>,
    /// The pane's working directory (the plugin's root).
    pub cwd: Option<std::path::PathBuf>,
    /// Whether the new pane takes the focus.
    pub focus: bool,
    /// Extra environment for the pane's command.
    pub env: Vec<(String, String)>,
    /// A popup's outer size in cells, `(width, height)`.
    pub size: Option<(u16, u16)>,
}

impl OpenPluginPane {
    /// `entrypoint` of `plugin_id`, focused, at its own placement.
    pub fn new(plugin_id: impl Into<String>, entrypoint: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            entrypoint: entrypoint.into(),
            placement: None,
            direction: None,
            target_pane: None,
            cwd: None,
            focus: true,
            env: Vec::new(),
            size: None,
        }
    }
}

/// `plugin.pane.open`. Answers the new pane, or `None` for a popup (herdr
/// answers `{"type": "ok"}` with no pane there).
pub fn open_plugin_pane(open: &OpenPluginPane) -> Call<Option<PaneInfo>> {
    let mut p = json!({"plugin_id": s(&open.plugin_id), "entrypoint": s(&open.entrypoint),
                       "focus": open.focus});
    if let Some(placement) = open.placement {
        p["placement"] = json!(placement.as_str());
    }
    if let Some(direction) = open.direction {
        p["direction"] = json!(direction.as_str());
    }
    if let Some(target) = &open.target_pane {
        p["target_pane_id"] = s(target.as_str());
    }
    if let Some(cwd) = &open.cwd {
        p["cwd"] = path(cwd);
    }
    if !open.env.is_empty() {
        let env: Map<String, Value> = open
            .env
            .iter()
            .map(|(k, v)| (clean(k, MAX_ARG), s(v)))
            .collect();
        p["env"] = Value::Object(env);
    }
    if let Some((w, h)) = open.size {
        p["width"] = json!(w);
        p["height"] = json!(h);
    }
    Call::new("plugin.pane.open", p, |r| {
        let mut v = r?;
        Ok(v.pointer_mut("/plugin_pane/pane")
            .map(Value::take)
            .and_then(|pane| serde_json::from_value::<PaneInfo>(pane).ok())
            .filter(|pane| pane.pane_id.is_some()))
    })
}

/// `plugin.pane.close` (panorama `close_plugin_pane`).
pub fn close_plugin_pane(pane: &PaneId) -> Call<()> {
    Call::new(
        "plugin.pane.close",
        json!({"pane_id": s(pane.as_str())}),
        unit,
    )
}

/// `notification.show` (herdr-inbox `notify`, with sound `request`).
pub fn notify(
    title: &Sanitized,
    body: &Sanitized,
    sound: Option<&str>,
) -> Call<NotificationResult> {
    let mut params = json!({"title": s(title.as_str()), "body": s(body.as_str())});
    if let Some(sound) = sound {
        params["sound"] = s(sound);
    }
    Call::new("notification.show", params, |r| {
        serde_json::from_value(r?).map_err(|e| HerdrError::Garbled(e.to_string()))
    })
}

/// `server.reload_config` (herdr-inbox radar).
pub fn reload_config() -> Call<Extra> {
    Call::new("server.reload_config", json!({}), extra)
}

/// Any method, params as given (not cleaned), the raw `result`. For what
/// has no typed call yet.
pub fn raw_call(method: &'static str, params: Value) -> Call<Value> {
    Call::new(method, params, raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_param_goes_through_clean() {
        let call = prompt(&PaneId::new("w1:p1\x1b[2J"), &Sanitized::new("hi"));
        assert_eq!(call.params()["target"], "w1:p1[2J");
        let open = OpenPluginPane {
            env: vec![("K\n".into(), "v\x07".into())],
            ..OpenPluginPane::new("p", "e")
        };
        assert_eq!(open_plugin_pane(&open).params()["env"], json!({"K ": "v"}));
    }

    #[test]
    fn no_agent_is_none_and_other_errors_stay_errors() {
        let call = agent(&PaneId::new("w1:p3"));
        let gone = HerdrError::Refused {
            code: AGENT_NOT_FOUND.into(),
            message: "x".into(),
        };
        assert_eq!(call.parse(Err(gone)).unwrap(), None);
        assert_eq!(call.parse(Ok(json!({}))).unwrap(), None);
        assert_eq!(
            call.parse(Err(HerdrError::NotRunning)),
            Err(HerdrError::NotRunning)
        );
    }

    #[test]
    fn line_ends_read_as_in_python_text_mode() {
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn start_agent_waits_its_own_timeout_plus_five_seconds() {
        let call = start_agent(
            &Sanitized::new("a"),
            "claude",
            &PaneId::new("w1:p8"),
            60_000,
        );
        assert_eq!(call.timeout(), Some(Duration::from_secs(65)));
    }
}
