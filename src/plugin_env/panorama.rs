//! herdr-panorama's state directory, session key, socket directory and lock
//! paths (Python `home.state_dir`, `home.session_key`, `protocol.socket_dir`,
//! `protocol.lock_path`).

use std::path::PathBuf;

use super::{PluginEnv, short_sha1};

/// The longest socket path the core uses: `sockaddr_un` holds 108 bytes on
/// Linux and 104 on macOS, so stay under both (`protocol.SUN_PATH_MAX`).
pub const SUN_PATH_MAX: usize = 100;

/// The file name the socket directory is sized for: a core socket name with
/// a 12-digit key (`protocol.socket_dir`).
const SIZING_NAME: &str = "core-0123456789ab.sock";

/// `$HERDR_PLUGIN_STATE_DIR`, else `$XDG_STATE_HOME/herdr-panorama`.
pub fn state_dir(env: &PluginEnv) -> PathBuf {
    match env.plugin_state_dir() {
        Some(dir) => dir.to_path_buf(),
        None => env.xdg_state_home().join("herdr-panorama"),
    }
}

/// One herdr server (its socket) → one key: the first 12 hex digits of
/// `sha1($HERDR_SOCKET_PATH or "default")`. Two `herdr --session` servers
/// never read or overwrite each other's state.
///
/// This is **not** the inbox's server key ([`super::inbox::server_key`]),
/// which falls back to herdr's default socket path instead of `"default"`.
pub fn session_key(env: &PluginEnv) -> String {
    match env.socket_path() {
        Some(sock) => short_sha1(sock.as_os_str().as_encoded_bytes()),
        None => short_sha1(b"default"),
    }
}

/// The core's own directory for its socket and lock: `<state_dir>/sockets`,
/// else `$XDG_RUNTIME_DIR` (or `/tmp`) `/herdr-panorama-<uid>` when the first
/// is too long for a Unix socket path, else `/tmp/herdr-panorama-<uid>`.
///
/// The length is counted in **characters**, as Python's `len(str(path))`
/// does, not bytes: a Python core and a Rust core must pick the same directory
/// to exclude each other. The caller makes it with
/// [`super::private_dir::make_private_dir`].
pub fn socket_dir(env: &PluginEnv) -> PathBuf {
    let runtime = env
        .xdg_runtime_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let own = format!("herdr-panorama-{}", env.uid());
    for dir in [state_dir(env).join("sockets"), runtime.join(&own)] {
        // Python measures the normalised `str(Path)`: no repeated or trailing
        // separators, no inner `.` components. Known gap: a relative state dir
        // with a leading "./", or one starting with "//", measures differently
        // at the boundary; herdr always passes absolute directories.
        let measured: PathBuf = dir.join(SIZING_NAME).components().collect();
        if measured.to_string_lossy().chars().count() <= SUN_PATH_MAX {
            return dir;
        }
    }
    PathBuf::from("/tmp").join(own)
}

/// The core's socket: `<socket_dir>/core-<session_key>.sock`.
pub fn core_socket_path(env: &PluginEnv) -> PathBuf {
    socket_dir(env).join(format!("core-{}.sock", session_key(env)))
}

/// The core's lock: `<socket_dir>/core-<session_key>.lock`, held (exclusive)
/// for the core's whole life. Open it with [`super::LockFile::private`].
pub fn core_lock_path(env: &PluginEnv) -> PathBuf {
    socket_dir(env).join(format!("core-{}.lock", session_key(env)))
}

/// The per-session home lock: `<state_dir>/home-<session_key>.lock`
/// (`home._lock`, mode 0644, waited for at most `LOCK_SECONDS`).
pub fn home_lock_path(env: &PluginEnv) -> PathBuf {
    state_dir(env).join(format!("home-{}.lock", session_key(env)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_env::vars::{HERDR_PLUGIN_STATE_DIR, HERDR_SOCKET_PATH};

    fn env(vars: &[(&str, &str)]) -> PluginEnv {
        PluginEnv::from_vars(
            vars.iter().map(|(k, v)| (*k, *v)),
            Some(PathBuf::from("/home/u")),
            1000,
        )
    }

    #[test]
    fn state_dir_prefers_herdr_then_xdg() {
        assert_eq!(
            state_dir(&env(&[])),
            PathBuf::from("/home/u/.local/state/herdr-panorama")
        );
        assert_eq!(
            state_dir(&env(&[("XDG_STATE_HOME", "/x")])),
            PathBuf::from("/x/herdr-panorama")
        );
        assert_eq!(
            state_dir(&env(&[
                ("XDG_STATE_HOME", "/x"),
                (HERDR_PLUGIN_STATE_DIR, "/p")
            ])),
            PathBuf::from("/p")
        );
    }

    #[test]
    fn session_key_is_sha1_of_the_socket_or_default() {
        assert_eq!(session_key(&env(&[])), "7505d64a54e0");
        assert_eq!(
            session_key(&env(&[(HERDR_SOCKET_PATH, "/run/herdr.sock")])),
            "837fc5258150"
        );
    }

    #[test]
    fn core_paths_live_in_the_state_dir_when_short_enough() {
        let e = env(&[
            (HERDR_PLUGIN_STATE_DIR, "/p"),
            (HERDR_SOCKET_PATH, "/run/herdr.sock"),
        ]);
        assert_eq!(
            core_lock_path(&e),
            PathBuf::from("/p/sockets/core-837fc5258150.lock")
        );
        assert_eq!(
            core_socket_path(&e),
            PathBuf::from("/p/sockets/core-837fc5258150.sock")
        );
        assert_eq!(
            home_lock_path(&e),
            PathBuf::from("/p/home-837fc5258150.lock")
        );
    }

    #[test]
    fn socket_dir_falls_back_by_length() {
        // "/p/sockets/" + name: exactly at the limit stays in the state dir.
        let at_limit =
            "/".to_string() + &"a".repeat(SUN_PATH_MAX - 1 - "/sockets/".len() - SIZING_NAME.len());
        let e = env(&[(HERDR_PLUGIN_STATE_DIR, &at_limit)]);
        assert_eq!(socket_dir(&e), PathBuf::from(&at_limit).join("sockets"));
        let over = at_limit.clone() + "a";
        let e = env(&[
            (HERDR_PLUGIN_STATE_DIR, &over),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ]);
        assert_eq!(
            socket_dir(&e),
            PathBuf::from("/run/user/1000/herdr-panorama-1000")
        );
        let e = env(&[(HERDR_PLUGIN_STATE_DIR, &over)]);
        assert_eq!(socket_dir(&e), PathBuf::from("/tmp/herdr-panorama-1000"));
        let long_run = "/".to_string() + &"r".repeat(120);
        let e = env(&[
            (HERDR_PLUGIN_STATE_DIR, &over),
            ("XDG_RUNTIME_DIR", &long_run),
        ]);
        assert_eq!(socket_dir(&e), PathBuf::from("/tmp/herdr-panorama-1000"));
    }

    #[test]
    fn socket_dir_counts_characters_like_python() {
        // 2-byte characters: within the limit in characters, over it in bytes.
        let base = SUN_PATH_MAX - 1 - "/sockets/".len() - SIZING_NAME.len();
        let wide = "/".to_string() + &"é".repeat(base);
        assert!(wide.len() + "/sockets/".len() + SIZING_NAME.len() > SUN_PATH_MAX);
        let e = env(&[(HERDR_PLUGIN_STATE_DIR, &wide)]);
        assert_eq!(socket_dir(&e), PathBuf::from(&wide).join("sockets"));
    }

    #[test]
    fn socket_dir_measures_the_normalised_path() {
        // A trailing slash and a doubled one are not counted, as in Python.
        let base = SUN_PATH_MAX - 1 - "/sockets/".len() - SIZING_NAME.len();
        let messy = "/".to_string() + &"a".repeat(base) + "//";
        let e = env(&[(HERDR_PLUGIN_STATE_DIR, &messy)]);
        assert_eq!(socket_dir(&e), PathBuf::from(&messy).join("sockets"));
    }
}
