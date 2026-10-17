//! [`ProjectsRoot`]: where Claude Code keeps transcripts, and the lookup of a
//! session's main transcript.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::agent_session::SessionId;

/// Claude Code's projects directory: `${CLAUDE_CONFIG_DIR:-~/.claude}/projects`.
/// Each project is a directory named by [`sanitize_cwd`] of its working
/// directory, holding one `<session>.jsonl` main transcript per session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectsRoot(PathBuf);

impl ProjectsRoot {
    /// A projects root at `path`.
    pub fn new(path: impl Into<PathBuf>) -> ProjectsRoot {
        ProjectsRoot(path.into())
    }

    /// The root this process's Claude Code uses: `$CLAUDE_CONFIG_DIR/projects`
    /// when the variable is set and not empty, else `~/.claude/projects`.
    /// `None` when neither is known.
    pub fn from_env() -> Option<ProjectsRoot> {
        ProjectsRoot::resolve(std::env::var_os("CLAUDE_CONFIG_DIR"), std::env::home_dir())
    }

    /// [`ProjectsRoot::from_env`] with the two inputs given: the value of
    /// `CLAUDE_CONFIG_DIR` and the home directory.
    pub fn resolve(config_dir: Option<OsString>, home: Option<PathBuf>) -> Option<ProjectsRoot> {
        let base = match config_dir.filter(|d| !d.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => home?.join(".claude"),
        };
        Some(ProjectsRoot(base.join("projects")))
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The project directory of a working directory.
    pub fn project_dir(&self, cwd: &str) -> PathBuf {
        self.0.join(sanitize_cwd(cwd))
    }

    /// The main transcript `<id>.jsonl` of a session: in `cwd`'s project
    /// directory first, then in every project directory (Claude Code shortens
    /// very long project names, so the name cannot always be derived).
    /// Panorama's `discovery.find_root` and the inbox's `transcript.find`.
    ///
    /// The id is a [`SessionId`], so the lookup never names a path outside the
    /// root. Project directories are searched in name order (Python used the
    /// directory's own order).
    pub fn find(&self, id: &SessionId, cwd: Option<&str>) -> Option<PathBuf> {
        let name = crate::agent_session::claude::transcript_file_name(id);
        if let Some(cwd) = cwd.filter(|c| !c.is_empty()) {
            let path = self.project_dir(cwd).join(&name);
            if path.is_file() {
                return Some(path);
            }
        }
        let mut dirs: Vec<PathBuf> = fs::read_dir(&self.0)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        dirs.sort();
        dirs.into_iter()
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    }
}

/// A project directory's name: the working directory with every character
/// that is not an ASCII letter or digit replaced by `-`.
pub fn sanitize_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests;
