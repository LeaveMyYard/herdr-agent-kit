//! Rules 1 and 2: an agent's terminal evidence, and whether a resume, a
//! recorded start or its own later activity overturns it.

use serde_json::Value;

use super::super::HookStart;
use super::super::limits::{REOPEN_MARGIN, STOP_NOTICE_WINDOW};
use super::{Agent, Session, StatusOf};
use crate::agent_session::claude::wire::Ending;
use crate::agent_session::{Status, Ts};

/// An agent's evidence, weighed by the later rules.
pub(super) struct Weighed<'a> {
    /// Terminal evidence: `(time, ending)`, in no order.
    pub(super) endings: Vec<(f64, Ending)>,
    /// The times something reopened the agent: resumes and recorded starts.
    pub(super) reopens: Vec<f64>,
    /// Its recorded hook starts.
    pub(super) starts: &'a [HookStart],
}

/// Whether the newest terminal evidence stands.
pub(super) enum Standing {
    /// It stands: the agent ended.
    Ended(StatusOf),
    /// No evidence, or it was overturned (`reopened`).
    Open { reopened: bool },
}

impl Session {
    /// Agent `a`'s evidence, or its final status when its workflow run's own
    /// ledger names it (nothing overturns that).
    pub(super) fn weigh<'s>(&'s self, a: &Agent<'_>) -> Result<Weighed<'s>, StatusOf> {
        let notices: Vec<(f64, Ending)> = self
            .notices
            .get(a.id)
            .into_iter()
            .flatten()
            .map(|n| (n.0.0, n.1))
            .collect();
        let mut endings = notices.clone();
        if let Some(ack) = a.ack
            && ack.status == "completed"
            && let Some(t) = ack.ts.map(Ts::secs)
        {
            endings.push((
                t,
                if ack.is_error {
                    Ending::Failed
                } else {
                    Ending::Done
                },
            ));
        }
        if let Some(run) = a.run {
            if let Some(ending) = self.workflows.ledger(run, a.id) {
                return Err(StatusOf::new(ending.into(), a.last, false)); // the run's own ledger: final
            }
            if let Some(end) = self.run_ending(run) {
                endings.push(end);
            }
        }
        for &stop in self.hooks.stops.get(a.id).into_iter().flatten() {
            // a recorded stop confirms an ending; a notice about the same stop says how it ended
            if !notices
                .iter()
                .any(|(t, _)| stop - REOPEN_MARGIN <= *t && *t <= stop + STOP_NOTICE_WINDOW)
            {
                endings.push((stop, Ending::Done));
            }
        }
        let starts: &[HookStart] = self.hooks.starts.get(a.id).map_or(&[], Vec::as_slice);
        let reopens = self
            .evidence()
            .resumes(a.id)
            .into_iter()
            .map(Ts::secs)
            .chain(starts.iter().map(|s| s.ts.0))
            .collect();
        Ok(Weighed {
            endings,
            reopens,
            starts,
        })
    }

    /// How workflow run `run` ended, as evidence for its agents: its
    /// launch's notice (a failed run cancels them), or else a summary saying
    /// `killed` / `failed` when no launch of the run is known.
    fn run_ending(&self, run: &str) -> Option<(f64, Ending)> {
        let workflows = self.workflows();
        if let Some((Ts(t), end)) = workflows.end(run) {
            let end = match end {
                Ending::Cancelled | Ending::Failed => Ending::Cancelled,
                Ending::Done => Ending::Done,
            };
            return Some((t, end));
        }
        let (summary, mtime) = self.workflows.summaries.get(run)?;
        let killed = matches!(
            summary.get("status").and_then(Value::as_str),
            Some("killed" | "failed")
        );
        (killed && workflows.live(run).is_none()).then_some((*mtime, Ending::Cancelled))
    }
}

impl Weighed<'_> {
    /// Rule 2: the newest evidence stands unless something reopened the
    /// agent after it, or the agent wrote more than [`REOPEN_MARGIN`] after
    /// it (`last`).
    pub(super) fn standing(&self, last: Option<f64>) -> Standing {
        let newest = self
            .endings
            .iter()
            .copied()
            .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let Some((when, ending)) = newest else {
            return Standing::Open { reopened: false };
        };
        let reopened = self.reopens.iter().any(|r| *r > when)
            || last.is_some_and(|l| l > when + REOPEN_MARGIN);
        if reopened {
            Standing::Open { reopened }
        } else {
            Standing::Ended(StatusOf::new(Status::from(ending), Some(when), false))
        }
    }
}
