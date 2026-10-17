//! panorama's hook records: recorded starts and stops, and the Claude
//! processes behind them.

use serde_json::Value;

use super::fields::{number, py_int};
use super::limits::HOOK_VERSION;
use super::{HookStart, Session};
use crate::agent_procs::{Liveness, Pid, ProcessIdentity};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Line;

/// panorama's hook records, and the Claude processes behind them.
impl Session {
    /// Forgets the hook records (the hook file was rewritten).
    pub fn reset_hooks(&mut self) {
        self.hook_starts.clear();
        self.hook_stops.clear();
        self.hook_procs.clear();
    }

    /// Folds one of panorama's hook records (`hooks/<session>.jsonl`, v1). A
    /// record of another version, without an `agent` string or without a
    /// numeric `ts` is skipped.
    pub fn apply_hook(&mut self, rec: &Line) {
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
            self.hook_procs.push((ts, p, s.clone()));
        }
        if agent.is_empty() {
            return;
        }
        match rec.get("event").and_then(Value::as_str) {
            Some("start") => {
                self.hook_starts
                    .entry(agent.to_string())
                    .or_default()
                    .push(HookStart {
                        ts: Ts(ts),
                        pid,
                        start: pstart,
                        pidns,
                    })
            }
            Some("stop") => self
                .hook_stops
                .entry(agent.to_string())
                .or_default()
                .push(ts),
            _ => {}
        }
    }

    /// Only a recorded, recognised Claude process in our pid namespace that
    /// no longer exists counts as gone; anything unknown is not.
    pub(super) fn process_gone(&self, start: &HookStart, live: &dyn Liveness) -> bool {
        let (Some(pid), Some(token)) = (start.pid, &start.start) else {
            return false;
        };
        let id = ProcessIdentity::new(pid, token.clone(), start.pidns.clone());
        live.process_alive(&id) == Some(false)
    }

    /// A later hook record from another Claude process: the session was
    /// resumed (or restarted) since; an agent's missing stop then says
    /// nothing about how it ended.
    pub(super) fn resumed_after(&self, start: &HookStart) -> bool {
        self.hook_procs.iter().any(|(t, p, s)| {
            *t > start.ts.0 && (Some(*p), Some(s.as_str())) != (start.pid, start.start.as_deref())
        })
    }
}
