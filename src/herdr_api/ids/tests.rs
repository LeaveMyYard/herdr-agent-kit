use super::*;
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let map: HashMap<String, OsString> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), OsString::from(v)))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn the_server_is_the_socket_path_first() {
    let id = ServerId::resolve(env(&[
        ("HERDR_SOCKET_PATH", "/run/h.sock"),
        ("XDG_CONFIG_HOME", "/x"),
        ("HOME", "/home/u"),
    ]));
    assert_eq!(id.socket(), Path::new("/run/h.sock"));
}

#[test]
fn then_xdg_config_home_then_home() {
    let id = ServerId::resolve(env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/home/u")]));
    assert_eq!(id.socket(), Path::new("/x/herdr/herdr.sock"));
    let id = ServerId::resolve(env(&[("HERDR_SOCKET_PATH", ""), ("HOME", "/home/u")]));
    assert_eq!(id.socket(), Path::new("/home/u/.config/herdr/herdr.sock"));
}

#[test]
fn ids_serialize_as_plain_strings() {
    let pane: PaneId = serde_json::from_str("\"w1:p2\"").unwrap();
    assert_eq!(pane.as_str(), "w1:p2");
    assert_eq!(
        serde_json::to_string(&TabId::new("w1:t3")).unwrap(),
        "\"w1:t3\""
    );
}
