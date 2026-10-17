//! The reconnecting event stream.

use std::time::{Duration, Instant};

use herdr_agent_kit::herdr_api::fake::{self, FakeHerdr, Reply};
use herdr_agent_kit::herdr_api::{EventKind, Events, Herdr, HerdrEvent, ServerId};
use serde_json::{Value, json};

use crate::support::*;

async fn next(events: &mut Events) -> HerdrEvent {
    tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .expect("an event within 5 s")
}

fn focus_kinds() -> [EventKind; 2] {
    [EventKind::TAB_FOCUSED, EventKind::PANE_FOCUSED]
}

#[test]
fn the_subscription_is_the_one_panorama_sends() {
    let (params, _) = exchange("events.jsonl", 2, "events.subscribe");
    let h = Herdr::new(ServerId::new("/nowhere")).with_client("panorama");
    let events = h.events(&focus_kinds());
    let sent: Value = serde_json::from_slice(events.request()).unwrap();
    let want = json!({"id": "panorama:events", "method": "events.subscribe", "params": params});
    assert_eq!(sent, want);
}

/// panorama tests/test_follow.py::test_event_stream_yields_focus_events_and_reconnects
/// and ::test_r12f_l1_events_marks_connect_and_disconnect
#[tokio::test(flavor = "current_thread")]
async fn events_mark_connect_and_loss_and_reconnect() {
    let fake = FakeHerdr::recorded();
    let mut events = fake.client().events(&focus_kinds());
    assert_eq!(next(&mut events).await, HerdrEvent::Connected);
    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
    // the recorded events, as herdr sent them, each after a line that is not JSON
    let record = fake::recording("events.jsonl", 2).unwrap();
    let recorded: Vec<Value> = record["replies"].as_array().unwrap()[1..].to_vec();
    for frame in &recorded {
        fake.send_line("not json");
        fake.send_line(&frame.to_string());
    }
    for frame in &recorded {
        let HerdrEvent::Event(e) = next(&mut events).await else {
            panic!("an event")
        };
        assert_eq!(Value::Object(e.data.clone()), frame["data"]);
        assert_eq!(e.kind, frame["event"].as_str().unwrap());
    }
    fake.drop_subscribers();
    assert_eq!(next(&mut events).await, HerdrEvent::Lost);
    assert_eq!(
        next(&mut events).await,
        HerdrEvent::Connected,
        "after a reconnect"
    );
    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
    fake.emit(json!({"type": "tab_focused", "tab_id": "w1:t1", "workspace_id": "w1"}));
    let HerdrEvent::Event(e) = next(&mut events).await else {
        panic!("an event")
    };
    assert_eq!(e.tab_id(), Some(t("w1:t1")));
    let subs = fake.params_of("events.subscribe");
    assert_eq!(subs.len(), 2);
    let want = json!({"subscriptions": [{"type": "tab.focused"}, {"type": "pane.focused"}]});
    assert_eq!(subs[0], want);
}

/// A refused subscription is not live: no `Connected`, and herdr keeps the
/// connection open, so nothing more comes (panorama's behaviour).
#[tokio::test(flavor = "current_thread")]
async fn a_refused_subscription_is_not_live() {
    let fake = FakeHerdr::recorded();
    let (_, frame) = exchange("errors.jsonl", 33, "events.subscribe");
    let (code, message) = (
        frame["error"]["code"].as_str(),
        frame["error"]["message"].as_str(),
    );
    fake.once(
        "events.subscribe",
        Reply::error(code.unwrap(), message.unwrap()),
    );
    let mut events = fake.client().events(&[EventKind::new("no.such.event")]);
    let quiet = tokio::time::timeout(Duration::from_millis(300), events.next()).await;
    assert!(quiet.is_err(), "nothing: {quiet:?}");
    assert_eq!(fake.subscribers(), 0);
}

/// No server: the stream keeps trying, quietly (no `Lost` for a stream that
/// was never live).
#[tokio::test(flavor = "current_thread")]
async fn events_without_a_server_stay_quiet() {
    let dir = scratch_dir("he");
    let mut events = Herdr::new(ServerId::new(dir.join("none"))).events(&focus_kinds());
    let quiet = tokio::time::timeout(Duration::from_millis(300), events.next()).await;
    assert!(quiet.is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review B1: `next()` sits in a `select!` beside a fast tick (plan §2.5);
/// each tick drops the pending `next()`. The reconnect wait must survive
/// that, or the stream never comes back after a loss.
#[tokio::test(flavor = "current_thread")]
async fn events_reconnect_inside_a_select_with_a_fast_tick() {
    let fake = FakeHerdr::recorded();
    let mut events = fake.client().events(&focus_kinds());
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut seen = Vec::new();
    let started = Instant::now();
    while seen.iter().filter(|e| **e == HerdrEvent::Connected).count() < 2 {
        assert!(started.elapsed() < Duration::from_secs(5), "seen {seen:?}");
        tokio::select! {
            event = events.next() => {
                if event == HerdrEvent::Connected && seen.is_empty() {
                    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
                    fake.drop_subscribers();
                }
                seen.push(event);
            }
            _ = tick.tick() => {}
        }
    }
    assert_eq!(
        seen,
        [
            HerdrEvent::Connected,
            HerdrEvent::Lost,
            HerdrEvent::Connected
        ]
    );
}
