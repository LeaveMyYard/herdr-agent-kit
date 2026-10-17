//! Claude Code's processes: "is this Claude Code", which Claude a process
//! serves, nested Claude, and the liveness of a Claude session's background
//! tasks. **IO** (reads the process table).
//!
//! **Is this process Claude Code?** [`is_claude`]: a native install (the
//! `claude` binary, or a versioned binary under `…/claude/versions/`), or node
//! running the `@anthropic-ai/claude-code` entry script. The `npx` launcher
//! above such a node process is *not* Claude (plan §1.3 row 5: the inbox used
//! to check only `comm == "claude"`).

use super::system::{SystemProcesses, parent_pid};
use super::{MAX_DEPTH, Pid, ProcessTable, basename};

mod liveness;
mod tasks;

pub use liveness::SystemLiveness;
pub use tasks::{live_tasks, task_of};

/// The entry scripts of the `@anthropic-ai/claude-code` npm package. A path
/// must end with one of these, so the scope is a whole path segment.
pub const CLAUDE_ENTRY: [&str; 2] = [
    "/@anthropic-ai/claude-code/cli.js",
    "/@anthropic-ai/claude-code/cli.mjs",
];

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
mod tests;
