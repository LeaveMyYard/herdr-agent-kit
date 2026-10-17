//! Plugin calls: `plugin.list`, `plugin.action.invoke`, `plugin.pane.open`
//! (with [`OpenPluginPane`]) and `plugin.pane.close`.

use serde_json::{Map, Value, json};

use super::call::Call;
use super::params::{path, s};
use super::reply::{list, unit};
use crate::herdr_api::ids::PaneId;
use crate::herdr_api::sanitize::{MAX_ARG, clean};
use crate::herdr_api::types::{PaneInfo, PluginInfo};

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
