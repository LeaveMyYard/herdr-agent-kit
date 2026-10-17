//! [`Workflows`]: workflow runs, their journals and summaries, and how a
//! run ended.

use serde_json::{Map, Value};

use super::super::WorkflowLaunch;
use super::super::stores::{Notices, WorkflowStore};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Ending;

/// The workflow runs ([`Session::workflows`](super::super::Session::workflows)).
#[derive(Clone, Copy, Debug)]
pub struct Workflows<'a> {
    pub(super) store: &'a WorkflowStore,
    pub(super) notices: &'a Notices,
}

impl<'a> Workflows<'a> {
    /// The workflow launches, in the order they were first read.
    pub fn launches(&self) -> impl Iterator<Item = &'a WorkflowLaunch> + 'a {
        self.store.launches.values()
    }

    /// Run `run`'s summary object (`workflows/<run>.json`), if read.
    pub fn summary(&self, run: &str) -> Option<&'a Map<String, Value>> {
        self.store.summaries.get(run).map(|(s, _)| s)
    }

    /// The modification time of run `run`'s summary when it was read.
    pub fn summary_mtime(&self, run: &str) -> Option<Ts> {
        self.store.summaries.get(run).map(|(_, m)| Ts(*m))
    }

    /// The journal's `started` record of agent `id` in run `run`: its label
    /// and phase.
    pub fn started(&self, run: &str, id: &str) -> Option<(&'a str, Option<&'a str>)> {
        let (label, phase) = self.store.started.get(run)?.get(id)?;
        Some((label, phase.as_deref()))
    }

    /// The phases of run `run` in the order its journal first named them.
    pub fn phases(&self, run: &str) -> &'a [String] {
        self.store.phases.get(run).map_or(&[], Vec::as_slice)
    }

    /// The `(time, ending)` of the latest notice about the run's latest
    /// launch, if any.
    pub fn end(&self, run: &str) -> Option<(Ts, Ending)> {
        let mut launches: Vec<&WorkflowLaunch> = self
            .store
            .launches
            .values()
            .filter(|w| w.run_id == run)
            .collect();
        launches.sort_by(|a, b| {
            a.ts.map_or(0.0, Ts::secs)
                .total_cmp(&b.ts.map_or(0.0, Ts::secs))
        });
        let task = launches.last()?.task_id.as_deref().unwrap_or("");
        let last = self.notices.get(task)?.last()?;
        Some((Ts(last.0.0), last.1))
    }

    /// Whether run `run` is still running: `None` when no launch of it is
    /// known (compacted away).
    pub fn live(&self, run: &str) -> Option<bool> {
        self.store
            .launches
            .values()
            .any(|w| w.run_id == run)
            .then(|| self.end(run).is_none())
    }
}
