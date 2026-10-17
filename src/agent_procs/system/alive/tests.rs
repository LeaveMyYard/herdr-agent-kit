use std::process::Command;

use super::*;

#[test]
fn a_gone_process_is_dead_and_another_namespace_is_unknown() {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = child.id();
    let start = start_token(pid).unwrap_or_default();
    child.wait().unwrap();
    // reaped: no process has this pid (until it is reused, which a start token catches)
    assert_eq!(
        process_alive(&ProcessIdentity::new(pid, start, None)),
        Some(false)
    );
    assert!(!pid_alive(Some(pid)));
    let me = std::process::id();
    let mine = start_token(me).unwrap();
    assert_eq!(
        process_alive(&ProcessIdentity::new(
            me,
            mine.clone(),
            Some("pid:[1]".into())
        )),
        None
    );
    assert_eq!(
        process_alive(&ProcessIdentity::new(me, mine, own_pidns())),
        Some(true)
    );
    assert!(pid_alive(Some(me)));
    assert!(pid_alive(None));
    assert!(pid_alive(Some(0)));
}

#[test]
fn a_process_we_may_not_signal_is_alive_by_its_start_token() {
    // pid 1 belongs to root: kill says EPERM (unless the tests run as root), the token decides
    let Some(init) = start_token(1) else { return };
    assert_eq!(
        process_alive(&ProcessIdentity::new(1, init.clone(), None)),
        Some(true)
    );
    assert_eq!(
        process_alive(&ProcessIdentity::new(1, format!("{init}x"), None)),
        Some(false)
    );
    assert!(pid_alive(Some(1)));
}
