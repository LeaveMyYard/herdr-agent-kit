//! The CLI path, against `CliStub`.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use herdr_agent_kit::herdr_api::fake::{self, CliReply, CliStub};
use herdr_agent_kit::herdr_api::{Herdr, HerdrError, ServerId, blocking};

use crate::support::*;

#[tokio::test(flavor = "current_thread")]
async fn the_cli_path_reads_the_recorded_answers() {
    let stub = CliStub::recorded();
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin(stub.bin());
    let b = h.blocking();
    assert_eq!(h.machine_list().await, Ok(vec![]));
    assert_eq!(b.machine_list(), Ok(vec![]));
    let unknown = "error: unknown machine 'nosuch'; use `herdr machine list`";
    let errs = [
        h.machine_snapshot("nosuch").await.unwrap_err(),
        b.machine_snapshot("nosuch").unwrap_err(),
    ];
    for err in errs {
        let want = HerdrError::Refused {
            code: String::new(),
            message: unknown.into(),
        };
        assert_eq!(err, want);
    }
    let err = h
        .machine_focus_agent("nosuch", &p("w1:p2"))
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), unknown);
    let record = fake::recording("plugin.jsonl", 3).unwrap();
    let dir = PathBuf::from(record["stdout"].as_str().unwrap().trim());
    let got = h.plugin_config_dir("leavemyyard.herdr-panorama").await;
    assert_eq!(got, Ok(Some(dir.clone())));
    assert_eq!(
        b.plugin_config_dir("leavemyyard.herdr-panorama"),
        Ok(Some(dir))
    );
    let other = h.plugin_config_dir("nobody.none").await.unwrap();
    assert!(other.is_some(), "herdr answers any id");
    let calls = stub.calls();
    assert_eq!(calls[0], ["machine", "list", "--json"]);
    assert_eq!(calls[2], ["--machine", "nosuch", "api", "snapshot"]);
}

/// panorama tests/test_herdr.py::test_snapshot_and_focus_through_the_fake: every
/// CLI argument goes through clean().
#[tokio::test(flavor = "current_thread")]
async fn cli_arguments_are_cleaned() {
    let mut stub = CliStub::new();
    stub.on(
        &["--machine", "m1", "agent", "focus", "w1:p1[2J"],
        CliReply::ok(""),
    );
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin(stub.bin());
    h.machine_focus_agent("m1", &p("w1:p1\x1b[2J"))
        .await
        .unwrap();
    assert_eq!(
        stub.calls(),
        [["--machine", "m1", "agent", "focus", "w1:p1[2J"]]
    );
}

/// panorama tests/test_herdr.py::test_missing_binary_is_an_error_not_an_exception
/// and herdr-inbox tests/test_successor.py::test_server_gone_without_a_herdr_binary_is_not_gone
#[tokio::test(flavor = "current_thread")]
async fn a_missing_binary_is_unreachable() {
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin("/nonexistent/herdr");
    assert!(matches!(
        h.machine_list().await,
        Err(HerdrError::Unreachable(_))
    ));
    let got = h.blocking().plugin_config_dir("x");
    assert!(matches!(got, Err(HerdrError::Unreachable(_))));
}

#[tokio::test(flavor = "current_thread")]
async fn a_slow_cli_times_out_and_server_not_running_is_not_running() {
    let mut stub = CliStub::new();
    let slow = CliReply {
        delay: Some(Duration::from_secs(3)),
        ..CliReply::ok("[]")
    };
    stub.on(&["machine", "list", "--json"], slow);
    let h = Herdr::new(ServerId::new("/nowhere"))
        .with_bin(stub.bin())
        .with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert!(matches!(
        h.machine_list().await,
        Err(HerdrError::NoReply(_))
    ));
    assert!(matches!(
        h.blocking().machine_list(),
        Err(HerdrError::NoReply(_))
    ));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    // the stderr herdr-inbox tests/test_successor.py's fake herdr prints for a gone server
    let gone = CliReply {
        stderr:
            r#"{"id": "cli:machine:list", "error": {"code": "server_not_running", "message": "x"}}"#
                .into(),
        exit: 1,
        ..CliReply::default()
    };
    stub.on(&["machine", "list", "--json"], gone);
    assert_eq!(h.machine_list().await, Err(HerdrError::NotRunning));
}

/// Review L2: herdr exits, but a grandchild keeps its stdout open (ssh's
/// ControlPersist master under `--machine`): the blocking CLI path still
/// returns within its timeout.
#[test]
fn a_cli_whose_grandchild_holds_the_pipe_still_times_out() {
    let dir = scratch_dir("hg");
    let bin = dir.join("herdr");
    std::fs::write(&bin, "#!/bin/sh\nsleep 3 &\nprintf '[]'\nexit 0\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let h = blocking::Herdr::new(ServerId::new("/nowhere"))
        .with_bin(&bin)
        .with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert!(matches!(h.machine_list(), Err(HerdrError::NoReply(_))));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
