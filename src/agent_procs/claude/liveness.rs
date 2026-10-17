//! [`SystemLiveness`]: liveness from this machine's process table, for one
//! Claude Code session.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::tasks::live_tasks;
use crate::agent_procs::{Liveness, Pid, ProcessIdentity, system};

/// Liveness from this machine's process table, for one session and one pass
/// of the status rules (panorama: one `Session.update` / one collect).
///
/// Answers are cached for the life of the value, so one pass sees one
/// consistent picture and reads each process at most once; make a new one for
/// the next pass.
#[derive(Debug, Default)]
pub struct SystemLiveness {
    process: Option<Pid>,
    processes: RefCell<BTreeMap<ProcessIdentity, Option<bool>>>,
    tasks: OnceCell<Option<BTreeSet<String>>>,
}

impl SystemLiveness {
    /// Liveness for a session whose Claude Code process is `process` (the
    /// pane's foreground Claude, [`super::first_claude`]). Its
    /// descendants are where the session's background tasks run
    /// ([`super::live_tasks`]); with `None`, tasks are unknown.
    ///
    /// The caller must make sure `process` is still the pane's Claude: a pid
    /// cached from an earlier pass may have been reused by another process,
    /// whose tree would call every running task finished. Compare its
    /// [`start_token`](crate::agent_procs::start_token) with the one taken when
    /// it was found (panorama `collect._live_tasks`), and look it up again on a
    /// mismatch.
    pub fn new(process: Option<Pid>) -> Self {
        Self {
            process,
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
            .get_or_init(|| self.process.and_then(live_tasks))
            .as_ref()
            .map(|t| t.contains(task))
    }
}

#[cfg(test)]
mod tests;
