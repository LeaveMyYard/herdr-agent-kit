//! Claude Code: everything about its session files, in one implementation.
//!
//! The only harness implementation so far (plan §2.3, "Harness-neutral
//! seams"). Discovery, tailing, the transcript wire format, continuation,
//! the status rules and the neutral [`SessionView`](super::SessionView)
//! producer ([`Session::view`]), shared by herdr-panorama (`scan/`, `collect.py`) and
//! herdr-inbox (`transcript.py`). Split along the purity boundary:
//!
//! - **pure** (data in, data out; no IO, threads or environment):
//!   [`wire`] (lenient record
//!   parsing, the marker prefilter, tool summaries, task notifications),
//!   [`continuation`] (the one continuation rule, D6, and forward-only
//!   chains) and [`fold`] (a session's keyed stores and the status rules);
//! - **IO**: [`io`] finds the projects root and transcripts, tails them
//!   (with the neutral [`Tailer`](super::tail::Tailer)), summarizes a transcript's tail
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
//! `tests/agent_session_claude_bridge_rules.rs`. Where the two plugins differ, the
//! item says which one it follows and why.

use super::{SessionId, Ts};

pub mod continuation;
pub mod fold;
pub mod io;
pub mod scan;
mod view;
pub mod wire;

pub use continuation::{Continuation, Hop, TranscriptFiles, chain};
pub use fold::{Owner, Session, StatusOf};
pub use scan::Scanner;
pub use wire::{Line, Record};

/// The transcript file name Claude Code uses for a session: `<id>.jsonl`.
pub fn transcript_file_name(id: &SessionId) -> String {
    format!("{id}.jsonl")
}
