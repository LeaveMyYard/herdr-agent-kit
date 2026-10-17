//! Reading one session's files into the [`fold`](super::fold). **IO.**
//!
//! panorama's `Session.update` and its readers (`scan/session.py`): the main
//! transcript, every subagent transcript and `meta.json`, the workflow
//! journals and run summaries, and the hook records, each tailed
//! incrementally ([`Tailer`]) so an update reads only what was appended.
//!
//! Performance as in panorama: a full-file pass only JSON-parses lines that
//! hold a marker ([`wire::wanted_lines`](super::wire::wanted_lines)); each agent's activity, pending
//! calls, last text and model come from the end of its file
//! ([`summarize_tail`](super::io::summarize_tail)), re-read only when the file grew; tool calls are a
//! byte count of `"type":"tool_use"`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::fold::{Owner, Session};
use crate::agent_session::tail::Tailer;

// The scanner itself: construction and accessors.
mod lifecycle;

// Reading: one pass, and one reader per file kind.
mod readers;
mod update;

// Supports: modification times and JSON object files.
mod disk;

/// A Bash moved to the background (its result record): an old agent file
/// holding it is read even when it is outside the done window.
pub(crate) const BG_MARK: &[u8] = br#""backgroundTaskId""#;

#[derive(Debug)]
struct FileState {
    path: PathBuf,
    owner: Owner,
    tail: Tailer,
    /// The file size its tail summary was read at.
    info_size: Option<u64>,
}

#[derive(Debug)]
struct Journal {
    run: String,
    path: PathBuf,
    tail: Tailer,
}

/// One session's files, read incrementally into a [`Session`].
#[derive(Debug)]
pub struct Scanner {
    root: PathBuf,
    cwd: Option<String>,
    hooks: Option<PathBuf>,
    session: Session,
    files: Vec<FileState>,
    /// The paths in `files`.
    file_paths: HashSet<PathBuf>,
    journals: Vec<Journal>,
    /// The runs in `journals`.
    journal_runs: HashSet<String>,
    meta_mtime: HashMap<String, f64>,
    bg_scanned: HashSet<(PathBuf, u64)>,
    hook_tail: Tailer,
}
