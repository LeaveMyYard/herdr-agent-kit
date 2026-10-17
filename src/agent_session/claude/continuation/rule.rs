//! [`Continuation`]: the continuation rule (see the [module docs](super))
//! folded over one main transcript's lines.

use crate::agent_session::claude::wire::{Line, contains};
use crate::agent_session::{SessionId, Ts};

/// The continuation state of one session's **main** transcript, folded line
/// by line. Feed it every line of the file in order, in batches as they are
/// appended ([`Tailer`](crate::agent_session::tail::Tailer));
/// [`Continuation::reset`] when the file was rewritten.
#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    own: SessionId,
    next: Option<SessionId>,
    at: Option<Ts>,
    /// The time of the last dated record of the lines read before the current
    /// batch (an undated `continued-in` takes it when nothing in its own batch
    /// is dated).
    line_ts: Option<Ts>,
}

impl Continuation {
    /// The state of a main transcript of session `own`, before any line.
    pub fn new(own: SessionId) -> Continuation {
        Continuation {
            own,
            next: None,
            at: None,
            line_ts: None,
        }
    }

    /// The session this transcript belongs to.
    pub fn own(&self) -> &SessionId {
        &self.own
    }

    /// The session the conversation continued in, or `None` (never
    /// continued, or resumed since).
    pub fn next(&self) -> Option<&SessionId> {
        self.next.as_ref()
    }

    /// The time of the standing continuation (`None` when there is none, or
    /// when it has no time).
    pub fn at(&self) -> Option<Ts> {
        self.at
    }

    /// Forgets everything read: the file was rewritten.
    pub fn reset(&mut self) {
        self.next = None;
        self.at = None;
        self.line_ts = None;
    }

    /// Folds the next lines of the transcript (raw, one record per line).
    ///
    /// Cheap on the common path: a line is parsed only when it may be a
    /// `continued-in` record, when a continuation stands and it may be a
    /// conversation record, or (from the end, stopping at the first hit) to
    /// date an undated `continued-in`.
    pub fn feed<L: AsRef<[u8]>>(&mut self, lines: &[L]) {
        // self.line_ts holds the time of the last dated record of lines[..done]
        let mut done = 0;
        for (i, raw) in lines.iter().enumerate() {
            let raw = raw.as_ref();
            let candidate = contains(raw, b"continued-in");
            let conversation =
                self.next.is_some() && (contains(raw, b"user") || contains(raw, b"assistant"));
            if !candidate && !conversation {
                continue;
            }
            let Some(line) = Line::parse(raw) else {
                continue;
            };
            match line.kind() {
                Some("continued-in") => {
                    // a time of 0 is falsy in both Pythons (`ts or last`): no time
                    let when = match line.ts().filter(|t| t.0 != 0.0) {
                        Some(t) => Some(t),
                        None => {
                            if let Some(found) = last_dated(&lines[done..i]) {
                                self.line_ts = Some(found);
                            }
                            done = i;
                            self.line_ts
                        }
                    };
                    let target = line
                        .get("continuedInSessionId")
                        .and_then(SessionId::from_value);
                    if let Some(target) = target
                        && !line.is_sidechain()
                        && target != self.own
                    {
                        self.next = Some(target);
                        self.at = when;
                    }
                }
                Some("user" | "assistant") if self.next.is_some() => {
                    let when = line.ts();
                    let later = match (self.at, when) {
                        (None, _) => true,
                        (Some(at), Some(t)) => t > at,
                        (Some(_), None) => false,
                    };
                    if later {
                        self.next = None;
                        self.at = None;
                    }
                }
                _ => {}
            }
        }
        if let Some(found) = last_dated(&lines[done..]) {
            self.line_ts = Some(found);
        }
    }
}

/// The own time of the last dated record in `lines`, searched from the end:
/// only a line holding a `"timestamp"` key is parsed, and the search stops at
/// the first dated record.
fn last_dated<L: AsRef<[u8]>>(lines: &[L]) -> Option<Ts> {
    lines
        .iter()
        .rev()
        .map(AsRef::as_ref)
        .filter(|raw| contains(raw, b"\"timestamp\""))
        .find_map(|raw| Line::parse(raw).and_then(|l| l.ts()).filter(|t| t.0 != 0.0))
}

#[cfg(test)]
mod tests;
