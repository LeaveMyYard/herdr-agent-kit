use super::*;
use std::sync::Mutex;

fn recorded() -> (Watch, Arc<Mutex<Vec<CallTrace>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let observer: Observer = Arc::new(move |t: &CallTrace| log.lock().unwrap().push(t.clone()));
    (Watch(Some(observer)), seen)
}

#[test]
fn nobody_watching_makes_nothing() {
    let w = Watch::default();
    assert!(
        w.start(Transport::Socket, || unreachable!(
            "no op is made for nobody"
        ))
        .is_none()
    );
}

#[test]
fn a_call_is_heard_once_when_it_ends_with_its_op_and_time() {
    let (w, seen) = recorded();
    let before = SystemTime::now();
    let call = w.start(Transport::Cli, || "api snapshot".into());
    std::thread::sleep(Duration::from_millis(20));
    assert!(seen.lock().unwrap().is_empty(), "heard before it ended");
    drop(call);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].transport, seen[0].op.as_str()),
        (Transport::Cli, "api snapshot")
    );
    assert!(seen[0].took >= Duration::from_millis(20));
    assert!(seen[0].started >= before);
}

#[test]
fn a_panicking_observer_never_reaches_the_call() {
    let w = Watch(Some(Arc::new(|_: &CallTrace| panic!("observer broke"))));
    drop(w.start(Transport::Socket, || "x".into())); // returns normally
}

#[test]
fn the_cli_op_is_its_first_two_arguments() {
    let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(cli_op(&args(&["api", "snapshot"])), "api snapshot");
    assert_eq!(
        cli_op(&args(&["--machine", "m1", "agent", "focus", "w1:p1"])),
        "--machine m1"
    );
    assert_eq!(cli_op(&args(&["machine"])), "machine");
}
