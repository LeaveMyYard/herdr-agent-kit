//! The reconnect backoff (panorama tests/test_follow.py
//! `test_r12_l1_backoff_grows_until_data_arrives`): each connection that
//! ends or fails waits longer, up to the cap.

use super::*;

#[test]
fn the_backoff_grows_until_it_is_capped() {
    let mut events = Events::new(PathBuf::from("unused"), "t", &[]);
    let mut waits = Vec::new();
    for _ in 0..7 {
        waits.push(events.backoff.as_secs_f64());
        events.back_off();
    }
    assert_eq!(waits, [0.5, 1.0, 2.0, 4.0, 8.0, 10.0, 10.0]); // never stuck at 0.5
}

/// Recorded reply `n` to `events.subscribe` (fixtures/herdr/events.jsonl, seq 2), as a line.
fn captured(n: usize) -> Vec<u8> {
    let text = include_str!("../../../../fixtures/herdr/events.jsonl");
    let record: serde_json::Value = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|r| r["seq"] == 2)
        .expect("the recorded subscribe");
    serde_json::to_vec(&record["replies"][n]).unwrap()
}

#[test]
fn data_from_herdr_resets_the_backoff_and_nothing_else_does() {
    // panorama's backoff "grows until data arrives": herdr's ack or an event proves the
    // connection and the next failure waits FIRST_BACKOFF again; a line that proves nothing
    // (no result, an empty one, empty data, not JSON) changes nothing
    let (ack, event) = (captured(0), captured(1));
    assert!(String::from_utf8_lossy(&ack).contains("subscription_started"));
    assert!(String::from_utf8_lossy(&event).contains("tab_focused"));
    let mut events = Events::new(PathBuf::from("unused"), "t", &[]);
    for _ in 0..4 {
        events.back_off();
    }
    assert_eq!(events.backoff.as_secs_f64(), 8.0);
    for nothing in [
        &br#"{"result": null}"#[..],
        br#"{"result": {}}"#,
        br#"{"data": {}}"#,
        b"not json",
    ] {
        events.handle(nothing);
        assert_eq!(
            events.backoff.as_secs_f64(),
            8.0,
            "{}",
            String::from_utf8_lossy(nothing)
        );
    }
    events.handle(&ack);
    assert_eq!(
        events.backoff, FIRST_BACKOFF,
        "the subscription's ack resets it"
    );
    for _ in 0..3 {
        events.back_off();
    }
    events.handle(&event);
    assert_eq!(events.backoff, FIRST_BACKOFF, "an event resets it");
}

#[tokio::test(flavor = "current_thread")]
async fn a_connection_closed_before_any_ack_keeps_the_backoff_growing() {
    // Python's server accepts, then closes before any ack (an older herdr, a shutdown): a
    // connect that succeeds proves nothing, so the waits keep growing (0.5 s, then 1 s, ...)
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("herdr.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = accepted.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // the subscribe request is read, then the connection closes before any reply
            let mut conn = conn.unwrap();
            let mut line = Vec::new();
            let _ = std::io::BufRead::read_until(
                &mut std::io::BufReader::new(&mut conn),
                b'\n',
                &mut line,
            );
            drop(conn);
        }
    });
    let mut events = Events::new(socket, "t", &[EventKind::TAB_FOCUSED]);
    // the first connection at once, the second after 0.5 s; at 1.2 s the stream waits 1 s more
    let _ = tokio::time::timeout(Duration::from_millis(1200), events.next()).await;
    assert_eq!(accepted.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(
        events.backoff.as_secs_f64(),
        2.0,
        "a bare connect reset the backoff"
    );
}
