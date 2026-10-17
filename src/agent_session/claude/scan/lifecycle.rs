//! Creating a [`Scanner`], its accessors, and registering a file to read.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::{FileState, Scanner};
use crate::agent_session::SessionId;
use crate::agent_session::claude::fold::{Owner, Session};
use crate::agent_session::tail::Tailer;

/// Construction and accessors.
impl Scanner {
    /// The session `id` whose main transcript is `root`. `cwd` shortens the
    /// paths in tool summaries; `hooks` is panorama's hook record file of the
    /// session, if any.
    pub fn new(id: SessionId, root: PathBuf, cwd: Option<String>, hooks: Option<PathBuf>) -> Self {
        Scanner {
            root,
            cwd,
            hooks,
            session: Session::new(id),
            files: Vec::new(),
            file_paths: HashSet::new(),
            journals: Vec::new(),
            journal_runs: HashSet::new(),
            meta_mtime: HashMap::new(),
            bg_scanned: HashSet::new(),
            hook_tail: Tailer::new(),
        }
    }

    /// The main transcript.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The fold of everything read so far.
    pub fn session(&self) -> &Session {
        &self.session
    }

    pub(super) fn add_file(&mut self, path: PathBuf, owner: Owner) {
        self.session.file_mut(owner.clone());
        self.file_paths.insert(path.clone());
        self.files.push(FileState {
            path,
            owner,
            tail: Tailer::new(),
            info_size: None,
        });
    }
}
