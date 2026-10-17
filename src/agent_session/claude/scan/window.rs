//! The done window: which never-read agent files a pass may skip, and when a
//! skipped one is read after all.

use std::fs;

use super::disk::{mtime, mtime_of};
use super::{BG_MARK, Scanner};
use crate::agent_session::Ts;
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::claude::wire::contains;

impl Scanner {
    /// Whether file `i` lies outside the window starting at `since`: an agent
    /// file never read and last written before it. `None` when its metadata
    /// cannot be read (it is neither read nor skipped).
    pub(super) fn outside_window(&self, i: usize, since: Option<Ts>) -> Option<bool> {
        let f = &self.files[i];
        let Some(since) = since else {
            return Some(false);
        };
        if f.owner == Owner::Main || f.tail.identity().is_some() {
            return Some(false);
        }
        match fs::metadata(&f.path) {
            Ok(m) => Some(mtime_of(&m) < since.0),
            Err(_) => None,
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
