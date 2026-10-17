//! Agent processes. **IO** (reads the process table).
//!
//! One implementation, for both plugins, of three questions about processes.
//! This module holds the harness-neutral parts (plan §2.3, "Harness-neutral
//! seams"); what is specific to one harness lives below it, one module per
//! harness ([`claude`] for Claude Code, the only one so far):
//!
//! - **Is this process an agent, and which harness?** For Claude Code,
//!   [`claude::is_claude`].
//! - **Which agent does this process serve, and is it nested in another?**
//!   For Claude Code, [`claude::claude_process`], [`claude::owning_claude`]
//!   and [`claude::nested_claude`]. They walk up the process tree through a
//!   [`ProcessTable`]: the real one is [`SystemProcesses`] (procfs, or `ps`
//!   where there is no procfs, as on macOS), the test one is
//!   `fake::FakeProcesses` (feature `test-support`).
//! - **Is a recorded process still alive?** The [`Liveness`] trait, which
//!   [`crate::agent_session::claude::fold`]'s `status_of` takes as an argument
//!   so the status rules stay pure. [`claude::SystemLiveness`] reads the real
//!   process table and Claude Code's background tasks, [`NoProcs`] answers
//!   "unknown" everywhere (transcripts mirrored from another machine), and
//!   `fake::FakeLiveness` (feature `test-support`) is scripted for tests.
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

pub mod claude;
mod liveness;
mod system;

#[cfg(any(test, feature = "test-support"))]
pub mod fake;

pub use liveness::{Liveness, NoProcs, ProcessIdentity};
pub use system::{
    SystemProcesses, descendants, own_pidns, parent_pid, pid_alive, process_alive, start_token,
};

/// A process id, as the plugins record it.
pub type Pid = u32;

/// How far up the process tree a walk goes before it gives up.
pub const MAX_DEPTH: usize = 32;

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
    /// The parent and command name of `pid`; `None` if unknown.
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

#[cfg(test)]
mod tests;
