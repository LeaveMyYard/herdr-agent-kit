//! `flock(2)` locks, opened and probed the way the Python plugins do it.
//!
//! Python's `fcntl.flock` and std's [`File::lock`] family both call
//! `flock(2)` on Linux and macOS, so a lock held by a Python process blocks a
//! Rust process on the same path and the other way round (the tests prove it
//! against a real Python process). A lock is held by an open file description:
//! dropping the [`HeldLock`] releases it (`LOCK_UN`, then close), and so does
//! the process dying.
//!
//! Every release is an explicit `LOCK_UN`, never just the close. Files are
//! opened close-on-exec, but a child forked by another thread holds a copy of
//! every descriptor between its fork and its exec, and that copy keeps the
//! open file description, and the lock on it, alive after this process closes
//! its own. `LOCK_UN` releases the lock on the description itself, whoever
//! still has it open. (Without it a probe or a dropped lock could linger for
//! the instant a sibling thread spends spawning a process: kit issue #9.)
//!
//! The open modes follow each Python call site:
//!
//! - [`LockFile::new`]: `open(path, "a")` (create, append, mode 0666 less the
//!   umask): the inbox worker, rewake, popup and marker locks;
//! - [`LockFile::private`]: `os.open(path, O_RDWR | O_CREAT | O_NOFOLLOW,
//!   0o600)`: panorama's core lock, inside its private socket directory;
//! - [`LockFile::mode`] for anything else (panorama's home lock is 0644).
//!
//! The waiting calls ([`LockFile::exclusive`], [`LockFile::exclusive_within`])
//! block the thread: from async code run them in `spawn_blocking`.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

mod held;
mod probe;

pub use held::HeldLock;

/// How to open a lock file. Opening never takes the lock; the methods do.
#[derive(Clone, Debug)]
pub struct LockFile {
    path: PathBuf,
    mode: u32,
    read_write: bool,
    no_follow: bool,
    create_parents: bool,
}

impl LockFile {
    /// `open(path, "a")`: created if missing (mode 0666 less the umask),
    /// opened for appending, its parent directories created first (the inbox
    /// worker's `path.parent.mkdir(parents=True)`).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        LockFile {
            path: path.into(),
            mode: 0o666,
            read_write: false,
            no_follow: false,
            create_parents: true,
        }
    }

    /// `O_RDWR | O_CREAT | O_NOFOLLOW`, mode 0600, parents not created:
    /// panorama's core lock, which lives in a directory that
    /// [`super::private_dir::make_private_dir`] made and checked. A symlink in
    /// its place is refused (`ELOOP`).
    pub fn private(path: impl Into<PathBuf>) -> Self {
        LockFile {
            path: path.into(),
            mode: 0o600,
            read_write: true,
            no_follow: true,
            create_parents: false,
        }
    }

    /// The same, created with `mode` (less the umask) instead.
    pub fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    /// The lock file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open(&self) -> io::Result<File> {
        self.open_with(self.create_parents)
    }

    fn open_with(&self, create_parents: bool) -> io::Result<File> {
        if create_parents
            && let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut opts = OpenOptions::new();
        opts.create(true).mode(self.mode);
        if self.read_write {
            opts.read(true).write(true);
        } else {
            opts.append(true);
        }
        if self.no_follow {
            opts.custom_flags(libc::O_NOFOLLOW);
        }
        opts.open(&self.path)
    }

    /// `LOCK_EX | LOCK_NB`: the lock, or `Ok(None)` when another open file
    /// holds it. `Err` only when the file cannot be opened or `flock` fails
    /// for another reason.
    pub fn try_exclusive(&self) -> io::Result<Option<HeldLock>> {
        let file = self.open()?;
        match file.try_lock() {
            Ok(()) => Ok(Some(HeldLock {
                file,
                path: self.path.clone(),
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// `LOCK_EX | LOCK_NB`, retried every `poll` for up to `wait` while
    /// `keep_trying()` says so: the lock, or `Ok(None)` if it stayed busy (or
    /// `keep_trying` gave up). One file is opened and kept for all the tries,
    /// as the Python loops do.
    ///
    /// The Python waits: the rewake watcher 5 s every 0.1 s (and gives up
    /// when another watcher takes ownership), panorama's home lock
    /// `LOCK_SECONDS` every 0.02 s, the tab-label lock 10 s every 0.05 s, the
    /// Rust worker `internal::plugin_paths::inbox::WORKER_LOCK_RETRY`.
    pub fn exclusive_within(
        &self,
        wait: Duration,
        poll: Duration,
        mut keep_trying: impl FnMut() -> bool,
    ) -> io::Result<Option<HeldLock>> {
        let file = self.open()?;
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(Some(HeldLock {
                        file,
                        path: self.path.clone(),
                    }));
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(e)) => return Err(e),
            }
            if Instant::now() >= deadline || !keep_trying() {
                return Ok(None);
            }
            thread::sleep(poll);
        }
    }

    /// `LOCK_EX`, waiting as long as it takes (panorama's per-session event
    /// file lock).
    pub fn exclusive(&self) -> io::Result<HeldLock> {
        let file = self.open()?;
        file.lock()?;
        Ok(HeldLock {
            file,
            path: self.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
