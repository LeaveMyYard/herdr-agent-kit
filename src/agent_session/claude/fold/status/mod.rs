//! The status rules ([`Session::status_of`]); module docs of [`super`].
//!
//! One rule per file, applied in order: [`terminal`] (rules 1 and 2:
//! terminal evidence and what overturns it), [`lost`] (an agent whose Claude
//! process is gone) and [`liveness`] (rule 3: no standing evidence).

use super::limits::NESTING_CAP;
use super::{Ack, AgentRec, FileFacts, Owner, Session};
use crate::agent_procs::Liveness;
use crate::agent_session::claude::wire::Ending;
use crate::agent_session::{Status, Ts};

mod liveness;
mod lost;
mod terminal;

use terminal::Standing;

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

/// What the rules read of one agent, gathered once.
struct Agent<'a> {
    id: &'a str,
    rec: Option<&'a AgentRec>,
    file: Option<&'a FileFacts>,
    /// The newest time in its transcript.
    last: Option<f64>,
    /// The workflow run it belongs to.
    run: Option<&'a str>,
    ack: Option<&'a Ack>,
}

/// The status rules (the [module docs](super)).
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
                .parentage()
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

    /// Agent `id`'s own status, its descendants aside.
    fn status(&self, id: &str, now: f64, live: &dyn Liveness) -> StatusOf {
        let rec = self.agents.get(id);
        let agent = Agent {
            id,
            rec,
            file: self.file_of(id),
            last: self.last_ts(id).map(Ts::secs),
            run: rec.and_then(|r| r.workflow.as_deref()),
            ack: self.parentage().ack(id),
        };
        let weighed = match self.weigh(&agent) {
            Ok(w) => w,
            Err(ledger) => return ledger,
        };
        let reopened = match weighed.standing(agent.last) {
            Standing::Ended(s) => return s,
            Standing::Open { reopened } => reopened,
        };
        if let Some(lost) = self.lost(&weighed, agent.last, live) {
            return lost;
        }
        self.liveness(&agent, &weighed, reopened, now, live)
    }
}
