//! Agent sessions: the harness-neutral seam (plan §2.3, "Harness-neutral
//! seams").
//!
//! This module holds the **neutral outputs** only: the types a plugin's core
//! may know about an agent's session, whichever harness runs it. Everything
//! that reads one harness's files lives below it, one module per harness:
//!
//! - [`claude`]: Claude Code, the only harness so far. It reads the
//!   transcripts and computes status, the continuation chain and titles, and
//!   hands them out as a [`SessionView`].
//!
//! Nothing outside a harness module parses that harness's files or wire
//! format. There is deliberately no `Harness` trait: one is introduced when a
//! second harness is built, so a trait with one implementation does not encode
//! Claude Code's rules.
//!
//! The neutral types are [`SessionId`] (the one session-id validator), [`Ts`],
//! [`Status`] and [`SessionView`]. They are pure data.
//!
//! ```
//! use herdr_agent_kit::agent_session::{SessionId, Status};
//!
//! let id: SessionId = "0E4A6B1C-9D2F-4A3B-8C7D-1E2F3A4B5C6D".parse().unwrap();
//! assert_eq!(id.as_str(), "0e4a6b1c-9d2f-4a3b-8c7d-1e2f3a4b5c6d");
//! assert!(SessionId::parse("../etc/passwd").is_none());
//! assert_eq!(Status::Cancelled.code(), "cxl");
//! ```

use std::fmt;

use serde_json::Value;

pub mod claude;

/// An agent session id: a canonical 8-4-4-4-12 hex UUID.
///
/// **The one validator** (plan §1.3 row 4): any case in, lowercase out
/// (Claude Code, the one harness so far, names transcripts by the lowercase
/// id). Anything else, such as
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
        f.write_str("not a session id (a UUID)")
    }
}

impl std::error::Error for InvalidSessionId {}

/// A time: seconds since the Unix epoch, as a float (panorama's `wire.ts`
/// returns record times this way).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Ts(pub f64);

impl Ts {
    /// The seconds since the epoch.
    pub fn secs(self) -> f64 {
        self.0
    }
}

/// An agent's status, as the plugins show it.
///
/// Computed harness-side (for Claude Code by
/// [`claude::Session::status_of`]); the cores only read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Running.
    Run,
    /// Ended (inferred when nothing recorded the ending).
    Done,
    /// Failed.
    Failed,
    /// Cancelled (killed, stopped, abandoned by its workflow run).
    Cancelled,
    /// Its turn ended, background work it started is still running.
    Bg,
    /// Started, never stopped, and its harness process is gone.
    Lost,
}

impl Status {
    /// panorama's status code (`run`, `done`, `fail`, `cxl`, `bg`, `lost`).
    pub fn code(self) -> &'static str {
        match self {
            Status::Run => "run",
            Status::Done => "done",
            Status::Failed => "fail",
            Status::Cancelled => "cxl",
            Status::Bg => "bg",
            Status::Lost => "lost",
        }
    }
}

/// Everything a plugin's core may know about one agent session (plan §2.3,
/// "The neutral session view").
///
/// A harness module produces it (for Claude Code:
/// [`claude::Session::view`]); the cores consume it and never call harness
/// code, so their rules are the same for every harness.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
    /// The session.
    pub id: SessionId,
    /// Its status.
    pub status: Status,
    /// The session it was started from, when known.
    pub parent: Option<SessionId>,
    /// The sessions started from it, when known.
    pub children: Vec<SessionId>,
    /// Its title, when it has one.
    pub title: Option<String>,
    /// The sessions its conversation continued in, oldest first, each with
    /// the time of the continuation that led to it (`None`: the harness
    /// recorded no time). The session itself is not in the list.
    pub successors: Vec<(SessionId, Option<Ts>)>,
}
