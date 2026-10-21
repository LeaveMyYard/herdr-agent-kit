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
