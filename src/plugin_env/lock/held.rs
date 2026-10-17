//! [`HeldLock`]: a lock this process holds, released on drop.

use std::fs::File;
use std::path::{Path, PathBuf};

/// A lock this process holds. Dropping it (or [`HeldLock::release`]) releases
/// the lock.
#[derive(Debug)]
pub struct HeldLock {
    pub(super) file: File,
    pub(super) path: PathBuf,
}

impl HeldLock {
    /// The lock file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open lock file. It is opened close-on-exec, so the lock is not
    /// inherited by a child process. A duplicate of it (`try_clone`) does not
    /// extend the lock: dropping the `HeldLock` releases it on the shared open
    /// file description, so the lock lives exactly as long as the `HeldLock`.
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Release the lock now (`LOCK_UN`, then close).
    pub fn release(self) {
        drop(self);
    }
}

impl Drop for HeldLock {
    /// `LOCK_UN` before the close: see the [`lock`](super) module docs.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
