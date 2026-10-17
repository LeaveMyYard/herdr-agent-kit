//! [`Follower`]: the continuation state of one transcript file, read
//! incrementally.

use std::path::{Path, PathBuf};

use crate::agent_session::claude::continuation::Continuation;
use crate::agent_session::tail::{Tail, Tailer};
use crate::agent_session::{SessionId, Ts};

/// The most a [`Follower`] reads in one step.
pub const FOLLOW_CHUNK: u64 = 8 * 1024 * 1024;

/// The continuation state of one transcript file, read incrementally: the
/// inbox's `transcript.continued_in` and the continuation part of panorama's
/// `Session.update`.
///
/// Only a session's **main** transcript can continue: a follower of a file
/// inside a `subagents/` directory (`<session>/subagents/agent-*.jsonl`,
/// `<session>/subagents/workflows/<run>/agent-*.jsonl`) never reports one.
/// The whole file is read once, in steps of at most [`FOLLOW_CHUNK`] bytes
/// (so a large transcript never sits in memory whole), then only what is
/// appended; a rewritten file is read again from the start.
///
/// One record over [`MAX_PARTIAL`](crate::agent_session::tail::MAX_PARTIAL) (8 MB) is dropped when it spans a step
/// boundary (some records between 8 and 16 MB, every one above 16 MB), as
/// any record over the cap is when it is appended while being followed.
/// Python's cold read kept them. A `continued-in` record is a few hundred
/// bytes; a dropped huge `user` record cannot clear a continuation.
#[derive(Clone, Debug)]
pub struct Follower {
    path: PathBuf,
    tailer: Tailer,
    state: Option<Continuation>,
}

impl Follower {
    /// A follower of the transcript `path` of session `own`.
    pub fn new(path: impl Into<PathBuf>, own: SessionId) -> Follower {
        let path = path.into();
        let state = (!in_subagents_dir(&path)).then(|| Continuation::new(own));
        Follower {
            path,
            tailer: Tailer::new(),
            state,
        }
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads what was appended. `false` when the file is missing (the state
    /// read so far is kept).
    pub fn update(&mut self) -> bool {
        let mut resets = 0;
        let mut before = self.tailer.offset();
        loop {
            match self.tailer.read_at_most(&self.path, FOLLOW_CHUNK) {
                Tail::Missing => return false,
                Tail::NoChange => return true,
                Tail::Reset => {
                    if let Some(state) = &mut self.state {
                        state.reset();
                    }
                    before = 0;
                    resets += 1;
                    if resets > 2 {
                        return true; // replaced again and again: read on the next update
                    }
                }
                Tail::Lines(lines) => {
                    if let Some(state) = &mut self.state {
                        state.feed(&lines);
                    }
                    let progressed = self.tailer.offset() > before;
                    before = self.tailer.offset();
                    if !progressed || self.tailer.offset() >= self.tailer.size() {
                        return true;
                    }
                }
            }
        }
    }

    /// The session the conversation continued in, as far as read.
    pub fn continued_in(&self) -> Option<&SessionId> {
        self.state.as_ref().and_then(Continuation::next)
    }

    /// The time of that continuation ([`Continuation::at`]).
    pub fn continued_at(&self) -> Option<Ts> {
        self.state.as_ref().and_then(Continuation::at)
    }
}

/// `subagents` is one of the three directories above the file (the inbox's
/// rule: folders above the session never count).
fn in_subagents_dir(path: &Path) -> bool {
    path.parent()
        .map(|dir| {
            dir.components()
                .rev()
                .take(3)
                .any(|c| c.as_os_str() == "subagents")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
