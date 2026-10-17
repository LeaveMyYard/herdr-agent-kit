use super::*;
use crate::agent_procs::fake::FakeProcesses;

fn chain(n: Pid) -> FakeProcesses {
    // n -> n-1 -> … -> 2 -> 1, none of them Claude
    let mut t = FakeProcesses::default();
    for pid in 2..=n {
        t.insert(pid, pid - 1, "sh", "s", &["sh"]);
    }
    t
}

#[test]
fn a_walk_stops_after_max_depth() {
    let depth = MAX_DEPTH as Pid;
    let mut t = chain(100);
    // the 33rd process up from 100 is out of reach
    t.insert(100 - depth, 99 - depth, "claude", "c", &["claude"]);
    assert_eq!(claude_process(&t, 100), None);
    assert!(!nested_claude(&t, 100));
    assert_eq!(owning_claude(&t, 100), Some(99)); // the grandparent fallback
    // the 32nd is the last one looked at
    t.insert(101 - depth, 100 - depth, "claude", "c", &["claude"]);
    assert_eq!(claude_process(&t, 100), Some((101 - depth, "c".into())));
    assert_eq!(owning_claude(&t, 100), Some(101 - depth));
}

#[test]
fn nested_claude_looks_at_32_processes() {
    let depth = MAX_DEPTH as Pid;
    let mut t = chain(100);
    t.insert(100, 99, "claude", "a", &["claude"]);
    t.insert(100 - depth, 99 - depth, "claude", "c", &["claude"]); // the 33rd
    assert!(!nested_claude(&t, 100));
    t.insert(101 - depth, 100 - depth, "claude", "c", &["claude"]); // the 32nd
    assert!(nested_claude(&t, 100));
}

#[test]
fn pid_1_is_never_a_claude() {
    // a container whose init is claude: hooks below a pane's claude
    let mut t = FakeProcesses::default();
    t.insert(1, 0, "claude", "init", &["claude"]);
    t.insert(30, 1, "zsh", "z", &["zsh"]);
    t.insert(20, 30, "sh", "s", &["sh"]);
    t.insert(10, 20, "python3", "p", &["python3", "hook.py"]);
    assert_eq!(claude_process(&t, 10), None);
    assert_eq!(owning_claude(&t, 10), Some(20)); // the grandparent, not init
    assert!(!nested_claude(&t, 10));
    t.insert(30, 1, "claude", "pane", &["claude"]);
    assert_eq!(claude_process(&t, 10), Some((30, "pane".into())));
    assert_eq!(owning_claude(&t, 10), Some(30));
    assert!(!nested_claude(&t, 10)); // one Claude, not two
    // a walk that starts at init stops at once
    assert_eq!(claude_process(&t, 1), None);
    assert_eq!(owning_claude(&t, 1), None);
}

#[test]
fn a_claude_without_a_start_token_is_not_recorded() {
    let mut t = FakeProcesses::default();
    t.insert(10, 1, "claude", "a", &["claude"]);
    t.forget_start(10);
    assert_eq!(claude_process(&t, 10), None);
    assert_eq!(owning_claude(&t, 10), Some(10));
}

#[test]
fn two_claudes_are_nested_and_the_nearest_owns() {
    // a real nested run: claude(child) -> timeout -> zsh -> claude(pane agent) -> herdr
    let mut t = FakeProcesses::default();
    t.insert(10, 9, "python3", "a", &["python3", "hook.py"]);
    t.insert(
        9,
        8,
        "2.1.301",
        "b",
        &["/u/.local/share/claude/versions/2.1.301", "-p"],
    );
    t.insert(8, 7, "timeout", "c", &["timeout", "60", "claude"]);
    t.insert(7, 6, "zsh", "d", &["zsh"]);
    t.insert(6, 5, "claude", "e", &["claude"]);
    t.insert(5, 1, "herdr", "f", &["herdr"]);
    assert!(nested_claude(&t, 10));
    assert_eq!(owning_claude(&t, 10), Some(9));
    assert_eq!(claude_process(&t, 10), Some((9, "b".into())));
    assert!(!nested_claude(&t, 7));
    assert_eq!(owning_claude(&t, 7), Some(6));
}

#[test]
fn an_unreadable_tree_is_not_nested_and_has_no_owner() {
    let t = FakeProcesses::default();
    assert!(!nested_claude(&t, 10));
    assert_eq!(owning_claude(&t, 10), None);
    assert_eq!(claude_process(&t, 10), None);
    assert_eq!(owning_claude(&t, 1), None);
    assert_eq!(owning_claude(&t, 0), None);
}

#[test]
fn the_owner_fallback_is_never_init() {
    let mut t = FakeProcesses::default();
    t.insert(10, 1, "sh", "a", &["sh"]);
    assert_eq!(owning_claude(&t, 10), None);
    t.insert(10, 9, "sh", "a", &["sh"]);
    assert_eq!(owning_claude(&t, 10), Some(9)); // 9 itself unreadable: still the fallback
}

#[test]
fn a_claude_named_by_comm_needs_no_argv_read() {
    let mut t = FakeProcesses::default();
    t.insert(10, 1, "claude", "a", &[] as &[&str]);
    assert_eq!(claude_process(&t, 10), Some((10, "a".into())));
    assert_eq!(t.cmdline_reads(), 0);
    t.insert(11, 10, "sh", "b", &["sh"]);
    assert_eq!(claude_process(&t, 11), Some((10, "a".into())));
    assert_eq!(t.cmdline_reads(), 1);
}
