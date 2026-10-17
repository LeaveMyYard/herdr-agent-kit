//! [`Evidence`]: the notices that end agents and tasks, and the resumes
//! that reopen agents.

use std::collections::{BTreeSet, HashMap};

use super::super::TaskEnd;
use super::super::stores::Notices;
use super::super::times::T;
use crate::agent_session::Ts;

/// What ended or reopened an agent or a task
/// ([`Session::evidence`](super::super::Session::evidence)).
#[derive(Clone, Copy, Debug)]
pub struct Evidence<'a> {
    pub(super) notices: &'a Notices,
    pub(super) resumes: &'a HashMap<String, BTreeSet<T>>,
}

impl<'a> Evidence<'a> {
    /// The `<task-notification>`s about `task` (an agent id, a background
    /// task id or a workflow run's task id) from any carrier in any file:
    /// oldest first.
    pub fn notices(&self, task: &str) -> impl ExactSizeIterator<Item = TaskEnd> + use<'a> {
        self.notices
            .get(task)
            .map(BTreeSet::iter)
            .unwrap_or_default()
            .map(|x| TaskEnd {
                at: Ts(x.0.0),
                ending: x.1,
                tool_uses: x.2,
            })
    }

    /// The times agent `id` was resumed by `SendMessage`, oldest first.
    pub fn resumes(&self, id: &str) -> impl ExactSizeIterator<Item = Ts> + use<'a> {
        self.resumes
            .get(id)
            .map(BTreeSet::iter)
            .unwrap_or_default()
            .map(|t| Ts(t.0))
    }
}
