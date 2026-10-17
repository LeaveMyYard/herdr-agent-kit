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
