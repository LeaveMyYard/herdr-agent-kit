//! Background tasks (Bash in the background, Monitor watches): launches,
//! stops, and which are still open or running.

use serde_json::{Map, Value};

use super::limits::{BG_CAP, MONITOR_SLACK};
use super::times::or_inf;
use super::{BgKind, BgLaunch, Owner, Session};
use crate::agent_procs::Liveness;
use crate::agent_session::Ts;
use crate::pytext::{non_empty, number};

/// Background tasks: launches, stops, open and still-running tasks.
impl Session {
    pub(super) fn background(
        &mut self,
        owner: &Owner,
        call: &str,
        tur: &Map<String, Value>,
        is_error: bool,
        when: Option<f64>,
    ) {
        let mut launch = None;
        if let Some(task) = non_empty(tur, "backgroundTaskId") {
            launch = Some((task, BgKind::Bash, None, false));
        } else if let Some(task) = non_empty(tur, "taskId")
            && tur.contains_key("timeoutMs")
            && !tur.contains_key("runId")
        {
            let timeout = number(tur.get("timeoutMs"))
                .filter(|ms| *ms > 0.0)
                .map(|ms| ms / 1000.0);
            let persistent = tur.get("persistent") == Some(&Value::Bool(true));
            launch = Some((task, BgKind::Monitor, timeout, persistent));
        }
        if let Some((task, kind, timeout, persistent)) = launch {
            // a fork's file repeats its parent's history, background launches included: main
            // wins, then the earliest record
            let replace = match self.bg.get(task) {
                None => true,
                Some(old) => {
                    old.owner != Owner::Main
                        && (*owner == Owner::Main || or_inf(when) < or_inf(old.ts.map(Ts::secs)))
                }
            };
            if replace {
                self.bg.insert(
                    task.to_string(),
                    BgLaunch {
                        task: task.to_string(),
                        owner: owner.clone(),
                        ts: when.map(Ts),
                        kind,
                        call: call.to_string(),
                        timeout,
                        persistent,
                    },
                );
            }
        }
        let mut stop = self.bg_stop_calls.get(call).cloned();
        if stop.is_none()
            && let Some(task) = tur.get("task_id").and_then(Value::as_str)
            && tur.contains_key("task_type")
        {
            stop = Some(task.to_string()); // TaskStop's own result names the task it stopped
        }
        if let (Some(stop), Some(t)) = (stop.filter(|s| !s.is_empty()), when)
            && !is_error
        {
            let at = self.bg_stops.entry(stop).or_insert(t);
            *at = at.min(t);
        }
    }

    /// The description of the call that started a background task.
    pub fn bg_description(&self, launch: &BgLaunch) -> &str {
        self.bg_calls
            .get(&launch.call)
            .map_or("", |(_, d)| d.as_str())
    }

    /// `owner`'s background tasks with no ending in the transcripts, oldest
    /// first.
    pub fn open_bg(&self, owner: &Owner, now: Ts) -> Vec<&BgLaunch> {
        self.open_bg_at(owner, now.0)
    }

    fn open_bg_at(&self, owner: &Owner, now: f64) -> Vec<&BgLaunch> {
        let expired = |l: &BgLaunch| match (l.kind, l.persistent, l.timeout, l.ts) {
            (BgKind::Monitor, false, Some(to), Some(ts)) => {
                to != 0.0 && now > ts.0 + to + MONITOR_SLACK
            }
            _ => false,
        };
        let mut out: Vec<&BgLaunch> = self
            .bg
            .values()
            .filter(|l| l.owner == *owner)
            .filter(|l| !self.notices.contains_key(&l.task) && !self.bg_stops.contains_key(&l.task))
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
    pub fn waiting_bg(&self, owner: &Owner, now: Ts, live: &dyn Liveness) -> Vec<&BgLaunch> {
        self.waiting_bg_at(owner, now.0, live)
    }

    pub(super) fn waiting_bg_at(
        &self,
        owner: &Owner,
        now: f64,
        live: &dyn Liveness,
    ) -> Vec<&BgLaunch> {
        self.open_bg_at(owner, now)
            .into_iter()
            .filter(|l| {
                let alive = match l.kind {
                    BgKind::Bash => live.task_alive(&l.task),
                    BgKind::Monitor => None,
                };
                alive.unwrap_or_else(|| l.ts.is_some_and(|ts| now - ts.0 <= BG_CAP))
            })
            .collect()
    }
}
