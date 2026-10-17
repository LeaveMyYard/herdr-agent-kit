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
