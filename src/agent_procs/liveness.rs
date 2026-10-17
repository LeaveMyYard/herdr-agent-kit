//! Liveness of recorded processes and background tasks: what the status rules
//! ask ([`Liveness`]), and the answer when nothing can be known ([`NoProcs`]).

use super::Pid;

/// A process as a hook recorded it: pid, start token and pid namespace
/// (panorama hook records' `pid`, `pstart`, `pidns`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessIdentity {
    /// The process id.
    pub pid: Pid,
    /// The start token ([`super::start_token`]), so a reused pid is not
    /// mistaken for this process.
    pub start: String,
    /// The pid namespace it was recorded in ([`super::own_pidns`]), if known.
    pub pidns: Option<String>,
}

impl ProcessIdentity {
    /// A recorded process.
    pub fn new(pid: Pid, start: impl Into<String>, pidns: Option<String>) -> Self {
        Self {
            pid,
            start: start.into(),
            pidns,
        }
    }
}

/// The process checks the session status rules need. Every answer is
/// `Some(true)` (alive), `Some(false)` (gone) or `None` (cannot be told); the
/// rules never call anything lost on `None`.
pub trait Liveness {
    /// Is the recorded process still running? `None` when it cannot be told:
    /// another pid namespace than ours, no permission, no process table.
    fn process_alive(&self, id: &ProcessIdentity) -> Option<bool>;

    /// Is the session's background Bash task `task` still running? `None`
    /// when it cannot be checked (then the transcript and its age cap decide).
    fn task_alive(&self, task: &str) -> Option<bool>;
}

/// Liveness that is never known: for transcripts read without access to their
/// processes (a remote machine's mirror; panorama `Collector(procs=False)`).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProcs;

impl Liveness for NoProcs {
    fn process_alive(&self, _: &ProcessIdentity) -> Option<bool> {
        None
    }

    fn task_alive(&self, _: &str) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests;
