//! [`PluginEnv`]: the environment, read once.
//!
//! herdr sets `HERDR_*` variables for plugin commands, panes and actions, and
//! Claude Code's hooks and MCP servers inherit `HERDR_PANE_ID` and
//! `HERDR_SOCKET_PATH` from the pane. A process takes one snapshot with
//! [`PluginEnv::from_env`]; tests build one with [`PluginEnv::from_vars`], so
//! no test ever reads (or leaks into) the real environment.
//!
//! As in the Python plugins (`os.environ.get(NAME) or default`), an **empty**
//! variable counts as unset everywhere.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// `HERDR_PLUGIN_ID`: the plugin herdr runs this command for.
pub const HERDR_PLUGIN_ID: &str = "HERDR_PLUGIN_ID";
/// `HERDR_PLUGIN_ROOT`: the plugin's install (or linked checkout) directory.
pub const HERDR_PLUGIN_ROOT: &str = "HERDR_PLUGIN_ROOT";
/// `HERDR_PLUGIN_STATE_DIR`: herdr's state directory for this plugin.
pub const HERDR_PLUGIN_STATE_DIR: &str = "HERDR_PLUGIN_STATE_DIR";
/// `HERDR_PLUGIN_CONFIG_DIR`: herdr's config directory for this plugin.
pub const HERDR_PLUGIN_CONFIG_DIR: &str = "HERDR_PLUGIN_CONFIG_DIR";
/// `HERDR_PLUGIN_ENTRYPOINT_ID`: the entrypoint herdr started.
pub const HERDR_PLUGIN_ENTRYPOINT_ID: &str = "HERDR_PLUGIN_ENTRYPOINT_ID";
/// `HERDR_PLUGIN_CONTEXT_JSON`: the invocation context (`focused_pane_id`, …).
pub const HERDR_PLUGIN_CONTEXT_JSON: &str = "HERDR_PLUGIN_CONTEXT_JSON";
/// `HERDR_SOCKET_PATH`: the herdr server's API socket.
pub const HERDR_SOCKET_PATH: &str = "HERDR_SOCKET_PATH";
/// `HERDR_PANE_ID`: the pane this process started in.
pub const HERDR_PANE_ID: &str = "HERDR_PANE_ID";
/// `HERDR_TAB_ID`: the tab this process started in.
pub const HERDR_TAB_ID: &str = "HERDR_TAB_ID";
/// `HERDR_WORKSPACE_ID`: the workspace this process started in.
pub const HERDR_WORKSPACE_ID: &str = "HERDR_WORKSPACE_ID";
/// `HERDR_BIN_PATH`: the herdr executable (default `herdr` on `PATH`).
pub const HERDR_BIN_PATH: &str = "HERDR_BIN_PATH";
/// `HERDR_ENV`: set inside herdr panes.
pub const HERDR_ENV: &str = "HERDR_ENV";

/// The plugin variables herdr sets for a plugin's own commands
/// (panorama `herdr.PLUGIN_ENV`).
pub const PLUGIN_VARS: [&str; 4] = [
    HERDR_PLUGIN_ID,
    HERDR_PLUGIN_ROOT,
    HERDR_PLUGIN_STATE_DIR,
    HERDR_PLUGIN_CONFIG_DIR,
];

/// A snapshot of the process environment, plus the home directory and user id
/// the path rules need.
#[derive(Clone, Debug)]
pub struct PluginEnv {
    vars: HashMap<OsString, OsString>,
    home: Option<PathBuf>,
    uid: u32,
}

impl PluginEnv {
    /// The real environment of this process, its home directory (`$HOME`, else
    /// the password database, as Python's `expanduser("~")`) and its real uid.
    pub fn from_env() -> Self {
        let vars: HashMap<OsString, OsString> = std::env::vars_os().collect();
        let home = vars
            .get(OsStr::new("HOME"))
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .or_else(std::env::home_dir);
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        PluginEnv { vars, home, uid }
    }

    /// An environment made of `vars` only, with this home directory and uid.
    /// Tests use this instead of touching the process environment.
    pub fn from_vars<I, K, V>(vars: I, home: Option<PathBuf>, uid: u32) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        let vars = vars
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        PluginEnv { vars, home, uid }
    }

    /// The value of `name`, or `None` when it is unset or empty.
    pub fn var_os(&self, name: &str) -> Option<&OsStr> {
        self.vars
            .get(OsStr::new(name))
            .map(OsString::as_os_str)
            .filter(|v| !v.is_empty())
    }

    /// The value of `name` as UTF-8, or `None` when it is unset, empty or not
    /// UTF-8.
    pub fn var(&self, name: &str) -> Option<&str> {
        self.var_os(name).and_then(OsStr::to_str)
    }

    /// The value of `name` as a path, or `None` when it is unset or empty.
    pub fn path_var(&self, name: &str) -> Option<&Path> {
        self.var_os(name).map(Path::new)
    }

    /// The home directory, if one is known.
    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// The real user id of the process.
    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// `$HERDR_PLUGIN_ID`, else `default` (the plugin's own id).
    pub fn plugin_id<'a>(&'a self, default: &'a str) -> &'a str {
        self.var(HERDR_PLUGIN_ID).unwrap_or(default)
    }

    /// `$HERDR_PLUGIN_ROOT`.
    pub fn plugin_root(&self) -> Option<&Path> {
        self.path_var(HERDR_PLUGIN_ROOT)
    }

    /// `$HERDR_PLUGIN_STATE_DIR`.
    pub fn plugin_state_dir(&self) -> Option<&Path> {
        self.path_var(HERDR_PLUGIN_STATE_DIR)
    }

    /// `$HERDR_PLUGIN_CONFIG_DIR`.
    pub fn plugin_config_dir(&self) -> Option<&Path> {
        self.path_var(HERDR_PLUGIN_CONFIG_DIR)
    }

    /// `$HERDR_PLUGIN_ENTRYPOINT_ID`.
    pub fn plugin_entrypoint_id(&self) -> Option<&str> {
        self.var(HERDR_PLUGIN_ENTRYPOINT_ID)
    }

    /// `$HERDR_PLUGIN_CONTEXT_JSON`, unparsed.
    pub fn plugin_context_json(&self) -> Option<&str> {
        self.var(HERDR_PLUGIN_CONTEXT_JSON)
    }

    /// `$HERDR_SOCKET_PATH`.
    pub fn socket_path(&self) -> Option<&Path> {
        self.path_var(HERDR_SOCKET_PATH)
    }

    /// `$HERDR_PANE_ID`.
    pub fn pane_id(&self) -> Option<&str> {
        self.var(HERDR_PANE_ID)
    }

    /// `$HERDR_TAB_ID`.
    pub fn tab_id(&self) -> Option<&str> {
        self.var(HERDR_TAB_ID)
    }

    /// `$HERDR_WORKSPACE_ID`.
    pub fn workspace_id(&self) -> Option<&str> {
        self.var(HERDR_WORKSPACE_ID)
    }

    /// `$HERDR_BIN_PATH`, else `herdr` (looked up on `PATH`).
    pub fn bin_path(&self) -> &OsStr {
        self.var_os(HERDR_BIN_PATH)
            .unwrap_or_else(|| OsStr::new("herdr"))
    }

    /// Does this process run inside herdr (`HERDR_PANE_ID` or `HERDR_ENV`)?
    pub fn in_herdr(&self) -> bool {
        self.var_os(HERDR_PANE_ID).is_some() || self.var_os(HERDR_ENV).is_some()
    }

    /// `$XDG_STATE_HOME`, else `~/.local/state`.
    pub fn xdg_state_home(&self) -> PathBuf {
        self.xdg("XDG_STATE_HOME", &[".local", "state"])
    }

    /// `$XDG_CONFIG_HOME`, else `~/.config`.
    pub fn xdg_config_home(&self) -> PathBuf {
        self.xdg("XDG_CONFIG_HOME", &[".config"])
    }

    /// `$XDG_RUNTIME_DIR`.
    pub fn xdg_runtime_dir(&self) -> Option<&Path> {
        self.path_var("XDG_RUNTIME_DIR")
    }

    fn xdg(&self, name: &str, under_home: &[&str]) -> PathBuf {
        if let Some(dir) = self.path_var(name) {
            return dir.to_path_buf();
        }
        // Python's expanduser("~") leaves "~" as it is when there is no home.
        let mut path = self.home.clone().unwrap_or_else(|| PathBuf::from("~"));
        path.extend(under_home);
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> PluginEnv {
        PluginEnv::from_vars(
            vars.iter().map(|(k, v)| (*k, *v)),
            Some(PathBuf::from("/home/u")),
            1000,
        )
    }

    #[test]
    fn empty_counts_as_unset() {
        let e = env(&[
            (HERDR_PANE_ID, ""),
            (HERDR_SOCKET_PATH, "/s"),
            ("XDG_STATE_HOME", ""),
        ]);
        assert_eq!(e.pane_id(), None);
        assert_eq!(e.socket_path(), Some(Path::new("/s")));
        assert_eq!(e.xdg_state_home(), PathBuf::from("/home/u/.local/state"));
        assert!(!e.in_herdr());
    }

    #[test]
    fn defaults() {
        let e = env(&[]);
        assert_eq!(e.plugin_id("x.y"), "x.y");
        assert_eq!(e.bin_path(), "herdr");
        assert_eq!(e.xdg_config_home(), PathBuf::from("/home/u/.config"));
        assert_eq!(e.xdg_runtime_dir(), None);
        let e = env(&[
            (HERDR_PLUGIN_ID, "a.b"),
            (HERDR_BIN_PATH, "/bin/h"),
            (HERDR_ENV, "1"),
        ]);
        assert_eq!(e.plugin_id("x.y"), "a.b");
        assert_eq!(e.bin_path(), "/bin/h");
        assert!(e.in_herdr());
    }

    #[test]
    fn no_home_keeps_a_literal_tilde_like_expanduser() {
        let e = PluginEnv::from_vars(Vec::<(String, String)>::new(), None, 0);
        assert_eq!(e.xdg_state_home(), PathBuf::from("~/.local/state"));
    }
}
