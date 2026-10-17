//! Scripted process tables and liveness, for tests. **Pure** (no IO).
//!
//! Behind the `test-support` feature, like `FakeHerdr`: the status tests of
//! `agent_session::claude` and both plugins' tests use it.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use super::{Liveness, Pid, ProcStat, ProcessIdentity, ProcessTable};

/// A process table written by the test: pid → (parent, command name, start
/// token, argv). A pid not in it is unreadable.
#[derive(Debug, Default)]
pub struct FakeProcesses {
    procs: BTreeMap<Pid, (ProcStat, Option<String>, Vec<String>)>,
    cmdline_reads: Cell<usize>,
}

impl FakeProcesses {
    /// Adds (or replaces) process `pid`.
    pub fn insert<S: AsRef<str>>(
        &mut self,
        pid: Pid,
        ppid: Pid,
        comm: &str,
        start: &str,
        argv: &[S],
    ) {
        let stat = ProcStat {
            ppid,
            comm: comm.to_owned(),
        };
        let argv = argv.iter().map(|a| a.as_ref().to_owned()).collect();
        self.procs.insert(pid, (stat, Some(start.to_owned()), argv));
    }

    /// Makes the start token of `pid` unreadable.
    pub fn forget_start(&mut self, pid: Pid) {
        if let Some((_, start, _)) = self.procs.get_mut(&pid) {
            *start = None;
        }
    }

    /// The pids whose `(comm, argv)` [`super::claude::is_claude`] accepts, ascending.
    pub fn claudes(&self) -> Vec<Pid> {
        self.procs
            .iter()
            .filter(|(_, (st, _, argv))| super::claude::is_claude(&st.comm, argv))
            .map(|(&pid, _)| pid)
            .collect()
    }

    /// How many times [`ProcessTable::cmdline`] was called.
    pub fn cmdline_reads(&self) -> usize {
        self.cmdline_reads.get()
    }
}

impl ProcessTable for FakeProcesses {
    fn stat(&self, pid: Pid) -> Option<ProcStat> {
        self.procs.get(&pid).map(|(st, _, _)| st.clone())
    }

    fn cmdline(&self, pid: Pid) -> Vec<String> {
        self.cmdline_reads.set(self.cmdline_reads.get() + 1);
        self.procs
            .get(&pid)
            .map(|(_, _, argv)| argv.clone())
            .unwrap_or_default()
    }

    fn start(&self, pid: Pid) -> Option<String> {
        self.procs.get(&pid).and_then(|(_, start, _)| start.clone())
    }
}

/// Scripted [`Liveness`]: a process or task the test did not name is unknown
/// (`None`), as on a machine whose processes cannot be read.
#[derive(Debug, Default, Clone)]
pub struct FakeLiveness {
    /// Recorded processes and whether each is alive.
    pub processes: BTreeMap<ProcessIdentity, bool>,
    /// The background task ids that are running, or `None` when they cannot
    /// be checked.
    pub tasks: Option<BTreeSet<String>>,
}

impl FakeLiveness {
    /// Marks the recorded process `id` alive or gone.
    pub fn process(mut self, id: ProcessIdentity, alive: bool) -> Self {
        self.processes.insert(id, alive);
        self
    }

    /// Makes background tasks checkable, with `running` the live ones.
    pub fn tasks<S: AsRef<str>>(mut self, running: &[S]) -> Self {
        self.tasks = Some(running.iter().map(|t| t.as_ref().to_owned()).collect());
        self
    }
}

impl Liveness for FakeLiveness {
    fn process_alive(&self, id: &ProcessIdentity) -> Option<bool> {
        self.processes.get(id).copied()
    }

    fn task_alive(&self, task: &str) -> Option<bool> {
        self.tasks.as_ref().map(|t| t.contains(task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_liveness_answers_only_what_it_was_told() {
        let id = ProcessIdentity::new(80, "s3", None);
        let other = ProcessIdentity::new(80, "s4", None);
        let live = FakeLiveness::default().process(id.clone(), false);
        assert_eq!(live.process_alive(&id), Some(false));
        assert_eq!(live.process_alive(&other), None);
        assert_eq!(live.task_alive("b1"), None);
        let live = live.tasks(&["b1"]);
        assert_eq!(live.task_alive("b1"), Some(true));
        assert_eq!(live.task_alive("b2"), Some(false));
    }
}
