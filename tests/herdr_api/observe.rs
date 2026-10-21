//! A client's observer hears each socket request and CLI run, through both clients and their
//! clones, failures too; a panicking observer never breaks the call.

use std::sync::{Arc, Mutex};

use herdr_agent_kit::herdr_api::fake::{CliReply, CliStub, FakeHerdr, Reply};
use herdr_agent_kit::herdr_api::{CallTrace, Herdr, Observer, ServerId, Transport};
use serde_json::json;

/// What the observer heard: each call's transport and op.
type Heard = Arc<Mutex<Vec<(Transport, String)>>>;

fn recorder() -> (Observer, Heard) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let obs: Observer =
        Arc::new(move |t: &CallTrace| log.lock().unwrap().push((t.transport, t.op.clone())));
    (obs, seen)
}

#[tokio::test(flavor = "current_thread")]
async fn every_socket_request_is_heard_through_both_clients() {
    let fake = FakeHerdr::start();
    fake.on(
        "session.snapshot",
        Reply::Result(json!({"snapshot": {"panes": []}})),
    );
    let (obs, seen) = recorder();
    let h = fake.client().with_observer(obs);
    h.raw("session.snapshot", json!({})).await.unwrap();
    let b = h.blocking(); // the blocking client of the same target keeps the observer
    tokio::task::spawn_blocking(move || b.raw("session.snapshot", json!({})).unwrap())
        .await
        .unwrap();
    let _ = h.raw("no.such.method", json!({})).await; // a failed call is heard too
    let seen = seen.lock().unwrap();
    let ops: Vec<_> = seen.iter().map(|(t, op)| (*t, op.as_str())).collect();
    assert_eq!(
        ops,
        [
            (Transport::Socket, "session.snapshot"),
            (Transport::Socket, "session.snapshot"),
            (Transport::Socket, "no.such.method")
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_cli_run_is_heard_by_its_first_two_arguments() {
    let mut stub = CliStub::new();
    stub.on(&["--machine", "m1", "api", "snapshot"], CliReply::ok("{}"));
    let (obs, seen) = recorder();
    let h = Herdr::new(ServerId::new("/nowhere"))
        .with_bin(stub.bin())
        .with_observer(obs);
    let _ = h.machine_snapshot("m1").await;
    let b = h.blocking();
    tokio::task::spawn_blocking(move || {
        let _ = b.machine_snapshot("m1");
    })
    .await
    .unwrap();
    let cli = (Transport::Cli, "--machine m1".to_owned());
    assert_eq!(*seen.lock().unwrap(), [cli.clone(), cli]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_panicking_observer_never_breaks_the_call() {
    let fake = FakeHerdr::start();
    fake.on(
        "session.snapshot",
        Reply::Result(json!({"snapshot": {"panes": []}})),
    );
    let h = fake
        .client()
        .with_observer(Arc::new(|_: &CallTrace| panic!("the observer broke")));
    assert!(h.raw("session.snapshot", json!({})).await.is_ok());
}
