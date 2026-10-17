use super::*;

#[test]
fn task_ids_from_output_paths() {
    assert_eq!(
        task_of("/tmp/claude-1000/p/s/tasks/b1x2.output"),
        Some("b1x2")
    );
    assert_eq!(
        task_of("/tmp/claude-1000/p/s/tasks//b1x2.output"),
        Some("b1x2")
    );
    assert_eq!(task_of("tasks/b1.output"), Some("b1"));
    assert_eq!(task_of("/x/tasks/.output"), None);
    assert_eq!(task_of("/x/tasks/b1.output (deleted)"), None);
    assert_eq!(task_of("/x/task/b1.output"), None);
    assert_eq!(task_of("/x/tasks/b1.out"), None);
    assert_eq!(task_of("b1.output"), None);
}
