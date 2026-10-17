use super::*;

#[test]
fn subagent_dirs_are_recognized() {
    assert!(in_subagents_dir(Path::new(
        "/r/p/s/subagents/agent-a.jsonl"
    )));
    assert!(in_subagents_dir(Path::new(
        "/r/p/s/subagents/workflows/w1/agent-a.jsonl"
    )));
    assert!(!in_subagents_dir(Path::new("/subagents/w/x/p/s.jsonl")));
    assert!(!in_subagents_dir(Path::new("/r/p/s.jsonl")));
}
