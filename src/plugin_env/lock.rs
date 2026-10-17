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

/// How to open a lock file. Opening never takes the lock; the methods do.
#[derive(Clone, Debug)]
pub struct LockFile {
    path: PathBuf,
    mode: u32,
    read_write: bool,
    no_follow: bool,
    create_parents: bool,
}

/// A lock this process holds. Dropping it (or [`HeldLock::release`]) releases
/// the lock.
#[derive(Debug)]
pub struct HeldLock {
    file: File,
    path: PathBuf,
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
    /// `LOCK_UN` before the close: see the module docs.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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
    /// Rust worker [`super::inbox::WORKER_LOCK_RETRY`].
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
/// the close: see the module docs.
fn shared_probe(file: &File) -> bool {
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn exclusive_excludes_a_second_open_file_in_the_same_process() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("a/b/worker.lock"));
        let held = lock.try_exclusive().unwrap().expect("free");
        assert!(lock.try_exclusive().unwrap().is_none());
        assert!(lock.held_elsewhere());
        assert!(lock.held_elsewhere_if_exists());
        held.release();
        assert!(!lock.held_elsewhere());
        assert!(!lock.held_elsewhere_if_exists());
        let again = lock.try_exclusive().unwrap();
        assert!(again.is_some());
        drop(again);
        assert!(lock.try_exclusive().unwrap().is_some());
    }

    // A duplicated descriptor shares the open file description, exactly as
    // the copy a child forked by another thread holds until its exec (kit
    // issue #9). The lock must not outlive our own release because of it.

    #[test]
    fn a_dropped_lock_is_released_while_a_copy_of_its_descriptor_lives() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("x.lock"));
        let held = lock.try_exclusive().unwrap().unwrap();
        let forked_copy = held.file().try_clone().unwrap();
        drop(held);
        assert!(!lock.held_elsewhere());
        assert!(lock.try_exclusive().unwrap().is_some());

        let held = lock.try_exclusive().unwrap().unwrap();
        let forked_copy_2 = held.file().try_clone().unwrap();
        held.release();
        assert!(lock.try_exclusive().unwrap().is_some());
        drop((forked_copy, forked_copy_2));
    }

    #[test]
    fn a_shared_probe_is_released_while_a_copy_of_its_descriptor_lives() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("x.lock"));
        let file = lock.open().unwrap();
        let forked_copy = file.try_clone().unwrap();
        assert!(!shared_probe(&file));
        drop(file);
        assert!(lock.try_exclusive().unwrap().is_some());
        drop(forked_copy);
    }

    #[test]
    fn a_shared_probe_creates_the_file_and_an_exclusive_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("x.lock"));
        assert!(!lock.held_elsewhere_if_exists());
        assert!(!lock.path().exists());
        assert!(!lock.held_elsewhere());
        assert!(lock.path().exists());
        // A probe never makes the directory (Python opens without mkdir).
        let nested = LockFile::new(dir.path().join("no/such.lock"));
        assert!(!nested.held_elsewhere());
        assert!(!dir.path().join("no").exists());
    }

    #[test]
    fn exclusive_within_waits_then_gives_up() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("x.lock"));
        let _held = lock.try_exclusive().unwrap().unwrap();
        let start = Instant::now();
        let got = lock
            .exclusive_within(
                Duration::from_millis(120),
                Duration::from_millis(20),
                || true,
            )
            .unwrap();
        assert!(got.is_none());
        assert!(start.elapsed() >= Duration::from_millis(120));
        let mut tries = 0;
        let got = lock
            .exclusive_within(Duration::from_secs(5), Duration::from_millis(1), || {
                tries += 1;
                tries < 3
            })
            .unwrap();
        assert!(got.is_none());
        assert_eq!(tries, 3);
    }

    #[test]
    fn exclusive_within_gets_a_lock_released_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::new(dir.path().join("x.lock"));
        let held = lock.try_exclusive().unwrap().unwrap();
        let t = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            drop(held);
        });
        let got = lock
            .exclusive_within(Duration::from_secs(5), Duration::from_millis(10), || true)
            .unwrap();
        assert!(got.is_some());
        t.join().unwrap();
    }

    #[test]
    fn private_lock_is_0600_and_refuses_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.lock");
        let held = LockFile::private(&path).try_exclusive().unwrap().unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        drop(held);
        let link = dir.path().join("link.lock");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let err = LockFile::private(&link).try_exclusive().unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::ELOOP));
        // The private form does not make parents.
        assert!(
            LockFile::private(dir.path().join("no/such.lock"))
                .try_exclusive()
                .is_err()
        );
    }
}
