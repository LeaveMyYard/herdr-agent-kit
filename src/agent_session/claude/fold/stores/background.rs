//! Background tasks (Bash in the background, Monitor watches): the calls
//! that start and stop them, their launches and their stops.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::super::owner::{Owner, replaces};
use super::super::values::{BgKind, BgLaunch};
use crate::agent_session::Ts;
use crate::pytext::{non_empty, number, str_or_empty, truthy};

/// Background launches and stops.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct BgStore {
    /// By task id: the copy that counts ([`replaces`]).
    pub(in super::super) launches: IndexMap<String, BgLaunch>,
    /// Bash and Monitor calls by call id: kind and `description`.
    pub(in super::super) calls: HashMap<String, (BgKind, String)>,
    /// `TaskStop` / `KillShell` calls by call id: the task they stop.
    stop_calls: HashMap<String, String>,
    /// Task id → the earliest successful stop.
    pub(in super::super) stops: HashMap<String, f64>,
}

impl BgStore {
    /// A Bash or Monitor call `id`: it may start a task.
    pub(in super::super) fn on_call(&mut self, id: &str, kind: BgKind, inp: &Map<String, Value>) {
        self.calls
            .insert(id.to_string(), (kind, str_or_empty(inp, "description")));
    }

    /// A `TaskStop` / `KillShell` call `id`.
    pub(in super::super) fn on_stop_call(&mut self, id: &str, inp: &Map<String, Value>) {
        // `task_id or shell_id`: a truthy task_id of another type wins, and is no task
        let task = match inp.get("task_id") {
            Some(v) if truthy(v) => Some(v),
            _ => inp.get("shell_id"),
        };
        if let Some(task) = task.and_then(Value::as_str).filter(|t| !t.is_empty()) {
            self.stop_calls.insert(id.to_string(), task.to_string());
        }
    }

    /// A tool result for `call` in `owner`'s file (`tur`: its
    /// `toolUseResult`): a task's launch or a successful stop.
    pub(in super::super) fn on_result(
        &mut self,
        owner: &Owner,
        call: &str,
        tur: &Map<String, Value>,
        is_error: bool,
        when: Option<f64>,
    ) {
        if let Some((task, kind, timeout, persistent)) = launch(tur) {
            let replace = match self.launches.get(task) {
                None => true,
                Some(old) => replaces(owner, when, &old.owner, old.ts.map(Ts::secs)),
            };
            if replace {
                self.launches.insert(
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
        let mut stop = self.stop_calls.get(call).cloned();
        if stop.is_none()
            && let Some(task) = tur.get("task_id").and_then(Value::as_str)
            && tur.contains_key("task_type")
        {
            stop = Some(task.to_string()); // TaskStop's own result names the task it stopped
        }
        if let (Some(stop), Some(t)) = (stop.filter(|s| !s.is_empty()), when)
            && !is_error
        {
            let at = self.stops.entry(stop).or_insert(t);
            *at = at.min(t);
        }
    }
}

/// The task a result starts: a Bash's `backgroundTaskId`, or a Monitor's
/// `taskId` with `timeoutMs` (a workflow's `runId` result is no task):
/// `(task, kind, timeout in seconds, persistent)`.
fn launch(tur: &Map<String, Value>) -> Option<(&str, BgKind, Option<f64>, bool)> {
    if let Some(task) = non_empty(tur, "backgroundTaskId") {
        return Some((task, BgKind::Bash, None, false));
    }
    let task = non_empty(tur, "taskId")?;
    if !tur.contains_key("timeoutMs") || tur.contains_key("runId") {
        return None;
    }
    let timeout = number(tur.get("timeoutMs"))
        .filter(|ms| *ms > 0.0)
        .map(|ms| ms / 1000.0);
    let persistent = tur.get("persistent") == Some(&Value::Bool(true));
    Some((task, BgKind::Monitor, timeout, persistent))
}
