//! The continuation rule (D6), one implementation for both plugins. **Pure.**
//!
//! Claude Code can move a conversation to a new session id inside the same
//! process. The old (main) transcript then holds a record
//! `{"type": "continued-in", "sessionId": OLD, "continuedInSessionId": NEW}`,
//! usually followed by bookkeeping records; nothing in NEW points back, and
//! herdr and the process environment keep reporting OLD.
//!
//! **The rule** (plan D6; herdr-panorama `scan/session.py` and herdr-inbox
//! `transcript.continued_in`, pinned by the bridged rules table):
//!
//! 1. A `continued-in` record counts only in the session's **main**
//!    transcript (never a subagent's file), only when it is not marked
//!    `isSidechain: true`, and only when `continuedInSessionId` is a session
//!    id ([`SessionId`]) other than the session's own id, both compared in
//!    canonical lowercase. Any other `continued-in` record is ignored: it
//!    neither sets nor clears anything.
//! 2. Its time is the record's own `timestamp`. An undated one takes the time
//!    of the last **dated** record of any type before it in the file (the
//!    record's own, top-level `timestamp`; a nested one never dates a record),
//!    or no time when there is none. A time of `0` counts as no time here, as
//!    it does in both Pythons' `ts or last` (panorama's backward search took
//!    a `0` as a time; this follows the inbox's `if when: last = when`). A
//!    conversation record dated `0` is still dated for rule 3.
//! 3. It is cleared only by a later conversation record: a `user` or
//!    `assistant` record whose own time is later than the continuation's (the
//!    old session was resumed with `claude --resume`, so it is live again). A
//!    continuation with no time is cleared by any `user` or `assistant`
//!    record. Bookkeeping (`last-prompt`, `cost-state`, `mode`,
//!    `file-history-snapshot`, and any type Claude Code adds later) never
//!    clears it, and neither does a conversation record without a time, or at
//!    or before the continuation's time.
//! 4. A later accepted `continued-in` replaces an earlier one.
//!
//! **Forward only.** [`chain`] follows continuations forward from a session
//! (the session, then the one it continued in, and so on). There is no
//! helper that walks backwards to the sessions that continued **into** a
//! session: an ancestor is a different, possibly resumed, conversation and is
//! never assumed to be the same agent.

use super::SessionId;
use super::Ts;
use super::wire::{Line, contains};

/// Continuations followed at most from one session (panorama's
/// `collect.CONTINUE_HOPS`).
pub const MAX_HOPS: usize = 8;

/// The continuation state of one session's **main** transcript, folded line
/// by line. Feed it every line of the file in order, in batches as they are
/// appended ([`super::io::Tailer`]); [`Continuation::reset`] when the file
/// was rewritten.
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

/// Where a session's transcript says its conversation went: the IO side of
/// [`chain`] ([`super::io::Transcripts`] reads the files).
pub trait TranscriptFiles {
    /// `None` when no transcript of `id` is found; otherwise the session its
    /// conversation continued in now (`Some(None)`: not continued).
    fn continued_in(&mut self, id: &SessionId) -> Option<Option<SessionId>>;

    /// The time of the continuation [`TranscriptFiles::continued_in`] last
    /// reported for `id` ([`Continuation::at`]); `None` when it has no time,
    /// or when this source does not know times (the default).
    fn continued_at(&mut self, id: &SessionId) -> Option<Ts> {
        let _ = id;
        None
    }
}

/// One session on a forward chain.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Hop {
    /// The session.
    pub id: SessionId,
    /// Its transcript was found. Only the last hop can be unfound: a session
    /// named by a continuation whose transcript is not written yet, or lives
    /// on another machine.
    pub found: bool,
    /// The time of the continuation that led here from the previous hop
    /// ([`TranscriptFiles::continued_at`]); always `None` on the first hop.
    pub at: Option<Ts>,
}

impl Hop {
    /// A hop: session `id`, whether its transcript was `found`, and the time
    /// of the continuation that led to it (`at`).
    pub fn new(id: SessionId, found: bool, at: Option<Ts>) -> Hop {
        Hop { id, found, at }
    }
}

/// `start` and every session its conversation continued in, in order:
/// **forward only**, never the sessions that continued into `start`.
///
/// Stops at the first repeat (a loop), at a session that did not continue, at
/// a session whose transcript is not found (it is still the last hop, with
/// `found: false`), or after `max_hops` continuations (at most
/// `max_hops + 1` hops).
///
/// The plugins read it differently, and both readings are views of this
/// result:
///
/// - panorama's `Collector.live_session` (the session whose transcript it
///   reads now): the hops with `found: true`, ending at the last of them;
/// - the inbox's `transcript.follow` (on herdr-inbox main when this was
///   written): **the last hop, found or not** (`hops.last()`). In the window
///   between a `continued-in` record and the new session's first write, that
///   is the new, not yet written session, not the old one. Which sessions an
///   inbox delivery may target is the inbox's own merged rule (a forward
///   family), not this function; a port builds it from forward walks;
/// - every id on the way, including a next one not written yet (panorama's
///   `Collector.continuations`, the inbox's `transcript.chain`): all hops.
///
/// Which view a port uses is that port's parity decision; this function
/// does not pick one.
///
/// The inbox's Python `chain` stopped one hop earlier (`max_hops = 8` ids,
/// panorama 9); this follows panorama.
pub fn chain(start: &SessionId, files: &mut impl TranscriptFiles, max_hops: usize) -> Vec<Hop> {
    let mut out: Vec<Hop> = Vec::new();
    let mut current = start.clone();
    let mut at: Option<Ts> = None;
    loop {
        if out.iter().any(|h| h.id == current) {
            break;
        }
        let Some(next) = files.continued_in(&current) else {
            out.push(Hop {
                id: current,
                found: false,
                at,
            });
            break;
        };
        let next_at = if next.is_some() {
            files.continued_at(&current)
        } else {
            None
        };
        out.push(Hop {
            id: current,
            found: true,
            at,
        });
        match next {
            Some(n) if out.len() <= max_hops => {
                current = n;
                at = next_at;
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sid(n: u8) -> SessionId {
        SessionId::parse(&format!("{n:08x}-0000-4000-8000-000000000000")).unwrap()
    }

    struct Map(HashMap<SessionId, Option<SessionId>>);

    impl TranscriptFiles for Map {
        fn continued_in(&mut self, id: &SessionId) -> Option<Option<SessionId>> {
            self.0.get(id).cloned()
        }
    }

    fn feed(c: &mut Continuation, lines: &[&str]) {
        c.feed(&lines.iter().map(|l| l.as_bytes()).collect::<Vec<_>>());
    }

    #[test]
    fn a_time_of_zero_is_no_time_for_the_continuation() {
        // both Pythons: `ts or last`, and the inbox's `if when: last = when`
        let own = sid(1);
        let to = sid(2);
        let cont = |ts: &str| {
            format!(r#"{{"type":"continued-in","timestamp":{ts},"continuedInSessionId":"{to}"}}"#)
        };
        // dated 0: takes the last dated record's time (10), so a resume at 5 does not clear it
        let mut c = Continuation::new(own.clone());
        feed(
            &mut c,
            &[
                r#"{"type":"user","timestamp":10}"#,
                &cont("0"),
                r#"{"type":"user","timestamp":5}"#,
            ],
        );
        assert_eq!(c.next(), Some(&to));
        assert_eq!(c.at(), Some(Ts(10.0)));
        // a record dated 0 does not date an undated continuation
        let mut c = Continuation::new(own.clone());
        feed(
            &mut c,
            &[
                r#"{"type":"user","timestamp":10}"#,
                r#"{"type":"mode","timestamp":"0"}"#,
                &cont("null"),
            ],
        );
        assert_eq!(c.at(), Some(Ts(10.0)));
        // a conversation record dated 0 is still dated: not later than 10
        feed(&mut c, &[r#"{"type":"user","timestamp":0}"#]);
        assert_eq!(c.next(), Some(&to));
    }

    #[test]
    fn a_resume_holding_a_lone_surrogate_still_clears_it() {
        // Node writes a lone surrogate escape when it cuts a string inside an emoji
        let mut c = Continuation::new(sid(1));
        let to = sid(2);
        feed(
            &mut c,
            &[
                &format!(
                    r#"{{"type":"continued-in","timestamp":10,"continuedInSessionId":"{to}"}}"#
                ),
                r#"{"type":"user","timestamp":20,"message":{"role":"user","content":"cut \ud83d"}}"#,
            ],
        );
        assert_eq!(c.next(), None);
    }

    #[test]
    fn chain_follows_forward_and_marks_an_unwritten_target() {
        let mut files = Map(HashMap::from([
            (sid(1), Some(sid(2))),
            (sid(2), Some(sid(3))),
        ]));
        let hops = chain(&sid(1), &mut files, MAX_HOPS);
        assert_eq!(
            hops,
            vec![
                Hop::new(sid(1), true, None),
                Hop::new(sid(2), true, None),
                Hop::new(sid(3), false, None)
            ]
        );
        // forward only: from the middle, the ancestor is not part of the chain
        let hops = chain(&sid(2), &mut files, MAX_HOPS);
        assert_eq!(
            hops.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
            vec![sid(2), sid(3)]
        );
    }

    #[test]
    fn chain_stops_at_a_loop_and_after_max_hops() {
        let mut files = Map(HashMap::from([
            (sid(1), Some(sid(2))),
            (sid(2), Some(sid(1))),
        ]));
        assert_eq!(chain(&sid(1), &mut files, MAX_HOPS).len(), 2);
        let mut long = Map((1..=9).map(|n| (sid(n), Some(sid(n + 1)))).collect());
        let hops = chain(&sid(1), &mut long, 3);
        assert_eq!(hops.len(), 4);
        assert!(hops.iter().all(|h| h.found));
        let hops = chain(&sid(1), &mut long, MAX_HOPS);
        assert_eq!(hops.len(), MAX_HOPS + 1);
        let mut none = Map(HashMap::new());
        assert_eq!(
            chain(&sid(1), &mut none, MAX_HOPS),
            vec![Hop::new(sid(1), false, None)]
        );
    }
}
