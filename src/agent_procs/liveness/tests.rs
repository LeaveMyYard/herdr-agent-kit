use super::*;

#[test]
fn no_procs_knows_nothing() {
    let id = ProcessIdentity::new(std::process::id(), "x", None);
    assert_eq!(NoProcs.process_alive(&id), None);
    assert_eq!(NoProcs.task_alive("b1"), None);
}
