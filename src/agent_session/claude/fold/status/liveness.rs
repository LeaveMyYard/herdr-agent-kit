//! Rule 3: without standing terminal evidence, an agent is running while
//! something still waits on it or it is active; otherwise it waits on its
//! background work, or it ended silently.

use serde_json::Value;

use super::super::limits::{ACTIVE, ASYNC_QUIET, PENDING_CAP, REOPEN_MARGIN};
use super::super::times::or_time;
use super::evidence::Weighed;
use super::{Agent, Owner, Session, StatusOf};
use crate::agent_procs::Liveness;
use crate::agent_session::{Status, Ts};

impl Session {
    /// Rule 3 for agent `a` at `now`.
    pub(super) fn liveness(
        &self,
        a: &Agent<'_>,
        w: &Weighed<'_>,
        reopened: bool,
        now: f64,
        live: &dyn Liveness,
    ) -> StatusOf {
        let mut last = a.last;
        if let Some(r) = w.reopens.iter().copied().reduce(f64::max)
            && last.is_none_or(|l| r > l)
        {
            last = Some(r); // a resume restarts the clock before the agent writes anything
        }
        let Some(last) = last else {
            return self.never_written(a, now);
        };
        if let Some(running) = self.waited_on(a, last, now) {
            return running;
        }
        self.silent(a, w, reopened, last, now, live)
    }

    /// An agent with no time of its own: running for [`ASYNC_QUIET`] after
    /// it was spawned, then done (inferred).
    fn never_written(&self, a: &Agent<'_>, now: f64) -> StatusOf {
        let started = or_time(
            a.ack.and_then(|a| a.ts).map(Ts::secs),
            self.parentage()
                .spawn_of(a.id)
                .and_then(|s| s.ts)
                .map(Ts::secs),
        );
        if started.is_some_and(|s| now - s <= ASYNC_QUIET) {
            return StatusOf::new(Status::Run, None, true);
        }
        StatusOf::new(Status::Done, started, true)
    }

    /// Ground truth that the agent is running: a live workflow run or an
    /// open foreground spawning call still waits on it, a tool call of its
    /// own is in flight, or it wrote within [`ACTIVE`]. A run relaunched
    /// after the agent started abandons it (cancelled).
    fn waited_on(&self, a: &Agent<'_>, last: f64, now: f64) -> Option<StatusOf> {
        let workflows = self.workflows();
        if let Some(run) = a.run
            && workflows.live(run) == Some(true)
        {
            let newest_launch = workflows
                .launches()
                .filter(|w| w.run_id == run)
                .filter_map(|w| w.ts.map(Ts::secs))
                .reduce(f64::max);
            let first = a.file.and_then(|f| f.info.first_ts).map(Ts::secs);
            if let (Some(launch), Some(first)) = (newest_launch, first)
                && first < launch - REOPEN_MARGIN
            {
                // started by an earlier instance of a run that was relaunched (resumed): the
                // new instance re-runs unfinished steps with new agents; this one was abandoned
                return Some(StatusOf::new(Status::Cancelled, Some(launch), false));
            }
            if now - last <= PENDING_CAP {
                // the live run still waits on it (the same ground truth as an open spawning call)
                return Some(StatusOf::new(Status::Run, None, false));
            }
        }
        if let Some(sp) = self.parentage().spawn_of(a.id)
            && !sp.background
            && !self.spawns.results.contains(&sp.call)
            && now - sp.ts.map_or(last, |t| t.0.max(last)) <= PENDING_CAP
        {
            // foreground: the parent still waits on the spawning call; that is ground truth
            return Some(StatusOf::new(Status::Run, None, false));
        }
        let pend_ok = a.file.is_some_and(|f| {
            f.info
                .pending
                .iter()
                .any(|p| p.ts.is_some_and(|t| now - t.secs() <= PENDING_CAP))
        });
        (now - last <= ACTIVE || pend_ok).then(|| StatusOf::new(Status::Run, None, false))
    }

    /// Its turn ended: waiting on its own background work, a background
    /// agent not notified yet (within [`ASYNC_QUIET`]), or done.
    fn silent(
        &self,
        a: &Agent<'_>,
        w: &Weighed<'_>,
        reopened: bool,
        last: f64,
        now: f64,
        live: &dyn Liveness,
    ) -> StatusOf {
        let me = Owner::Agent(a.id.to_string());
        if !self.background().waiting(&me, Ts(now), live).is_empty() {
            return StatusOf::new(Status::Bg, None, false); // its turn ended, its background work runs
        }
        let meta = a.rec.map(|r| &r.meta);
        let meta_is = |key: &str, value: &str| {
            meta.and_then(|m| m.get(key)).and_then(Value::as_str) == Some(value)
        };
        if meta_is("agentType", "fork") {
            return StatusOf::new(Status::Done, Some(last), true); // interactive sidechain: idle
        }
        let background = a.ack.is_some_and(|a| a.status == "async_launched")
            || meta_is("requestShape", "background");
        // panorama: `(background or reopened) and (not evidence or reopened)`
        if (reopened || (background && w.endings.is_empty())) && now - last <= ASYNC_QUIET {
            return StatusOf::new(Status::Run, None, true); // not notified yet: still working
        }
        if a.file.is_some_and(|f| f.info.handback) {
            return StatusOf::new(Status::Done, Some(last), false);
        }
        StatusOf::new(Status::Done, Some(last), true)
    }
}
