//! The private directory a Unix socket lives in (panorama `core.private_dir`
//! and `Core._start`).
//!
//! A socket and its lock are only as safe as their directory: it must be a
//! real directory (not a symlink), owned by us, with no group or world access,
//! in a parent no other user can rename it out of.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::Path;

/// The sticky bit (`S_ISVTX`, 0o1000 on Linux and macOS; `mode_t` differs in
/// width between them, so it is spelled out).
const S_ISVTX: u32 = 0o1000;

/// Why a directory is not a safe home for a socket (the Python messages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotPrivate(pub String);

impl std::fmt::Display for NotPrivate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotPrivate {}

/// `Ok` if `dir` is a safe home for a socket of user `uid`:
///
/// - its parent is owned by `uid` or root, and either not writable by group
///   or others, or sticky (`/tmp`);
/// - `dir` itself is a directory, not a symlink, owned by `uid`, with no
///   group or world permission bits.
pub fn check_private(dir: &Path, uid: u32) -> Result<(), NotPrivate> {
    let parent = dir.parent().unwrap_or(dir);
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let up = fs::metadata(parent).map_err(|e| NotPrivate(format!("its parent: {e}")))?;
    if up.uid() != uid && up.uid() != 0 {
        return Err(NotPrivate(format!("in a directory of uid {}", up.uid())));
    }
    if up.mode() & 0o022 != 0 && up.mode() & S_ISVTX == 0 {
        return Err(NotPrivate(format!(
            "in a directory others can write in (mode {:o}, not sticky)",
            up.mode() & 0o7777
        )));
    }
    let st = fs::symlink_metadata(dir).map_err(|e| NotPrivate(e.to_string()))?;
    if st.file_type().is_symlink() {
        return Err(NotPrivate("a symlink".into()));
    }
    if !st.is_dir() {
        return Err(NotPrivate("not a directory".into()));
    }
    if st.uid() != uid {
        return Err(NotPrivate(format!("owned by uid {}", st.uid())));
    }
    if st.mode() & 0o077 != 0 {
        return Err(NotPrivate(format!(
            "mode {:o}, not 700",
            st.mode() & 0o7777
        )));
    }
    Ok(())
}

/// What [`make_private_dir`] could not do.
#[derive(Debug)]
pub enum PrivateDirError {
    /// The directory (or its parent) could not be created.
    Create(io::Error),
    /// It exists but is not private.
    NotPrivate(NotPrivate),
}

impl std::fmt::Display for PrivateDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrivateDirError::Create(e) => write!(f, "no socket directory: {e}"),
            PrivateDirError::NotPrivate(why) => write!(f, "the socket directory is {why}"),
        }
    }
}

impl std::error::Error for PrivateDirError {}

/// Make `dir` (mode 0700, its parents as needed) unless it exists, then
/// [`check_private`] it. An existing directory is never chmod-ed: one that is
/// not private is refused, as panorama's core refuses to start.
pub fn make_private_dir(dir: &Path, uid: u32) -> Result<(), PrivateDirError> {
    if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(PrivateDirError::Create)?;
    }
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(PrivateDirError::Create(e)),
    }
    check_private(dir, uid).map_err(PrivateDirError::NotPrivate)
}

#[cfg(test)]
mod tests {
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
}
