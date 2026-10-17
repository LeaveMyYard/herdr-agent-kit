//! Liveness of recorded processes and background tasks.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::{Pid, system};

/// A process as a hook recorded it: pid, start token and pid namespace
/// (panorama hook records' `pid`, `pstart`, `pidns`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessIdentity {
    /// The process id.
    pub pid: Pid,
    /// The start token ([`super::ProcStat::start`]), so a reused pid is not
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

/// Liveness from this machine's process table, for one session and one pass
/// of the status rules (panorama: one `Session.update` / one collect).
///
/// Answers are cached for the life of the value, so one pass sees one
/// consistent picture and reads each process at most once; make a new one for
/// the next pass.
#[derive(Debug, Default)]
pub struct SystemLiveness {
    claude: Option<Pid>,
    processes: RefCell<BTreeMap<ProcessIdentity, Option<bool>>>,
    tasks: OnceCell<Option<BTreeSet<String>>>,
}

impl SystemLiveness {
    /// Liveness for a session whose Claude Code process is `claude` (the
    /// pane's foreground Claude, [`super::first_claude`]). Its descendants
    /// are where the session's background tasks run; with `None`, tasks are
    /// unknown.
    pub fn new(claude: Option<Pid>) -> Self {
        Self {
            claude,
            ..Self::default()
        }
    }
}

impl Liveness for SystemLiveness {
    fn process_alive(&self, id: &ProcessIdentity) -> Option<bool> {
        if let Some(&known) = self.processes.borrow().get(id) {
            return known;
        }
        let alive = system::process_alive(id);
        self.processes.borrow_mut().insert(id.clone(), alive);
        alive
    }

    fn task_alive(&self, task: &str) -> Option<bool> {
        self.tasks
            .get_or_init(|| self.claude.and_then(system::live_tasks))
            .as_ref()
            .map(|t| t.contains(task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_procs_knows_nothing() {
        let id = ProcessIdentity::new(std::process::id(), "x", None);
        assert_eq!(NoProcs.process_alive(&id), None);
        assert_eq!(NoProcs.task_alive("b1"), None);
    }

    #[test]
    fn tasks_are_unknown_without_a_claude_process() {
        assert_eq!(SystemLiveness::new(None).task_alive("b1"), None);
    }

    #[test]
    fn this_process_is_alive_and_a_reused_pid_is_not_it() {
        let me = std::process::id();
        let start = system::stat(me).expect("own stat").start;
        let live = SystemLiveness::new(None);
        assert_eq!(
            live.process_alive(&ProcessIdentity::new(me, start.clone(), None)),
            Some(true)
        );
        assert_eq!(
            live.process_alive(&ProcessIdentity::new(me, format!("{start}0"), None)),
            Some(false)
        );
        // cached: the same answer again without a second read
        assert_eq!(live.processes.borrow().len(), 2);
        assert_eq!(
            live.process_alive(&ProcessIdentity::new(me, start, None)),
            Some(true)
        );
        assert_eq!(live.processes.borrow().len(), 2);
    }
}
