use super::follower::in_subagents_dir;
use super::*;
use std::path::{Path, PathBuf};

#[test]
fn partial_line_is_buffered_until_its_newline() {
    let mut t = Tailer::new();
    assert_eq!(
        t.consume(b"{\"a\":1}\n{\"b\":"),
        vec![b"{\"a\":1}".to_vec()]
    );
    assert_eq!(t.consume(b"2}\r\n\n  \n"), vec![b"{\"b\":2}".to_vec()]);
}

#[test]
fn runaway_line_is_dropped_and_reading_resyncs() {
    let mut t = Tailer::new();
    let big = vec![b'x'; MAX_PARTIAL + 1];
    assert!(t.consume(&big).is_empty());
    assert!(t.consume(b"xx").is_empty());
    assert_eq!(t.consume(b"yyy\nok\n"), vec![b"ok".to_vec()]);
}

#[test]
fn a_partial_line_at_the_cap_is_kept() {
    let mut t = Tailer::new();
    assert!(t.consume(&vec![b'x'; MAX_PARTIAL]).is_empty());
    assert_eq!(t.consume(b"\n"), vec![vec![b'x'; MAX_PARTIAL]]);
}

#[test]
fn projects_root_resolution() {
    let home = Some(PathBuf::from("/home/u"));
    assert_eq!(
        ProjectsRoot::resolve(None, home.clone()).unwrap().path(),
        Path::new("/home/u/.claude/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("".into()), home.clone())
            .unwrap()
            .path(),
        Path::new("/home/u/.claude/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("/cfg".into()), home)
            .unwrap()
            .path(),
        Path::new("/cfg/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("/cfg".into()), None)
            .unwrap()
            .path(),
        Path::new("/cfg/projects")
    );
    assert_eq!(ProjectsRoot::resolve(None, None), None);
}

#[test]
fn sanitized_project_names() {
    assert_eq!(sanitize_cwd("/home/user/project"), "-home-user-project");
    assert_eq!(sanitize_cwd("/a b/é_x.y"), "-a-b---x-y");
}

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
