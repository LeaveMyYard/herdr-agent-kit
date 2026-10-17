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
fn an_empty_home_is_root_like_expanduser() {
    assert_eq!(home_from(Some(OsStr::new(""))), Some(PathBuf::from("/")));
    assert_eq!(home_from(Some(OsStr::new("/h"))), Some(PathBuf::from("/h")));
    let e = PluginEnv::from_vars([("HOME", "")], home_from(Some(OsStr::new(""))), 0);
    assert_eq!(e.xdg_state_home(), PathBuf::from("/.local/state"));
}

#[test]
fn no_home_keeps_a_literal_tilde_like_expanduser() {
    let e = PluginEnv::from_vars(Vec::<(String, String)>::new(), None, 0);
    assert_eq!(e.xdg_state_home(), PathBuf::from("~/.local/state"));
}
