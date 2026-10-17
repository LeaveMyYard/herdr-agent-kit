//! The done window: which never-read agent files a pass may skip, and when a
//! skipped one is read after all.

use std::fs;

use super::disk::{mtime, mtime_of};
use super::{BG_MARK, Scanner};
use crate::agent_session::Ts;
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::claude::wire::contains;

/// Where a file lies relative to the done window.
pub(super) enum Window {
    /// Read it.
    Inside,
    /// Skip it (it may be read after all, [`Scanner::still_waiting`]).
    Outside,
    /// Its metadata cannot be read: neither read nor skipped.
    Unreadable,
}

impl Scanner {
    /// Where file `i` lies relative to the window starting at `since`: an
    /// agent file never read and last written before it is outside.
    pub(super) fn window_of(&self, i: usize, since: Option<Ts>) -> Window {
        let f = &self.files[i];
        let Some(since) = since else {
            return Window::Inside;
        };
        if f.owner == Owner::Main || f.tail.identity().is_some() {
            return Window::Inside;
        }
        match fs::metadata(&f.path) {
            Ok(m) if mtime_of(&m) < since.0 => Window::Outside,
            Ok(_) => Window::Inside,
            Err(_) => Window::Unreadable,
        }
    }

    /// Whether skipped file `i` must be read after all: an old file of an
    /// agent nobody saw end (no notice, no completed acknowledgement) can
    /// still be waiting on its own background work, so it is read when it
    /// holds a background launch (a byte search, once per file version).
    pub(super) fn still_waiting(&mut self, i: usize) -> bool {
        let Owner::Agent(owner) = &self.files[i].owner else {
            return false;
        };
        let ended = self.session.evidence().notices(owner).next().is_some()
            || self
                .session
                .parentage()
                .ack(owner)
                .is_some_and(|a| a.status == "completed");
        if ended {
            return false;
        }
        let path = self.files[i].path.clone();
        if !self
            .bg_scanned
            .insert((path.clone(), mtime(&path).0.to_bits()))
        {
            return false;
        }
        fs::read(&path).is_ok_and(|data| contains(&data, BG_MARK))
    }
}
