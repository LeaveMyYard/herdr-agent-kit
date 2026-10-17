use super::*;

#[test]
fn the_fake_liveness_answers_only_what_it_was_told() {
    let id = ProcessIdentity::new(80, "s3", None);
    let other = ProcessIdentity::new(80, "s4", None);
    let live = FakeLiveness::default().process(id.clone(), false);
    assert_eq!(live.process_alive(&id), Some(false));
    assert_eq!(live.process_alive(&other), None);
    assert_eq!(live.task_alive("b1"), None);
    let live = live.tasks(&["b1"]);
    assert_eq!(live.task_alive("b1"), Some(true));
    assert_eq!(live.task_alive("b2"), Some(false));
}
