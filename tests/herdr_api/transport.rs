//! The socket transport: not running, unreachable, no reply, garbled,
//! timeouts; and the fake's own scripting.

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use herdr_agent_kit::herdr_api::fake::{FakeHerdr, Reply};
use herdr_agent_kit::herdr_api::{Herdr, HerdrError, ServerId, blocking};
use serde_json::{Value, json};

use crate::support::*;

#[tokio::test(flavor = "current_thread")]
async fn no_socket_and_a_dead_socket_are_not_running() {
    let dir = scratch_dir("hn");
    let missing = ServerId::new(dir.join("none"));
    assert_eq!(
        Herdr::new(missing.clone()).snapshot().await,
        Err(HerdrError::NotRunning)
    );
    assert_eq!(
        blocking::Herdr::new(missing).snapshot(),
        Err(HerdrError::NotRunning)
    );
    let dead = dir.join("dead");
    drop(std::os::unix::net::UnixListener::bind(&dead).unwrap()); // the file stays, nobody listens
    // a child another test forked just then holds the listener until its exec
    let started = Instant::now();
    while std::os::unix::net::UnixStream::connect(&dead).is_ok() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let dead = ServerId::new(&dead);
    assert_eq!(
        Herdr::new(dead.clone()).panes().await,
        Err(HerdrError::NotRunning)
    );
    assert_eq!(
        blocking::Herdr::new(dead).panes(),
        Err(HerdrError::NotRunning)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// A socket this process may not connect to is unreachable, not "gone"
/// (herdr-inbox tests/test_successor.py: only herdr's own word is "gone").
#[tokio::test(flavor = "current_thread")]
async fn a_forbidden_socket_is_unreachable_not_gone() {
    let fake = FakeHerdr::recorded();
    std::fs::set_permissions(fake.socket(), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::os::unix::net::UnixStream::connect(fake.socket()).is_ok() {
        return; // root ignores the mode: nothing to test
    }
    let err = fake.client().panes().await.unwrap_err();
    assert!(matches!(err, HerdrError::Unreachable(_)), "{err:?}");
    assert!(!err.maybe_done());
    let err = fake.blocking_client().panes().unwrap_err();
    assert!(matches!(err, HerdrError::Unreachable(_)), "{err:?}");
}

/// panorama tests/test_tui.py::test_a_reply_that_is_missing_or_garbled_after_sending_is_no_reply
#[tokio::test(flavor = "current_thread")]
async fn a_missing_or_garbled_reply_after_sending_may_have_happened() {
    let fake = FakeHerdr::recorded();
    let h = fake.client();
    let b = fake.blocking_client();
    let tab = t("w1:t3");
    let replies = [
        Reply::Close,
        Reply::Raw(b"not json\n".to_vec()),
        Reply::Raw(b"[1]\n".to_vec()),
    ];
    for reply in replies {
        fake.on("tab.move", reply.clone());
        let a = h.move_tab(&tab, 0).await.unwrap_err();
        let s = b.move_tab(&tab, 0).unwrap_err();
        for err in [a, s] {
            assert!(err.maybe_done(), "{reply:?}: {err:?}");
            match reply {
                Reply::Close => assert!(matches!(err, HerdrError::NoReply(_))),
                _ => assert!(matches!(err, HerdrError::Garbled(_))),
            }
        }
    }
    assert_eq!(fake.params_of("tab.move").len(), 6, "each one went out");
}

#[tokio::test(flavor = "current_thread")]
async fn an_unanswered_request_times_out_as_no_reply() {
    let fake = FakeHerdr::recorded();
    fake.on("agent.prompt", Reply::Hang);
    let limit = Duration::from_millis(200);
    let hi = text("hi");
    let started = Instant::now();
    let err = fake
        .client()
        .with_timeout(limit)
        .prompt(&p("w1:p2"), &hi)
        .await;
    assert_eq!(err, Err(HerdrError::NoReply("timed out".into())));
    let err = fake
        .blocking_client()
        .with_timeout(limit)
        .prompt(&p("w1:p2"), &hi);
    assert_eq!(err, Err(HerdrError::NoReply("timed out".into())));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test(flavor = "current_thread")]
async fn once_replies_come_first_then_the_recordings() {
    let fake = FakeHerdr::recorded();
    fake.once("agent.get", Reply::error("agent_not_found", "gone"));
    let h = fake.client().with_client("herdr-inbox");
    assert_eq!(h.agent(&p("w1:p2")).await, Ok(None));
    assert!(h.agent(&p("w1:p2")).await.unwrap().is_some());
    let ids: Vec<Value> = fake.requests().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, [json!("herdr-inbox:1"), json!("herdr-inbox:1")]);
}

#[test]
fn the_fake_answers_a_garbled_line_like_herdr_and_records_it() {
    use std::io::{BufRead, BufReader, Write};
    let fake = FakeHerdr::start();
    let mut conn = std::os::unix::net::UnixStream::connect(fake.socket()).unwrap();
    conn.write_all(b"{not json\n").unwrap();
    let mut line = String::new();
    BufReader::new(&conn).read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    let got = (reply["id"].as_str(), reply["error"]["code"].as_str());
    assert_eq!(got, (Some(""), Some("invalid_request")));
    let req = fake.requests().pop().unwrap();
    assert_eq!((req.method.as_str(), req.line.as_str()), ("", "{not json"));
    // nothing scripted or recorded: refused, never a made-up success
    let err = fake.blocking_client().panes().unwrap_err();
    assert!(err.is_code("invalid_request"));
}

/// Review L1: a herdr that stopped accepting (its backlog full) must not
/// hang the blocking client's connect.
#[cfg(target_os = "linux")]
#[test]
fn a_blocking_connect_to_a_wedged_server_times_out() {
    let dir = scratch_dir("hw");
    let path = dir.join("s");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    // fill the backlog: the filler blocks on the first connect that does not fit
    let filler = {
        let path = path.clone();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            while let Ok(s) = std::os::unix::net::UnixStream::connect(&path) {
                held.push(s);
            }
            held.len()
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    let err = blocking::Herdr::new(ServerId::new(&path))
        .with_timeout(Duration::from_millis(300))
        .panes()
        .unwrap_err();
    assert_eq!(err, HerdrError::Unreachable("connect timed out".into()));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(listener);
    std::fs::remove_file(&path).unwrap();
    assert!(filler.join().unwrap() > 0);
    std::fs::remove_dir_all(dir).unwrap();
}
