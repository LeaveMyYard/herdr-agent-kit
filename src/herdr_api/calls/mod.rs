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
//!
//! [`clean`]: super::clean
//! [`Sanitized`]: super::Sanitized

mod agent;
mod call;
mod pane;
mod plugin;
mod reply;
mod session;
mod tab;

#[cfg(test)]
mod tests;

pub(crate) use agent::not_ready;
pub use agent::{AGENT_NOT_FOUND, agent, agent_named, agents, focus_agent, prompt, start_agent};
pub use call::{Call, raw_call};
pub use pane::{
    MoveTo, focus_pane, move_pane, pane, panes, process_info, read_pane, rename_pane, set_token,
    swap_panes, toggle_zoom, universal_newlines,
};
pub use plugin::{
    Direction, OpenPluginPane, Placement, close_plugin_pane, invoke_action, open_plugin_pane,
    plugins,
};
pub use session::{notify, reload_config, snapshot, workspaces};
pub use tab::{
    apply_layout, close_tab, create_tab, focus_tab, layout_export, move_tab, pane_layout,
    rename_tab, set_split_ratio, tab, tabs,
};
