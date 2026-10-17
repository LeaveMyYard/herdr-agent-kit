use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

use serde_json::Value;

use super::*;

#[test]
fn from_env_reads_the_socket_and_the_binary() {
    let vars: HashMap<&str, OsString> = [
        ("HERDR_SOCKET_PATH", OsString::from("/run/h.sock")),
        ("HERDR_BIN_PATH", OsString::from("/opt/herdr")),
    ]
    .into();
    let h = Herdr::resolve(|n| vars.get(n).cloned());
    assert_eq!(h.server().socket(), Path::new("/run/h.sock"));
    assert_eq!(h.bin(), Path::new("/opt/herdr"));
    let h = Herdr::resolve(|n| (n == "HOME").then(|| OsString::from("/home/u")));
    assert_eq!(h.bin(), Path::new("herdr"));
    assert_eq!(
        h.server().socket(),
        Path::new("/home/u/.config/herdr/herdr.sock")
    );
}

#[test]
fn request_ids_name_the_client() {
    let h = Herdr::new(ServerId::new("/x")).with_client("panorama");
    let line = h.target.line(&calls::snapshot());
    let v: Value = serde_json::from_slice(&line).unwrap();
    assert_eq!(v["id"], "panorama:1");
    assert_eq!(h.blocking().server(), h.server());
}
