//! Everything about Claude Code's session files, in one implementation.
//!
//! Discovery, tailing, the transcript wire format, continuation and
//! session-id validation, shared by herdr-panorama (`scan/`, `collect.py`) and
//! herdr-inbox (`transcript.py`). Split along the purity boundary:
//!
//! - **pure** (data in, data out; no IO, threads or environment):
//!   [`SessionId`] (the one session-id validator), [`wire`] (lenient record
//!   parsing, the marker prefilter, tool summaries, task notifications),
//!   [`continuation`] (the one continuation rule, D6, and forward-only
//!   chains) and [`fold`] (a session's keyed stores and the status rules);
//! - **IO**: [`io`] finds the projects root and transcripts, tails files
//!   ([`io::Tailer`]), summarizes a transcript's tail
//!   ([`io::summarize_tail`]) and follows continuations on disk
//!   ([`io::Transcripts`]); [`scan`] reads one session's files into a
//!   [`fold::Session`] ([`scan::Scanner`]).
//!
//! # Parity
//!
//! The behaviour follows the Python plugins on their `main` branches
//! (herdr-panorama `src/herdr_panorama/scan/`, herdr-inbox
//! `src/herdr_inbox/transcript.py`). The bridged rules table
//! (`tests/test_bridge_rules.py` in both repos) is ported case for case in
//! `tests/claude_session_bridge_rules.rs`. Where the two plugins differ, the
//! item says which one it follows and why.

use std::fmt;

use serde_json::Value;

pub mod continuation;
pub mod fold;
pub mod io;
pub mod scan;
pub mod wire;

pub use continuation::{Continuation, Hop, TranscriptFiles, chain};
pub use fold::{Owner, Session, Status, StatusOf};
pub use scan::Scanner;
pub use wire::{Line, Record, Ts};

/// A Claude Code session id: a canonical 8-4-4-4-12 hex UUID.
///
/// **The one validator** (plan §1.3 row 4): any case in, lowercase out
/// (Claude Code names transcripts by the lowercase id). Anything else, such as
/// an empty string, surrounding whitespace, a trailing newline, `..`, a path
/// separator or a glob character, is not a session id, so it never becomes a
/// file name, an rsync rule or a followed continuation.
///
/// Matches panorama's `discovery.canonical_session_id` and the inbox's
/// `transcript.session_id_or_none`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(String);

impl SessionId {
    /// The canonical (lowercase) id, or `None` when `value` is not exactly a
    /// UUID: ASCII hex digits in groups of 8-4-4-4-12, the whole string.
    pub fn parse(value: &str) -> Option<SessionId> {
        let b = value.as_bytes();
        if b.len() != 36 {
            return None;
        }
        let ok = b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        });
        ok.then(|| SessionId(value.to_ascii_lowercase()))
    }

    /// [`SessionId::parse`] of a JSON value: only a string can be a session id.
    pub fn from_value(value: &Value) -> Option<SessionId> {
        value.as_str().and_then(SessionId::parse)
    }

    /// The canonical (lowercase) text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The transcript file name Claude Code uses: `<id>.jsonl`.
    pub fn file_name(&self) -> String {
        format!("{}.jsonl", self.0)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionId({})", self.0)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for SessionId {
    type Err = InvalidSessionId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SessionId::parse(s).ok_or(InvalidSessionId)
    }
}

/// The error of [`SessionId`]'s `FromStr`: the text is not a session id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSessionId;

impl fmt::Display for InvalidSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a Claude Code session id (a UUID)")
    }
}

impl std::error::Error for InvalidSessionId {}
