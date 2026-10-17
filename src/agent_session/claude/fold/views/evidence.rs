//! [`Evidence`]: the notices that end agents and tasks, and the resumes
//! that reopen agents.

use std::collections::{BTreeSet, HashMap};

use super::super::stores::Notices;
use super::super::times::T;
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Ending;

/// What ended or reopened an agent or a task
/// ([`Session::evidence`](super::super::Session::evidence)).
#[derive(Clone, Copy, Debug)]
pub struct Evidence<'a> {
    pub(super) notices: &'a Notices,
    pub(super) resumes: &'a HashMap<String, BTreeSet<T>>,
}

impl Evidence<'_> {
    /// The `<task-notification>`s about `task` (an agent id, a background
    /// task id or a workflow run's task id) from any carrier in any file:
    /// `(time, ending, tool uses)`, oldest first.
    pub fn notices(&self, task: &str) -> Vec<(Ts, Ending, Option<u64>)> {
        self.notices
            .get(task)
            .map(|n| n.iter().map(|x| (Ts(x.0.0), x.1, x.2)).collect())
            .unwrap_or_default()
    }

    /// The times agent `id` was resumed by `SendMessage`, oldest first.
    pub fn resumes(&self, id: &str) -> Vec<Ts> {
        self.resumes
            .get(id)
            .map(|r| r.iter().map(|t| Ts(t.0)).collect())
            .unwrap_or_default()
    }
}
