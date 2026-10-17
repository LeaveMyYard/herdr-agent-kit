//! [`Background`]: background tasks, and which are still open or running.

use super::super::limits::{BG_CAP, MONITOR_SLACK};
use super::super::stores::{BgStore, Notices};
use super::super::{BgKind, BgLaunch, Owner};
use crate::agent_procs::Liveness;
use crate::agent_session::Ts;

/// The background tasks ([`Session::background`](super::super::Session::background)):
/// Bash runs in the background and Monitor watches.
///
/// A Bash result with `backgroundTaskId` (run in the background, or moved
/// there by its timeout) or a Monitor result (`taskId` with `timeoutMs`)
/// starts a task owned by the file's owner. It ends with any terminal notice
/// naming it, a Monitor's timeout, or a successful `TaskStop` / `KillShell`.
#[derive(Clone, Copy, Debug)]
pub struct Background<'a> {
    pub(super) store: &'a BgStore,
    pub(super) notices: &'a Notices,
}

impl<'a> Background<'a> {
    /// The description of the call that started a background task.
    pub fn description(&self, launch: &BgLaunch) -> &'a str {
        self.store
            .calls
            .get(&launch.call)
            .map_or("", |(_, d)| d.as_str())
    }

    /// `owner`'s background tasks with no ending in the transcripts, oldest
    /// first.
    pub fn open(&self, owner: &Owner, now: Ts) -> Vec<&'a BgLaunch> {
        let now = now.0;
        let expired = |l: &BgLaunch| match (l.kind, l.persistent, l.timeout, l.ts) {
            (BgKind::Monitor, false, Some(to), Some(ts)) => {
                to != 0.0 && now > ts.0 + to + MONITOR_SLACK
            }
            _ => false,
        };
        let mut out: Vec<&BgLaunch> = self
            .store
            .launches
            .values()
            .filter(|l| l.owner == *owner)
            .filter(|l| {
                !self.notices.contains_key(&l.task) && !self.store.stops.contains_key(&l.task)
            })
            .filter(|l| !expired(l))
            .collect();
        out.sort_by(|a, b| {
            a.ts.map_or(0.0, Ts::secs)
                .total_cmp(&b.ts.map_or(0.0, Ts::secs))
        });
        out
    }

    /// The open tasks that are still running: their process lives, or, when
    /// that cannot be checked, they started at most [`BG_CAP`] ago. A
    /// Monitor's process is never checked (its timeout decides).
    pub fn waiting(&self, owner: &Owner, now: Ts, live: &dyn Liveness) -> Vec<&'a BgLaunch> {
        self.open(owner, now)
            .into_iter()
            .filter(|l| {
                let alive = match l.kind {
                    BgKind::Bash => live.task_alive(&l.task),
                    BgKind::Monitor => None,
                };
                alive.unwrap_or_else(|| l.ts.is_some_and(|ts| now.0 - ts.0 <= BG_CAP))
            })
            .collect()
    }
}
