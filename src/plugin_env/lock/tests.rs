use super::probe::shared_probe;
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
