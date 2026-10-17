//! Chain-following: [`chain`] walks a conversation forward, session by
//! session, asking a [`TranscriptFiles`] source one [`Lookup`] per session.

use crate::agent_session::{SessionId, Ts};

/// Continuations followed at most from one session (panorama's
/// `collect.CONTINUE_HOPS`).
pub const MAX_HOPS: usize = 8;

/// What a session's transcript says about where its conversation went: one
/// answer of [`TranscriptFiles::lookup`].
///
/// `#[non_exhaustive]`: another harness may need an answer these three do not
/// cover, so a later minor version may add one. A match outside this crate
/// needs a `_` arm; an implementor builds the variants as usual. [`chain`]
/// counts a transcript as found only on [`Lookup::Ended`] or
/// [`Lookup::ContinuedIn`], follows only [`Lookup::ContinuedIn`], and stops at
/// any other answer.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Lookup {
    /// No transcript of the session is found (not written yet, or on another
    /// machine).
    NotFound,
    /// The transcript is found and the conversation did not continue
    /// elsewhere: it never did, or it was resumed since
    /// ([`Continuation::next`](super::Continuation::next) is `None`).
    Ended,
    /// The transcript is found and the conversation continued in `next`.
    /// `#[non_exhaustive]` so it may gain a field: build it with
    /// [`Lookup::continued_in`], match it with `{ next, at, .. }`.
    #[non_exhaustive]
    ContinuedIn {
        /// The session it continued in.
        next: SessionId,
        /// The time of that continuation
        /// ([`Continuation::at`](super::Continuation::at)): `None` when it has
        /// no time, or when the source does not know times.
        at: Option<Ts>,
    },
}

impl Lookup {
    /// [`Lookup::ContinuedIn`]: the conversation continued in `next`, at `at`
    /// (`None` when the continuation has no time or the source does not know
    /// times).
    pub fn continued_in(next: SessionId, at: Option<Ts>) -> Lookup {
        Lookup::ContinuedIn { next, at }
    }
}

/// Where a session's transcript says its conversation went: the IO side of
/// [`chain`] ([`Transcripts`](crate::agent_session::claude::io::Transcripts)
/// reads the files).
pub trait TranscriptFiles {
    /// The [`Lookup`] for session `id`, as its transcript reads now.
    fn lookup(&mut self, id: &SessionId) -> Lookup;
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
    /// ([`Lookup::ContinuedIn`]'s `at`); always `None` on the first hop.
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
        let lookup = files.lookup(&current);
        out.push(Hop {
            id: current,
            found: matches!(lookup, Lookup::Ended | Lookup::ContinuedIn { .. }),
            at,
        });
        match lookup {
            Lookup::ContinuedIn { next, at: next_at } if out.len() <= max_hops => {
                current = next;
                at = next_at;
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests;
