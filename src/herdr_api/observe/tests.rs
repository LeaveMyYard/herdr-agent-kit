use super::*;
use std::sync::Mutex;

#[test]
fn nobody_watching_costs_no_clock_and_reports_nothing() {
    let w = Watch::default();
    assert!(w.start().is_none());
    w.report(None, Transport::Socket, || {
        unreachable!("no op is made for nobody")
    });
}

#[test]
fn a_watched_call_is_reported_once_with_its_op_and_time() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let w = Watch(Some(Arc::new(move |t: &CallTrace| {
        log.lock().unwrap().push(t.clone())
    })));
    let before = SystemTime::now();
    let timing = w.start();
    std::thread::sleep(Duration::from_millis(20));
    w.report(timing, Transport::Cli, || "api snapshot".into());
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
    let timing = w.start();
    w.report(timing, Transport::Socket, || "x".into()); // returns normally
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
