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

#[test]
fn data_from_herdr_resets_the_backoff_and_nothing_else_does() {
    // panorama's backoff "grows until data arrives": an ack or an event proves the connection
    // and the next failure waits FIRST_BACKOFF again; a line that proves nothing changes nothing
    let mut events = Events::new(PathBuf::from("unused"), "t", &[]);
    for _ in 0..4 {
        events.back_off();
    }
    assert_eq!(events.backoff.as_secs_f64(), 8.0);
    events.handle(br#"{"id": "t:events", "result": null}"#);
    events.handle(b"not json");
    assert_eq!(
        events.backoff.as_secs_f64(),
        8.0,
        "nothing arrived: still growing"
    );
    events.handle(br#"{"id": "t:events", "result": {"ok": true}}"#);
    assert_eq!(
        events.backoff, FIRST_BACKOFF,
        "the subscription's ack resets it"
    );
    for _ in 0..3 {
        events.back_off();
    }
    events.handle(br#"{"data": {"type": "tab.focused"}}"#);
    assert_eq!(events.backoff, FIRST_BACKOFF, "an event resets it");
}
