//! The status rules ([`Session::status_of`]); module docs of [`super`].

use serde_json::Value;

use super::limits::{
    ACTIVE, ASYNC_QUIET, NESTING_CAP, PENDING_CAP, REOPEN_MARGIN, STOP_NOTICE_WINDOW,
};
use super::times::or_time;
use super::{HookStart, Owner, Session};
use crate::agent_procs::Liveness;
use crate::agent_session::claude::wire::Ending;
use crate::agent_session::{Status, Ts};

impl From<Ending> for Status {
    fn from(e: Ending) -> Status {
        match e {
            Ending::Done => Status::Done,
            Ending::Failed => Status::Failed,
            Ending::Cancelled => Status::Cancelled,
        }
    }
}

/// [`Session::status_of`]'s answer: panorama's `(status, ended_at, inferred)`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatusOf {
    /// The status.
    pub status: Status,
    /// When it ended (`None` while running).
    pub ended: Option<Ts>,
    /// Guessed from silence, not recorded.
    pub inferred: bool,
}

impl StatusOf {
    fn new(status: Status, ended: Option<f64>, inferred: bool) -> StatusOf {
        let ended = ended.map(Ts);
        StatusOf {
            status,
            ended,
            inferred,
        }
    }
}

impl Session {
    /// Agent `id`'s status at `now`: the status rules (module docs), with
    /// `live` answering the process checks. An agent that ended its turn
    /// silently but has a running descendant is waiting on it: running.
    pub fn status_of(&self, id: &str, now: Ts, live: &dyn Liveness) -> StatusOf {
        self.status_seen(id, now.0, live, &[])
    }

    fn status_seen(&self, id: &str, now: f64, live: &dyn Liveness, seen: &[&str]) -> StatusOf {
        let status = self.status(id, now, live);
        if status.status == Status::Done
            && status.inferred
            && !seen.contains(&id)
            && seen.len() < NESTING_CAP
        {
            let mut seen = seen.to_vec();
            seen.push(id);
            let running = self
                .children(&Owner::Agent(id.to_string()))
                .into_iter()
                .filter(|c| !seen.contains(c))
                .any(|c| {
                    matches!(
                        self.status_seen(c, now, live, &seen).status,
                        Status::Run | Status::Bg
                    )
                });
            if running {
                return StatusOf::new(Status::Run, None, true);
            }
        }
        status
    }

    fn status(&self, id: &str, now: f64, live: &dyn Liveness) -> StatusOf {
        let rec = self.agents.get(id);
        let f = self.file_of(id);
        let mut last = self.last_ts(id).map(Ts::secs);
        let run = rec.and_then(|r| r.workflow.as_deref());
        let notices: Vec<(f64, Ending)> = self
            .notices
            .get(id)
            .into_iter()
            .flatten()
            .map(|n| (n.0.0, n.1))
            .collect();
        let mut evidence = notices.clone();
        let ack = self.ack(id);
        if let Some(a) = ack
            && a.status == "completed"
            && let Some(t) = a.ts.map(Ts::secs)
        {
            evidence.push((
                t,
                if a.is_error {
                    Ending::Failed
                } else {
                    Ending::Done
                },
            ));
        }
        if let Some(run) = run {
            if self.journal_failed.get(run).is_some_and(|s| s.contains(id)) {
                return StatusOf::new(Status::Failed, last, false); // the run's own ledger: final
            }
            if self.journal_done.get(run).is_some_and(|s| s.contains(id)) {
                return StatusOf::new(Status::Done, last, false);
            }
            if let Some((Ts(t), end)) = self.run_end(run) {
                let end = match end {
                    Ending::Cancelled | Ending::Failed => Ending::Cancelled,
                    Ending::Done => Ending::Done,
                };
                evidence.push((t, end));
            } else if let Some((summary, mtime)) = self.wf_summary.get(run)
                && matches!(
                    summary.get("status").and_then(Value::as_str),
                    Some("killed" | "failed")
                )
                && self.run_live(run).is_none()
            {
                evidence.push((*mtime, Ending::Cancelled));
            }
        }
        for &stop in self.hook_stops.get(id).into_iter().flatten() {
            // a recorded stop confirms an ending; a notice about the same stop says how it ended
            if !notices
                .iter()
                .any(|(t, _)| stop - REOPEN_MARGIN <= *t && *t <= stop + STOP_NOTICE_WINDOW)
            {
                evidence.push((stop, Ending::Done));
            }
        }
        let starts: &[HookStart] = self.hook_starts.get(id).map_or(&[], Vec::as_slice);
        let reopen_times: Vec<f64> = self
            .resumes(id)
            .into_iter()
            .map(Ts::secs)
            .chain(starts.iter().map(|s| s.ts.0))
            .collect();
        let mut reopened = false;
        let newest = evidence
            .iter()
            .copied()
            .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((when, ending)) = newest {
            reopened = reopen_times.iter().any(|r| *r > when)
                || last.is_some_and(|l| l > when + REOPEN_MARGIN);
            if !reopened {
                return StatusOf::new(ending.into(), Some(when), false);
            }
        }
        // the first of the latest starts (Python's `max`)
        let latest_start = starts
            .iter()
            .fold(None::<&HookStart>, |best, s| match best {
                Some(b) if b.ts >= s.ts => Some(b),
                _ => Some(s),
            });
        if let Some(hs) = latest_start
            && !evidence.iter().any(|(t, _)| *t >= hs.ts.0)
            && !self.resumed_after(hs)
            && self.process_gone(hs, live)
        {
            // started, never stopped, its Claude process is gone
            return StatusOf::new(Status::Lost, or_time(last, Some(hs.ts.0)), false);
        }
        if let Some(r) = reopen_times.iter().copied().reduce(f64::max)
            && last.is_none_or(|l| r > l)
        {
            last = Some(r); // a resume restarts the clock before the agent writes anything
        }
        // no standing terminal evidence: liveness
        let Some(last) = last else {
            let started = or_time(
                ack.and_then(|a| a.ts).map(Ts::secs),
                self.spawn_of(id).and_then(|s| s.ts).map(Ts::secs),
            );
            if started.is_some_and(|s| now - s <= ASYNC_QUIET) {
                return StatusOf::new(Status::Run, None, true);
            }
            return StatusOf::new(Status::Done, started, true);
        };
        if let Some(run) = run
            && self.run_live(run) == Some(true)
        {
            let newest_launch = self
                .wf_launches
                .values()
                .filter(|w| w.run_id == run)
                .filter_map(|w| w.ts.map(Ts::secs))
                .reduce(f64::max);
            let first = f.and_then(|f| f.info.first_ts).map(Ts::secs);
            if let (Some(launch), Some(first)) = (newest_launch, first)
                && first < launch - REOPEN_MARGIN
            {
                // started by an earlier instance of a run that was relaunched (resumed): the
                // new instance re-runs unfinished steps with new agents; this one was abandoned
                return StatusOf::new(Status::Cancelled, Some(launch), false);
            }
            if now - last <= PENDING_CAP {
                // the live run still waits on it (the same ground truth as an open spawning call)
                return StatusOf::new(Status::Run, None, false);
            }
        }
        if let Some(sp) = self.spawn_of(id)
            && !sp.background
            && !self.spawn_results.contains(&sp.call)
            && now - sp.ts.map_or(last, |t| t.0.max(last)) <= PENDING_CAP
        {
            // foreground: the parent still waits on the spawning call; that is ground truth
            return StatusOf::new(Status::Run, None, false);
        }
        let pend_ok = f.is_some_and(|f| {
            f.info
                .pending
                .iter()
                .any(|p| p.ts.is_some_and(|t| now - t.secs() <= PENDING_CAP))
        });
        if now - last <= ACTIVE || pend_ok {
            return StatusOf::new(Status::Run, None, false);
        }
        if !self
            .waiting_bg_at(&Owner::Agent(id.to_string()), now, live)
            .is_empty()
        {
            return StatusOf::new(Status::Bg, None, false); // its turn ended, its background work runs
        }
        let meta = rec.map(|r| &r.meta);
        let meta_is = |key: &str, value: &str| {
            meta.and_then(|m| m.get(key)).and_then(Value::as_str) == Some(value)
        };
        if meta_is("agentType", "fork") {
            return StatusOf::new(Status::Done, Some(last), true); // interactive sidechain: idle
        }
        let background = ack.is_some_and(|a| a.status == "async_launched")
            || meta_is("requestShape", "background");
        // panorama: `(background or reopened) and (not evidence or reopened)`
        if (reopened || (background && evidence.is_empty())) && now - last <= ASYNC_QUIET {
            return StatusOf::new(Status::Run, None, true); // not notified yet: still working
        }
        if f.is_some_and(|f| f.info.handback) {
            return StatusOf::new(Status::Done, Some(last), false);
        }
        StatusOf::new(Status::Done, Some(last), true)
    }
}
