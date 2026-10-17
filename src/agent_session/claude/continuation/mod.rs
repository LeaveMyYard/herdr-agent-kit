//! Continuation (D6), one implementation for both plugins. **Pure.**
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
//!    id ([`SessionId`](crate::agent_session::SessionId)) other than the
//!    session's own id, both compared in canonical lowercase. Any other
//!    `continued-in` record is ignored: it neither sets nor clears anything.
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
//! The module holds two concepts, one submodule each: **the rule**
//! ([`Continuation`], whether and since when one main transcript says its
//! conversation continued elsewhere) and **chain-following** ([`chain`], the
//! sessions a conversation went through, asking a [`TranscriptFiles`] source
//! one [`Lookup`] per session).
//!
//! **Forward only.** [`chain`] follows continuations forward from a session
//! (the session, then the one it continued in, and so on). There is no
//! helper that walks backwards to the sessions that continued **into** a
//! session: an ancestor is a different, possibly resumed, conversation and is
//! never assumed to be the same agent.

mod follow;
mod rule;

pub use follow::{Hop, Lookup, MAX_HOPS, TranscriptFiles, chain};
pub use rule::Continuation;
