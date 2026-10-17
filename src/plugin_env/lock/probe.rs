//! Probes: is a lock held by someone else? Each probe takes the lock without
//! waiting and releases it at once.

use std::fs::{File, OpenOptions};

use super::LockFile;

impl LockFile {
    /// Is the lock held by someone else? Probed with `LOCK_SH | LOCK_NB` on a
    /// file opened (and so created, but never its directory) as for taking
    /// it, released at once: the inbox's `worker.is_running` and
    /// `deeplink.popup_is_running`.
    ///
    /// A file that cannot be opened reads as "not held"; any `flock` failure
    /// reads as "held", as in Python (`except OSError: return True`). A shared
    /// probe never blocks a holder's own exclusive retry for longer than the
    /// instant it takes.
    pub fn held_elsewhere(&self) -> bool {
        let Ok(file) = self.open_with(false) else {
            return false;
        };
        shared_probe(&file)
    }

    /// Is the lock held by someone else? Probed with `LOCK_EX | LOCK_NB` and
    /// released at once, and only if the file already exists: the inbox's
    /// `rewake.watcher_alive`. A missing or unopenable file reads as "not
    /// held"; any `flock` failure as "held".
    pub fn held_elsewhere_if_exists(&self) -> bool {
        if !self.path.exists() {
            return false;
        }
        let Ok(file) = OpenOptions::new().append(true).open(&self.path) else {
            return false;
        };
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(_) => true,
        }
    }
}

/// `LOCK_SH | LOCK_NB` on `file`, released at once: is the lock held by
/// another open file description? The release is an explicit `LOCK_UN`, not
/// the close: see the [`lock`](super) module docs.
pub(super) fn shared_probe(file: &File) -> bool {
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(_) => true,
    }
}
