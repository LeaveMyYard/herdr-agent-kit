//! [`Transcripts`]: main transcripts under one projects root, followed
//! incrementally, for [`chain`](crate::agent_session::claude::chain).

use std::collections::HashMap;
use std::path::PathBuf;

use super::{Follower, ProjectsRoot};
use crate::agent_session::claude::continuation::TranscriptFiles;
use crate::agent_session::{SessionId, Ts};

/// Followers kept by one [`Transcripts`] (least recently used dropped first).
pub const FOLLOWERS_MAX: usize = 256;

/// Main transcripts under one projects root, followed incrementally:
/// [`TranscriptFiles`] for [`super::chain`](crate::agent_session::claude::chain).
#[derive(Debug)]
pub struct Transcripts {
    root: ProjectsRoot,
    cwd: Option<String>,
    followers: HashMap<SessionId, (Follower, u64)>,
    known: HashMap<SessionId, PathBuf>,
    tick: u64,
}

impl Transcripts {
    /// Transcripts under `root`; `cwd` (the agent's working directory, when
    /// known) is searched first.
    pub fn new(root: ProjectsRoot, cwd: Option<String>) -> Transcripts {
        Transcripts {
            root,
            cwd,
            followers: HashMap::new(),
            known: HashMap::new(),
            tick: 0,
        }
    }

    /// The projects root.
    pub fn root(&self) -> &ProjectsRoot {
        &self.root
    }

    /// Records where a session's main transcript is (the path its
    /// `SessionStart` hook reported); while it is a file it is preferred over
    /// any path found by search, as the inbox's `find(id, known)` did.
    pub fn remember(&mut self, id: SessionId, path: PathBuf) {
        self.known.insert(id, path);
    }

    /// The main transcript of a session: the remembered path while it is a
    /// file, else [`ProjectsRoot::find`].
    pub fn find(&self, id: &SessionId) -> Option<PathBuf> {
        if let Some(path) = self.known.get(id).filter(|p| p.is_file()) {
            return Some(path.clone());
        }
        self.root.find(id, self.cwd.as_deref())
    }

    fn follower(&mut self, id: &SessionId) -> Option<&mut Follower> {
        self.tick += 1;
        let tick = self.tick;
        let remembered = self.known.get(id).filter(|p| p.is_file()).cloned();
        let current = remembered.or_else(|| {
            self.followers
                .get(id)
                .map(|(f, _)| f.path().to_path_buf())
                .filter(|p| p.is_file())
        });
        let path = match current {
            Some(path) => path,
            None => self.find(id)?,
        };
        if self.followers.get(id).is_none_or(|(f, _)| f.path() != path) {
            if self.followers.len() >= FOLLOWERS_MAX
                && let Some(old) = self
                    .followers
                    .iter()
                    .min_by_key(|(_, (_, t))| *t)
                    .map(|(k, _)| k.clone())
            {
                self.followers.remove(&old);
            }
            self.followers
                .insert(id.clone(), (Follower::new(path, id.clone()), tick));
        }
        let entry = self.followers.get_mut(id)?;
        entry.1 = tick;
        Some(&mut entry.0)
    }
}

impl TranscriptFiles for Transcripts {
    fn continued_in(&mut self, id: &SessionId) -> Option<Option<SessionId>> {
        let follower = self.follower(id)?;
        if !follower.update() {
            return None;
        }
        Some(follower.continued_in().cloned())
    }

    fn continued_at(&mut self, id: &SessionId) -> Option<Ts> {
        self.followers.get(id).and_then(|(f, _)| f.continued_at())
    }
}
