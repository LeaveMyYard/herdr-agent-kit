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
mod tests {
    use super::*;
    use crate::plugin_env::vars::HERDR_SOCKET_PATH;

    fn env(vars: &[(&str, &str)]) -> PluginEnv {
        PluginEnv::from_vars(
            vars.iter().map(|(k, v)| (*k, *v)),
            Some(PathBuf::from("/home/u")),
            1000,
        )
    }

    #[test]
    fn db_path_prefers_the_override_then_xdg() {
        assert_eq!(
            db_path(&env(&[])),
            PathBuf::from("/home/u/.local/state/herdr-inbox/inbox.db")
        );
        assert_eq!(
            db_path(&env(&[("XDG_STATE_HOME", "/x")])),
            PathBuf::from("/x/herdr-inbox/inbox.db")
        );
        // The plugin state dir is NOT the inbox's state dir.
        let e = env(&[
            (HERDR_INBOX_DB, "/d/i.db"),
            ("HERDR_PLUGIN_STATE_DIR", "/p"),
        ]);
        assert_eq!(state_dir(&e), PathBuf::from("/d"));
        assert_eq!(
            state_dir(&env(&[(HERDR_INBOX_DB, "i.db")])),
            PathBuf::from(".")
        );
    }

    #[test]
    fn server_id_falls_back_to_herdrs_default_socket() {
        assert_eq!(server_id(&env(&[])), "/home/u/.config/herdr/herdr.sock");
        assert_eq!(
            server_id(&env(&[("XDG_CONFIG_HOME", "/c")])),
            "/c/herdr/herdr.sock"
        );
        assert_eq!(server_id(&env(&[(HERDR_SOCKET_PATH, "/s")])), "/s");
    }

    #[test]
    fn worker_lock_is_keyed_by_the_server() {
        let e = env(&[
            (HERDR_INBOX_DB, "/d/inbox.db"),
            (HERDR_SOCKET_PATH, "/run/herdr.sock"),
        ]);
        let mine = PathBuf::from("/d/worker-837fc5258150.lock");
        assert_eq!(worker_lock_path(&e, None), mine);
        assert_eq!(
            worker_lock_path(&e, Some(OsStr::new("/run/herdr.sock"))),
            mine
        );
        assert_eq!(worker_lock_path(&e, Some(OsStr::new(""))), mine);
        assert_ne!(worker_lock_path(&e, Some(OsStr::new("/other"))), mine);
        // No socket: herdr's default socket path, not panorama's "default".
        let e = env(&[(HERDR_INBOX_DB, "/d/inbox.db")]);
        let default = short_sha1(b"/home/u/.config/herdr/herdr.sock");
        assert_eq!(
            worker_lock_path(&e, None),
            PathBuf::from(format!("/d/worker-{default}.lock"))
        );
    }

    #[test]
    fn rewake_paths_take_canonical_session_ids_only() {
        let e = env(&[(HERDR_INBOX_DB, "/d/inbox.db")]);
        assert_eq!(
            rewake_lock_path(&e, "1CD70DE7-aaaa-4bbb-8ccc-0123456789ab"),
            Some(PathBuf::from(
                "/d/rewake/1cd70de7-aaaa-4bbb-8ccc-0123456789ab.lock"
            ))
        );
        assert_eq!(
            rewake_owner_path(&e, "1cd70de7-aaaa-4bbb-8ccc-0123456789ab"),
            Some(PathBuf::from(
                "/d/rewake/1cd70de7-aaaa-4bbb-8ccc-0123456789ab.owner"
            ))
        );
        assert_eq!(rewake_lock_path(&e, "../x"), None);
        assert_eq!(rewake_owner_path(&e, "*"), None);
    }

    #[test]
    fn popup_and_marker_locks() {
        let e = env(&[(HERDR_INBOX_DB, "/d/inbox.db")]);
        assert_eq!(popup_lock_path(&e), PathBuf::from("/d/popup.lock"));
        assert_eq!(
            markers_lock_path(&e),
            PathBuf::from("/d/inbox.db.markers.lock")
        );
    }
}
