use super::*;
use std::os::unix::fs::PermissionsExt;

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn makes_a_0700_dir_and_accepts_it() {
    let root = tempfile::tempdir().unwrap();
    chmod(root.path(), 0o700);
    let dir = root.path().join("state/sockets");
    make_private_dir(&dir, uid()).unwrap();
    let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode & 0o077, 0);
    // Again: it exists, still fine.
    make_private_dir(&dir, uid()).unwrap();
}

#[test]
fn refuses_open_modes_symlinks_files_and_other_owners() {
    let root = tempfile::tempdir().unwrap();
    chmod(root.path(), 0o700);
    let dir = root.path().join("s");
    fs::create_dir(&dir).unwrap();
    chmod(&dir, 0o750);
    assert_eq!(
        check_private(&dir, uid()),
        Err(NotPrivate("mode 750, not 700".into()))
    );
    // make_private_dir does not chmod an existing directory.
    assert!(matches!(
        make_private_dir(&dir, uid()),
        Err(PrivateDirError::NotPrivate(_))
    ));
    chmod(&dir, 0o700);
    assert_eq!(check_private(&dir, uid()), Ok(()));

    let link = root.path().join("l");
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    assert_eq!(
        check_private(&link, uid()),
        Err(NotPrivate("a symlink".into()))
    );

    let file = root.path().join("f");
    fs::write(&file, "").unwrap();
    assert_eq!(
        check_private(&file, uid()),
        Err(NotPrivate("not a directory".into()))
    );

    if uid() != 0 {
        // Another user's view: our parent is not theirs (root's would be).
        assert_eq!(
            check_private(&dir, uid() + 1),
            Err(NotPrivate(format!("in a directory of uid {}", uid())))
        );
    }

    let missing = root.path().join("missing");
    assert!(check_private(&missing, uid()).is_err());
}

#[test]
fn refuses_a_parent_others_can_write_unless_sticky() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("s");
    fs::create_dir(&dir).unwrap();
    chmod(&dir, 0o700);
    chmod(root.path(), 0o777);
    assert_eq!(
        check_private(&dir, uid()),
        Err(NotPrivate(
            "in a directory others can write in (mode 777, not sticky)".into()
        ))
    );
    chmod(root.path(), 0o1777);
    assert_eq!(check_private(&dir, uid()), Ok(()));
    chmod(root.path(), 0o700);
}
