//! Hook records ([format v1](super::super#hook-records-format-v1)): the
//! recorded starts and stops of agents, and the Claude processes that wrote
//! them.

use std::collections::HashMap;

use serde_json::Value;

use super::super::limits::HOOK_VERSION;
use super::super::values::HookStart;
use crate::agent_procs::Pid;
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Line;
use crate::pytext::{number, py_int};

/// The hook records folded so far.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct HookStore {
    /// Agent id → its `start` records, in file order.
    pub(in super::super) starts: HashMap<String, Vec<HookStart>>,
    /// Agent id → the times of its `stop` records.
    pub(in super::super) stops: HashMap<String, Vec<f64>>,
    /// Every record with a process identity: `(ts, pid, pstart)`.
    procs: Vec<(f64, Pid, String)>,
}

impl HookStore {
    /// Forgets every record (the hook file was rewritten).
    pub(in super::super) fn clear(&mut self) {
        self.starts.clear();
        self.stops.clear();
        self.procs.clear();
    }

    /// One record. A record of another version, without an `agent` string
    /// or without a numeric `ts` is skipped.
    pub(in super::super) fn apply(&mut self, rec: &Line) {
        let version_ok = match rec.get("v") {
            Some(Value::Bool(b)) => *b, // Python: True == 1
            Some(v) => v.as_f64() == Some(HOOK_VERSION),
            None => false,
        };
        let (Some(agent), Some(ts)) = (
            rec.get("agent").and_then(Value::as_str),
            number(rec.get("ts")),
        ) else {
            return;
        };
        if !version_ok {
            return;
        }
        let pid = py_int(rec.get("pid")).and_then(|p| Pid::try_from(p).ok());
        let pstart = rec
            .get("pstart")
            .and_then(Value::as_str)
            .map(str::to_string);
        let pidns = rec.get("pidns").and_then(Value::as_str).map(str::to_string);
        if let (Some(p), Some(s)) = (pid, &pstart) {
            self.procs.push((ts, p, s.clone()));
        }
        if agent.is_empty() {
            return;
        }
        match rec.get("event").and_then(Value::as_str) {
            Some("start") => self
                .starts
                .entry(agent.to_string())
                .or_default()
                .push(HookStart {
                    ts: Ts(ts),
                    pid,
                    start: pstart,
                    pidns,
                }),
            Some("stop") => self.stops.entry(agent.to_string()).or_default().push(ts),
            _ => {}
        }
    }

    /// A later hook record from another Claude process: the session was
    /// resumed (or restarted) since; an agent's missing stop then says
    /// nothing about how it ended.
    pub(in super::super) fn resumed_after(&self, start: &HookStart) -> bool {
        self.procs.iter().any(|(t, p, s)| {
            *t > start.ts.0 && (Some(*p), Some(s.as_str())) != (start.pid, start.start.as_deref())
        })
    }
}
