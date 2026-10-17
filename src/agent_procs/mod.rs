//! Agent processes. **IO** (reads the process table).
//!
//! One implementation, for both plugins, of three questions about processes:
//!
//! - **Is this process Claude Code?** [`is_claude`]: a native install (the
//!   `claude` binary, or a versioned binary under `…/claude/versions/`), or
//!   node running the `@anthropic-ai/claude-code` entry script. The `npx`
//!   launcher above such a node process is *not* Claude (plan §1.3 row 5: the
//!   inbox used to check only `comm == "claude"`).
//! - **Which Claude does this process serve, and is it nested in another
//!   Claude?** [`claude_process`], [`owning_claude`] and [`nested_claude`]
//!   walk up the process tree through a [`ProcessTable`]: the real one is
//!   [`SystemProcesses`] (procfs, or `ps` where there is no procfs, as on
//!   macOS), the test one is
//!   `fake::FakeProcesses` (feature `test-support`).
//! - **Is a recorded process still alive?** The [`Liveness`] trait, which
//!   [`crate::agent_session::claude::fold`]'s `status_of` takes as an argument so the
//!   status rules stay pure. [`SystemLiveness`] reads the real process table,
//!   [`NoProcs`] answers "unknown" everywhere (transcripts mirrored from
//!   another machine), and `fake::FakeLiveness` (feature
//!   `test-support`) is scripted for tests.
//!
//! Ported from herdr-panorama `hooks.py` (`is_claude`, `claude_process`,
//! `process_alive`, `own_pidns`) and `bgproc.py`, and herdr-inbox `proc.py`
//! (`nested_claude`, `owning_claude`, `alive`), both on their `main` after the
//! P0b bridge releases. The shared case table is `tests/test_bridge_rules.py`
//! in both plugins; `tests/bridge_rules_is_claude.rs` here is its port.
//!
//! Anything that cannot be read counts as *unknown*, never as "gone" or
//! "nested": an unreadable tree is not nested (the inbox never switches itself
//! off by mistake), and an unreadable process is not called dead.

mod liveness;
mod system;

#[cfg(any(test, feature = "test-support"))]
pub mod fake;

pub use liveness::{Liveness, NoProcs, ProcessIdentity, SystemLiveness};
pub use system::{
    SystemProcesses, descendants, live_tasks, own_pidns, parent_pid, pid_alive, process_alive,
    start_token, task_of,
};

/// A process id, as the plugins record it.
pub type Pid = u32;

/// How far up the process tree a walk goes before it gives up.
pub const MAX_DEPTH: usize = 32;

/// The entry scripts of the `@anthropic-ai/claude-code` npm package. A path
/// must end with one of these, so the scope is a whole path segment.
pub const CLAUDE_ENTRY: [&str; 2] = [
    "/@anthropic-ai/claude-code/cli.js",
    "/@anthropic-ai/claude-code/cli.mjs",
];

/// What a [`ProcessTable`] knows about one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStat {
    /// The parent's pid.
    pub ppid: Pid,
    /// The command name (`comm`): procfs's `comm`, or the base name of `ps`'s.
    pub comm: String,
}

/// A view of the process table, for walking up from a process.
pub trait ProcessTable {
    /// The parent, command name and start token of `pid`; `None` if unknown.
    fn stat(&self, pid: Pid) -> Option<ProcStat>;
    /// The argv of `pid`; empty if unknown.
    fn cmdline(&self, pid: Pid) -> Vec<String>;
    /// The start token of `pid` ([`start_token`]); `None` if unknown. Two
    /// processes with one pid have different tokens, so a reused pid is not
    /// mistaken for a recorded process.
    fn start(&self, pid: Pid) -> Option<String>;
}

/// The last `/`-separated component of `path` (Python's `os.path.basename`).
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Is a process with command name `comm` and argument vector `argv` Claude
/// Code?
///
/// - A native install: `comm` is `claude`, or `argv[0]`'s base name is
///   `claude` (a symlink), or `argv[0]` is under `…/claude/versions/` (the
///   versioned binary, whose `comm` is its version).
/// - Node (`argv[0]`'s base name starts with `node`) running the
///   `@anthropic-ai/claude-code` entry script ([`CLAUDE_ENTRY`]) as `argv[1]`
///   or `argv[2]` (one node option before it).
///
/// A bare package name is not Claude: `npx @anthropic-ai/claude-code` is the
/// launcher above it, and the launcher must not count as a second, outer
/// Claude.
pub fn is_claude<S: AsRef<str>>(comm: &str, argv: &[S]) -> bool {
    if comm == "claude" {
        return true;
    }
    let first = argv.first().map_or("", AsRef::as_ref);
    if basename(first) == "claude" || first.contains("/claude/versions/") {
        return true;
    }
    if basename(first).starts_with("node") {
        return argv.iter().skip(1).take(2).any(|a| {
            let a = a.as_ref();
            CLAUDE_ENTRY.iter().any(|entry| a.ends_with(entry))
        });
    }
    false
}

/// The first Claude Code process in a list of `(pid, command name, argv)`,
/// such as herdr's `pane process-info` foreground processes (panorama
/// `herdr.claude_pid`): the launcher above the real cli is skipped.
pub fn first_claude<'a, S, I>(procs: I) -> Option<Pid>
where
    S: AsRef<str> + 'a,
    I: IntoIterator<Item = (Pid, &'a str, &'a [S])>,
{
    procs
        .into_iter()
        .find(|(_, comm, argv)| is_claude(comm, argv))
        .map(|(pid, _, _)| pid)
}

/// `is_claude` for a process in a walk: `comm` first, so argv is read only for
/// a process not named `claude` (same answer, fewer reads).
fn walk_is_claude(table: &impl ProcessTable, pid: Pid, comm: &str) -> bool {
    comm == "claude" || is_claude(comm, &table.cmdline(pid))
}

/// The nearest Claude at or above `parent` (pass [`parent_pid`]: a hook's
/// parent): its pid and start token, or `None` (panorama
/// `hooks.claude_process`). The walk stops at pid 1, at an unreadable process
/// or after [`MAX_DEPTH`] steps; a Claude whose start token cannot be read is
/// `None` too (the hook then records no pid, and nothing is called lost).
pub fn claude_process(table: &impl ProcessTable, parent: Pid) -> Option<(Pid, String)> {
    let mut pid = parent;
    for _ in 0..MAX_DEPTH {
        if pid <= 1 {
            return None;
        }
        let st = table.stat(pid)?;
        if walk_is_claude(table, pid, &st.comm) {
            return Some((pid, table.start(pid)?));
        }
        pid = st.ppid;
    }
    None
}

/// The pid of the nearest Claude at or above `parent` (the session a hook
/// belongs to); with no Claude found, `parent`'s own parent (hook → sh →
/// agent); `None` if even that is unknown (inbox `proc.owning_claude`).
pub fn owning_claude(table: &impl ProcessTable, parent: Pid) -> Option<Pid> {
    let mut pid = parent;
    let mut first = None;
    for depth in 0..MAX_DEPTH {
        if pid <= 1 {
            break;
        }
        let Some(st) = table.stat(pid) else { break };
        if walk_is_claude(table, pid, &st.comm) {
            return Some(pid);
        }
        if depth == 0 {
            first = Some(st.ppid);
        }
        pid = st.ppid;
    }
    first.filter(|&p| p > 1)
}

/// Is the Claude this process serves nested inside another Claude? True when
/// a second Claude sits above the nearest one, walking up from `parent` (inbox
/// `proc.nested_claude`). A `claude -p` started from an agent's Bash tool
/// inherits the pane's `HERDR_PANE_ID`, so herdr identity alone would take it
/// for the pane's agent. An unknown or unreadable tree is not nested.
pub fn nested_claude(table: &impl ProcessTable, parent: Pid) -> bool {
    let mut claudes = 0;
    let mut pid = parent;
    for _ in 0..MAX_DEPTH {
        if pid <= 1 {
            break;
        }
        let Some(st) = table.stat(pid) else { break };
        if walk_is_claude(table, pid, &st.comm) {
            claudes += 1;
            if claudes == 2 {
                return true;
            }
        }
        pid = st.ppid;
    }
    false
}

/// [`claude_process`] for this process: a fresh [`SystemProcesses`], from
/// [`parent_pid`].
pub fn claude_process_here() -> Option<(Pid, String)> {
    claude_process(&SystemProcesses::new(), parent_pid())
}

/// [`owning_claude`] for this process: a fresh [`SystemProcesses`], from
/// [`parent_pid`].
pub fn owning_claude_here() -> Option<Pid> {
    owning_claude(&SystemProcesses::new(), parent_pid())
}

/// [`nested_claude`] for this process: a fresh [`SystemProcesses`], from
/// [`parent_pid`].
pub fn nested_claude_here() -> bool {
    nested_claude(&SystemProcesses::new(), parent_pid())
}

#[cfg(test)]
mod tests {
    use super::fake::FakeProcesses;
    use super::*;

    #[test]
    fn basename_is_posix() {
        assert_eq!(basename("/a/b/claude"), "claude");
        assert_eq!(basename("claude"), "claude");
        assert_eq!(basename("/a/claude/"), "");
        assert_eq!(basename(""), "");
    }

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
}
