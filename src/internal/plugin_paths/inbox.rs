//! herdr-inbox's database path, server id and lock paths (Python
//! `store.db_path`, `herdr.server_id`, `worker._lock_path`, `rewake._dir`,
//! `deeplink._lock_path`, `tabs.marker_lock`).
//!
//! The inbox keeps its state next to its database, not in
//! `$HERDR_PLUGIN_STATE_DIR`: the hooks and MCP servers that Claude Code
//! starts do not get the plugin variables, and they must find the same files.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::session_file_name;
use crate::plugin_env::{PluginEnv, short_sha1};

/// `HERDR_INBOX_DB`: overrides the database path (tests, remote machines).
pub const HERDR_INBOX_DB: &str = "HERDR_INBOX_DB";

/// The Rust worker's lock contract (herdr-inbox `worker._start_rust_worker`):
/// retry `LOCK_EX|LOCK_NB` for about this long before concluding another
/// worker holds the lock (a Python probe holds `LOCK_SH` for an instant every
/// 50 ms), then exit 0.
pub const WORKER_LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(200);

/// `$HERDR_INBOX_DB`, else `$XDG_STATE_HOME/herdr-inbox/inbox.db`.
pub fn db_path(env: &PluginEnv) -> PathBuf {
    match env.path_var(HERDR_INBOX_DB) {
        Some(db) => db.to_path_buf(),
        None => env.xdg_state_home().join("herdr-inbox").join("inbox.db"),
    }
}

/// The directory the inbox's state and locks live in: the database's parent.
pub fn state_dir(env: &PluginEnv) -> PathBuf {
    parent(&db_path(env))
}

/// Which herdr server this process talks to: `$HERDR_SOCKET_PATH`, else
/// herdr's default socket `$XDG_CONFIG_HOME/herdr/herdr.sock`.
pub fn server_id(env: &PluginEnv) -> OsString {
    match env.socket_path() {
        Some(sock) => sock.as_os_str().to_owned(),
        None => env
            .xdg_config_home()
            .join("herdr")
            .join("herdr.sock")
            .into_os_string(),
    }
}

/// The 12-digit key of a server: `sha1(server)[:12]`.
pub fn server_key(server: &OsStr) -> String {
    short_sha1(server.as_encoded_bytes())
}

/// One delivery worker per herdr server:
/// `<state_dir>/worker-<sha1(server)[:12]>.lock`, for `server` or, when it is
/// `None`, this process's [`server_id`].
pub fn worker_lock_path(env: &PluginEnv, server: Option<&OsStr>) -> PathBuf {
    let key = match server.filter(|s| !s.is_empty()) {
        Some(server) => server_key(server),
        None => server_key(&server_id(env)),
    };
    state_dir(env).join(format!("worker-{key}.lock"))
}

/// The rewake watchers' directory: `<state_dir>/rewake`.
pub fn rewake_dir(env: &PluginEnv) -> PathBuf {
    state_dir(env).join("rewake")
}

/// A session's rewake watcher lock: `<state_dir>/rewake/<session>.lock`, held
/// for the watcher's life. `None` unless `session_id` is a canonical UUID; the
/// file name is its lowercase form.
pub fn rewake_lock_path(env: &PluginEnv, session_id: &str) -> Option<PathBuf> {
    let name = session_file_name(session_id)?;
    Some(rewake_dir(env).join(format!("{name}.lock")))
}

/// A session's rewake owner file: `<state_dir>/rewake/<session>.owner`.
pub fn rewake_owner_path(env: &PluginEnv, session_id: &str) -> Option<PathBuf> {
    let name = session_file_name(session_id)?;
    Some(rewake_dir(env).join(format!("{name}.owner")))
}

/// The popup's lock: `<state_dir>/popup.lock`, held by a running popup.
pub fn popup_lock_path(env: &PluginEnv) -> PathBuf {
    state_dir(env).join("popup.lock")
}

/// The tab-label lock: `<db_path>.markers.lock`.
pub fn markers_lock_path(env: &PluginEnv) -> PathBuf {
    let mut path = db_path(env).into_os_string();
    path.push(".markers.lock");
    PathBuf::from(path)
}

/// `Path(p).parent` in Python: `"."` for a bare file name.
fn parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        Some(_) => PathBuf::from("."),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests;
