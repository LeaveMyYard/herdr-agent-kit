use std::path::PathBuf;

use super::*;
use crate::herdr_api::{HerdrError, PaneId};

#[test]
fn every_argument_is_cleaned() {
    let inv = machine_focus_agent("m\n1", &PaneId::new("w1:p1\x1b[2J"));
    assert_eq!(inv.args, ["--machine", "m 1", "agent", "focus", "w1:p1[2J"]);
}

#[test]
fn exit_errors_keep_herdrs_code_or_its_text() {
    let e = exit_error(
        Some(1),
        b"",
        br#"{"error":{"code":"agent_not_found","message":"agent target w1:p3 not found"},"id":"cli:agent:get"}"#,
    );
    assert!(e.is_code("agent_not_found"));
    let e = exit_error(
        Some(2),
        b"",
        b"error: unknown machine 'nosuch'; use `herdr machine list`\n",
    );
    assert_eq!(
        e.to_string(),
        "error: unknown machine 'nosuch'; use `herdr machine list`"
    );
    assert_eq!(exit_error(Some(3), b"", b"").to_string(), "exit 3");
    assert_eq!(exit_error(Some(1), b" out \n", b"").to_string(), "out");
}

/// herdr-inbox tests/test_successor.py::test_server_gone_needs_herdrs_own_word
/// (the stderr shape is that test's fake herdr: no recording has a stopped
/// server).
#[test]
fn only_server_not_running_means_not_running() {
    let gone =
        br#"{"id": "cli:workspace:list", "error": {"code": "server_not_running", "message": "x"}}"#;
    assert_eq!(exit_error(Some(1), b"", gone), HerdrError::NotRunning);
    let other = br#"{"error": {"code": "other", "message": "x"}}"#;
    assert!(exit_error(Some(1), b"", other).is_code("other"));
    assert!(matches!(
        exit_error(Some(1), b"", b"not json"),
        HerdrError::Refused { .. }
    ));
}

#[test]
fn machine_list_reads_a_list_or_an_envelope() {
    assert_eq!(parse_machines("[]\n").unwrap(), vec![]);
    assert_eq!(parse_machines("").unwrap(), vec![]);
    assert!(parse_machines("nope").is_err());
    let m =
        parse_machines(r#"[{"id": "m1", "label": "box", "target": "u@h", "enabled": true}, 3]"#)
            .unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].target.as_deref(), Some("u@h"));
    let m =
        parse_machines(r#"{"result": {"machines": [{"id": "m2", "enabled": false}]}}"#).unwrap();
    assert_eq!(m[0].id.as_deref(), Some("m2"));
}

#[test]
fn a_machine_snapshot_needs_a_snapshot() {
    assert!(parse_snapshot(r#"{"result": {"snapshot": {"version": "0.9.3"}}}"#).is_ok());
    assert_eq!(
        parse_snapshot(r#"{"result": {}}"#),
        Err(HerdrError::Garbled("no snapshot".into()))
    );
    assert_eq!(
        parse_snapshot("x"),
        Err(HerdrError::Garbled("unreadable snapshot".into()))
    );
}

#[test]
fn config_dir_is_trimmed() {
    assert_eq!(parse_config_dir("/a/b\n"), Some(PathBuf::from("/a/b")));
    assert_eq!(parse_config_dir("  \n"), None);
}
