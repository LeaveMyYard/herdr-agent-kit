use super::*;

#[test]
fn tasks_are_unknown_without_a_claude_process() {
    assert_eq!(SystemLiveness::new(None).task_alive("b1"), None);
}

#[test]
fn this_process_is_alive_and_a_reused_pid_is_not_it() {
    let me = std::process::id();
    let start = system::start_token(me).expect("own start token");
    let live = SystemLiveness::new(None);
    assert_eq!(
        live.process_alive(&ProcessIdentity::new(me, start.clone(), None)),
        Some(true)
    );
    assert_eq!(
        live.process_alive(&ProcessIdentity::new(me, format!("{start}0"), None)),
        Some(false)
    );
    // cached: the same answer again without a second read
    assert_eq!(live.processes.borrow().len(), 2);
    assert_eq!(
        live.process_alive(&ProcessIdentity::new(me, start, None)),
        Some(true)
    );
    assert_eq!(live.processes.borrow().len(), 2);
}
