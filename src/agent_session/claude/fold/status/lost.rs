//! The "process gone" rule: an agent that started, never stopped and whose
//! Claude process is gone is [`Status::Lost`].

use super::super::HookStart;
use super::super::times::or_time;
use super::terminal::Weighed;
use super::{Session, StatusOf};
use crate::agent_procs::{Liveness, ProcessIdentity};
use crate::agent_session::Status;

impl Session {
    /// [`Status::Lost`] when the agent's latest recorded start has no
    /// evidence at or after it, no other Claude process wrote a hook record
    /// since, and its own process is gone.
    pub(super) fn lost(
        &self,
        w: &Weighed<'_>,
        last: Option<f64>,
        live: &dyn Liveness,
    ) -> Option<StatusOf> {
        let hs = latest_start(w.starts)?;
        let lost = !w.endings.iter().any(|(t, _)| *t >= hs.ts.0)
            && !self.hooks.resumed_after(hs)
            && process_gone(hs, live);
        lost.then(|| StatusOf::new(Status::Lost, or_time(last, Some(hs.ts.0)), false))
    }
}

/// The first of the latest starts (Python's `max`).
fn latest_start(starts: &[HookStart]) -> Option<&HookStart> {
    starts
        .iter()
        .fold(None::<&HookStart>, |best, s| match best {
            Some(b) if b.ts >= s.ts => Some(b),
            _ => Some(s),
        })
}

/// Only a recorded, recognised Claude process in our pid namespace that no
/// longer exists counts as gone; anything unknown is not.
fn process_gone(start: &HookStart, live: &dyn Liveness) -> bool {
    let (Some(pid), Some(token)) = (start.pid, &start.start) else {
        return false;
    };
    let id = ProcessIdentity::new(pid, token.clone(), start.pidns.clone());
    live.process_alive(&id) == Some(false)
}
