//! What herdr has registered besides the session: plugins (`plugin.list`)
//! and saved SSH machines (`herdr machine list`).

use serde::{Deserialize, Serialize};

use super::Extra;
use super::lenient::lenient;

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
