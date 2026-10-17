//! The inbox's process-walk tests, ported: herdr-inbox
//! `tests/test_bridge_inbox.py` (nested Claude of any install) and
//! `tests/test_rereview_fixes.py::test_nested_detection`.

use herdr_agent_kit::agent_procs::fake::FakeProcesses;
use herdr_agent_kit::agent_procs::{Pid, nested_claude, owning_claude};

type Proc = (&'static str, &'static [&'static str]);

const VERSIONED: Proc = ("2.1.301", &["/home/u/.local/share/claude/versions/2.1.301"]);
const NODE: Proc = (
    "node",
    &[
        "node",
        "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
    ],
);
const NATIVE: Proc = ("claude", &["claude"]);
const SHELL: Proc = ("zsh", &["zsh"]);

/// Our parent is 100; `chain` lists the processes from there up (pid 100 + i,
/// parent 101 + i); the one above the last is unreadable.
fn tree(chain: &[Proc]) -> FakeProcesses {
    let mut t = FakeProcesses::default();
    for (i, (comm, argv)) in chain.iter().enumerate() {
        let pid = 100 + i as Pid;
        t.insert(pid, pid + 1, comm, "s", argv);
    }
    t
}

/// `test_a_nested_claude_is_found_whatever_the_install` (§1.3 row 5:
/// `comm == "claude"` missed versioned binaries and node installs).
#[test]
fn test_a_nested_claude_is_found_whatever_the_install() {
    for (inner, outer) in [
        (VERSIONED, NATIVE),
        (NATIVE, VERSIONED),
        (NODE, NODE),
        (VERSIONED, VERSIONED),
    ] {
        let t = tree(&[SHELL, inner, SHELL, SHELL, outer, SHELL]);
        assert!(nested_claude(&t, 100), "{inner:?} in {outer:?}");
        // the nearest Claude, not the shell above us
        assert_eq!(owning_claude(&t, 100), Some(101));
    }
}

/// `test_one_claude_of_any_install_is_not_nested`
#[test]
fn test_one_claude_of_any_install_is_not_nested() {
    for only in [VERSIONED, NODE, NATIVE] {
        let t = tree(&[SHELL, only, SHELL]);
        assert!(!nested_claude(&t, 100), "{only:?}");
        assert_eq!(owning_claude(&t, 100), Some(101));
    }
}

/// `test_no_claude_above_falls_back_to_the_grandparent`
#[test]
fn test_no_claude_above_falls_back_to_the_grandparent() {
    let t = tree(&[SHELL, ("bash", &["bash", "-c", "claude"]), SHELL]);
    assert!(!nested_claude(&t, 100));
    assert_eq!(owning_claude(&t, 100), Some(101));
}

/// `test_the_argv_is_read_only_for_a_process_not_named_claude`
#[test]
fn test_the_argv_is_read_only_for_a_process_not_named_claude() {
    let t = tree(&[NATIVE, SHELL]);
    assert_eq!(owning_claude(&t, 100), Some(100));
    assert_eq!(t.cmdline_reads(), 0);
}

/// `test_nested_detection`: pid → (parent, comm); argv unknown (empty).
#[test]
fn test_nested_detection() {
    fn table(rows: &[(Pid, Pid, &str)]) -> FakeProcesses {
        let mut t = FakeProcesses::default();
        for &(pid, ppid, comm) in rows {
            t.insert(pid, ppid, comm, "s", &[] as &[&str]);
        }
        t
    }
    let top = [
        (100, 50, "zsh"),
        (50, 40, "claude"),
        (40, 30, "zsh"),
        (30, 1, "herdr"),
    ];
    let nested = [
        (200, 150, "uv"),
        (150, 140, "claude"),
        (140, 130, "timeout"),
        (130, 120, "zsh"),
        (120, 50, "zsh"),
        (50, 40, "claude"),
        (40, 30, "zsh"),
        (30, 1, "herdr"),
    ];
    assert!(!nested_claude(&table(&top), 100));
    assert!(nested_claude(&table(&nested), 200));
    // npm/npx wrappers or an unreadable tree: never claim nested
    let wrappers = [
        (100, 60, "node"),
        (60, 50, "claude"),
        (50, 40, "npx"),
        (40, 1, "zsh"),
    ];
    assert!(!nested_claude(&table(&wrappers), 100));
    assert!(!nested_claude(&table(&[]), 100));
}
