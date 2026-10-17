//! Transcript IO. **IO.**
//!
//! - [`ProjectsRoot`]: where Claude Code keeps transcripts
//!   (`${CLAUDE_CONFIG_DIR:-~/.claude}/projects`), and the lookup of a
//!   session's main transcript ([`ProjectsRoot::find`]).
//! - [`related`]: a session's subagent transcripts and workflow journals.
//! - [`Tailer`]: incremental reading of an append-only file ([`Tail`]).
//! - [`summarize_tail`]: activity, calls in flight, last text and model from
//!   the end of a transcript.
//! - [`Follower`] and [`Transcripts`]: the continuation rule
//!   ([`super::Continuation`]) applied to files on disk, incrementally, and
//!   forward chains over them ([`super::chain`]).
//!
//! Parsing and rules live in the pure modules ([`super::wire`],
//! [`super::continuation`]); this module only reads.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::SessionId;
use super::continuation::{Continuation, TranscriptFiles};
use super::wire::TailInfo;

// -- the projects root and discovery ------------------------------------------------------------

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
        let name = id.file_name();
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

/// One subagent transcript of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFile {
    /// The agent id (`agent-<id>.jsonl`).
    pub agent_id: String,
    /// Its transcript.
    pub transcript: PathBuf,
    /// Its `agent-<id>.meta.json` (it may not exist yet).
    pub meta: PathBuf,
    /// The workflow run it belongs to (`subagents/workflows/<run>/`).
    pub workflow: Option<String>,
}

fn session_dir(main_transcript: &Path) -> PathBuf {
    main_transcript.with_extension("")
}

/// `<session>/subagents/` next to a main transcript `<session>.jsonl`.
pub fn subagents_dir(main_transcript: &Path) -> PathBuf {
    session_dir(main_transcript).join("subagents")
}

/// The `agent-<id>.jsonl` files of a directory, by agent id. A missing
/// directory is empty (Claude Code creates them lazily). Names that are not
/// UTF-8 are skipped.
pub fn scan_agent_files(dir: &Path, workflow: Option<&str>) -> Vec<AgentFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<AgentFile> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let aid = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
            (!aid.is_empty()).then(|| AgentFile {
                agent_id: aid.to_string(),
                transcript: e.path(),
                meta: dir.join(format!("agent-{aid}.meta.json")),
                workflow: workflow.map(str::to_string),
            })
        })
        .collect();
    out.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
    out
}

/// The workflow run ids under `subagents/workflows/`, sorted.
pub fn scan_workflow_ids(subagents: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(subagents.join("workflows")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

/// A session's subagent files and workflow journals (`{run: journal path}`):
/// panorama's `discovery.related`.
pub fn related(main_transcript: &Path) -> (Vec<AgentFile>, Vec<(String, PathBuf)>) {
    let subs = subagents_dir(main_transcript);
    let mut files = scan_agent_files(&subs, None);
    let mut journals = Vec::new();
    for run in scan_workflow_ids(&subs) {
        let dir = subs.join("workflows").join(&run);
        files.extend(scan_agent_files(&dir, Some(&run)));
        journals.push((run, dir.join("journal.jsonl")));
    }
    (files, journals)
}

/// `<session>/workflows/<run>.json`: a workflow run's summary.
pub fn workflow_summary_path(main_transcript: &Path, run: &str) -> PathBuf {
    session_dir(main_transcript)
        .join("workflows")
        .join(format!("{run}.json"))
}

// -- tailing ----------------------------------------------------------------------------------

/// A partial line longer than this is dropped, and reading resyncs at the
/// next newline.
pub const MAX_PARTIAL: usize = 8 * 1024 * 1024;

/// What one [`Tailer::read`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tail {
    /// The file does not exist or cannot be read.
    Missing,
    /// Nothing was appended.
    NoChange,
    /// The file shrank or was replaced: forget everything derived from it,
    /// then read again (from the start).
    Reset,
    /// The complete lines appended since the last read (possibly none), CRLF
    /// and blank lines dropped.
    Lines(Vec<Vec<u8>>),
}

/// Incremental reading of an append-only file: panorama's `scan/tail.py`
/// (after zoetrope's `bytes.rs`, MIT). Stat, read the bytes appended past the
/// last offset, split on `\n`, return complete lines and buffer the trailing
/// partial line. A file that shrank or was replaced (its `(dev, inode)`
/// changed) resets to offset 0. Lines are bytes: callers decode only the
/// lines they parse.
#[derive(Clone, Debug, Default)]
pub struct Tailer {
    offset: u64,
    partial: Vec<u8>,
    overflowed: bool,
    identity: Option<(u64, u64)>,
    mtime: Option<SystemTime>,
    size: u64,
}

impl Tailer {
    /// A tailer at the start of a file it has never seen.
    pub fn new() -> Tailer {
        Tailer::default()
    }

    /// The bytes read so far.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The file's size at the last read that found it unchanged or grown.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Its modification time then.
    pub fn mtime(&self) -> Option<SystemTime> {
        self.mtime
    }

    /// The `(dev, inode)` of the file read, once read (always `None` off Unix).
    pub fn identity(&self) -> Option<(u64, u64)> {
        self.identity
    }

    /// Reads what was appended to `path` since the last read.
    pub fn read(&mut self, path: &Path) -> Tail {
        self.read_at_most(path, u64::MAX)
    }

    /// [`Tailer::read`], reading at most `limit` bytes (at least one): a
    /// large file is read in bounded steps, each returning its complete
    /// lines; the next call continues where this one stopped.
    pub fn read_at_most(&mut self, path: &Path, limit: u64) -> Tail {
        let Ok(meta) = fs::metadata(path) else {
            return Tail::Missing;
        };
        let identity = file_identity(&meta);
        let replaced = self.identity.is_some() && identity != self.identity;
        if meta.len() < self.offset || replaced {
            self.offset = 0;
            self.overflowed = false;
            self.identity = identity;
            self.partial.clear();
            return Tail::Reset;
        }
        self.mtime = meta.modified().ok();
        self.size = meta.len();
        self.identity = identity;
        if meta.len() == self.offset {
            return Tail::NoChange;
        }
        let mut buf = Vec::new();
        let read = File::open(path).and_then(|mut f| {
            f.seek(SeekFrom::Start(self.offset))?;
            f.take((meta.len() - self.offset).min(limit.max(1)))
                .read_to_end(&mut buf)
        });
        if read.is_err() {
            return Tail::Missing;
        }
        self.offset += buf.len() as u64;
        Tail::Lines(self.consume(&buf))
    }

    /// Splits appended bytes into complete lines and buffers the trailing
    /// partial line (pure over the byte stream).
    pub fn consume(&mut self, appended: &[u8]) -> Vec<Vec<u8>> {
        let mut lines = Vec::new();
        let mut start = 0;
        while let Some(rel) = memchr::memchr(b'\n', &appended[start..]) {
            let i = start + rel;
            if self.overflowed {
                self.overflowed = false;
                self.partial.clear();
            } else if !self.partial.is_empty() {
                self.partial.extend_from_slice(&appended[start..i]);
                push_line(&mut lines, &self.partial);
                self.partial.clear();
            } else {
                push_line(&mut lines, &appended[start..i]);
            }
            start = i + 1;
        }
        if !self.overflowed && start < appended.len() {
            self.partial.extend_from_slice(&appended[start..]);
            if self.partial.len() > MAX_PARTIAL {
                self.partial = Vec::new();
                self.overflowed = true;
            }
        }
        lines
    }
}

fn push_line(lines: &mut Vec<Vec<u8>>, raw: &[u8]) {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    if !super::wire::blank(raw) {
        lines.push(raw.to_vec());
    }
}

#[cfg(unix)]
fn file_identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_identity(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

// -- the tail summary -------------------------------------------------------------------------

/// The tail window [`summarize_tail`] starts with.
pub const TAIL_BYTES: u64 = 64 * 1024;
/// The largest tail window: it doubles up to this when the last records are
/// huge.
pub const TAIL_MAX: u64 = 8 * 1024 * 1024;
/// The first line is read for its time only up to this many bytes.
pub const HEAD_MAX: u64 = 256 * 1024;

/// Activity, calls in flight, last text, model and times from the end of a
/// transcript (and its first line): panorama's `summarize_tail`. The window
/// starts at [`TAIL_BYTES`] and doubles, up to [`TAIL_MAX`], until it holds at
/// least two `user`/`assistant` records and its first newline is in its first
/// half, so one huge last record cannot hide them. A file that cannot be read
/// gives an empty summary.
pub fn summarize_tail(path: &Path, cwd: Option<&str>) -> TailInfo {
    summarize_tail_from(path, cwd, TAIL_BYTES)
}

/// [`summarize_tail`] starting from another window size (at least one byte).
pub fn summarize_tail_from(path: &Path, cwd: Option<&str>, window: u64) -> TailInfo {
    match read_tail(path, window.max(1)) {
        Ok((head, data, start)) => {
            let mut lines: Vec<&[u8]> = data.split(|&c| c == b'\n').collect();
            if start > 0 {
                lines.remove(0);
            }
            TailInfo::from_lines(head.as_deref(), &lines, cwd)
        }
        Err(_) => TailInfo::default(),
    }
}

fn count(data: &[u8], needle: &[u8]) -> usize {
    memchr::memmem::find_iter(data, needle).count()
}

type TailRead = (Option<Vec<u8>>, Vec<u8>, u64);

fn read_tail(path: &Path, mut window: u64) -> std::io::Result<TailRead> {
    let size = fs::metadata(path)?.len();
    let mut file = File::open(path)?;
    let mut head = Vec::new();
    BufReader::new((&mut file).take(HEAD_MAX)).read_until(b'\n', &mut head)?;
    let head = head.ends_with(b"\n").then_some(head);
    loop {
        let start = size.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let dated = count(&data, br#""type":"assistant""#) + count(&data, br#""type":"user""#);
        let first_newline = memchr::memchr(b'\n', &data).map_or(-1, |i| i as i64);
        if start == 0
            || window >= TAIL_MAX
            || (dated >= 2 && first_newline < (data.len() / 2) as i64)
        {
            return Ok((head, data, start));
        }
        window *= 2;
    }
}

// -- continuation on disk ---------------------------------------------------------------------

/// The most a [`Follower`] reads in one step.
pub const FOLLOW_CHUNK: u64 = 8 * 1024 * 1024;

/// The continuation state of one transcript file, read incrementally: the
/// inbox's `transcript.continued_in` and the continuation part of panorama's
/// `Session.update`.
///
/// Only a session's **main** transcript can continue: a follower of a file
/// inside a `subagents/` directory (`<session>/subagents/agent-*.jsonl`,
/// `<session>/subagents/workflows/<run>/agent-*.jsonl`) never reports one.
/// The whole file is read once, in steps of at most [`FOLLOW_CHUNK`] bytes
/// (so a large transcript never sits in memory whole), then only what is
/// appended; a rewritten file is read again from the start.
#[derive(Clone, Debug)]
pub struct Follower {
    path: PathBuf,
    tailer: Tailer,
    state: Option<Continuation>,
}

impl Follower {
    /// A follower of the transcript `path` of session `own`.
    pub fn new(path: impl Into<PathBuf>, own: SessionId) -> Follower {
        let path = path.into();
        let state = (!in_subagents_dir(&path)).then(|| Continuation::new(own));
        Follower {
            path,
            tailer: Tailer::new(),
            state,
        }
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads what was appended. `false` when the file is missing (the state
    /// read so far is kept).
    pub fn update(&mut self) -> bool {
        let mut resets = 0;
        loop {
            match self.tailer.read_at_most(&self.path, FOLLOW_CHUNK) {
                Tail::Missing => return false,
                Tail::NoChange => return true,
                Tail::Reset => {
                    if let Some(state) = &mut self.state {
                        state.reset();
                    }
                    resets += 1;
                    if resets > 2 {
                        return true; // replaced again and again: read on the next update
                    }
                }
                Tail::Lines(lines) => {
                    if let Some(state) = &mut self.state {
                        state.feed(&lines);
                    }
                    if self.tailer.offset() >= self.tailer.size() {
                        return true;
                    }
                }
            }
        }
    }

    /// The session the conversation continued in, as far as read.
    pub fn continued_in(&self) -> Option<&SessionId> {
        self.state.as_ref().and_then(Continuation::next)
    }
}

/// `subagents` is one of the three directories above the file (the inbox's
/// rule: folders above the session never count).
fn in_subagents_dir(path: &Path) -> bool {
    path.parent()
        .map(|dir| {
            dir.components()
                .rev()
                .take(3)
                .any(|c| c.as_os_str() == "subagents")
        })
        .unwrap_or(false)
}

/// Followers kept by one [`Transcripts`] (least recently used dropped first).
pub const FOLLOWERS_MAX: usize = 256;

/// Main transcripts under one projects root, followed incrementally:
/// [`TranscriptFiles`] for [`super::chain`].
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_line_is_buffered_until_its_newline() {
        let mut t = Tailer::new();
        assert_eq!(
            t.consume(b"{\"a\":1}\n{\"b\":"),
            vec![b"{\"a\":1}".to_vec()]
        );
        assert_eq!(t.consume(b"2}\r\n\n  \n"), vec![b"{\"b\":2}".to_vec()]);
    }

    #[test]
    fn runaway_line_is_dropped_and_reading_resyncs() {
        let mut t = Tailer::new();
        let big = vec![b'x'; MAX_PARTIAL + 1];
        assert!(t.consume(&big).is_empty());
        assert!(t.consume(b"xx").is_empty());
        assert_eq!(t.consume(b"yyy\nok\n"), vec![b"ok".to_vec()]);
    }

    #[test]
    fn a_partial_line_at_the_cap_is_kept() {
        let mut t = Tailer::new();
        assert!(t.consume(&vec![b'x'; MAX_PARTIAL]).is_empty());
        assert_eq!(t.consume(b"\n"), vec![vec![b'x'; MAX_PARTIAL]]);
    }

    #[test]
    fn projects_root_resolution() {
        let home = Some(PathBuf::from("/home/u"));
        assert_eq!(
            ProjectsRoot::resolve(None, home.clone()).unwrap().path(),
            Path::new("/home/u/.claude/projects")
        );
        assert_eq!(
            ProjectsRoot::resolve(Some("".into()), home.clone())
                .unwrap()
                .path(),
            Path::new("/home/u/.claude/projects")
        );
        assert_eq!(
            ProjectsRoot::resolve(Some("/cfg".into()), home)
                .unwrap()
                .path(),
            Path::new("/cfg/projects")
        );
        assert_eq!(
            ProjectsRoot::resolve(Some("/cfg".into()), None)
                .unwrap()
                .path(),
            Path::new("/cfg/projects")
        );
        assert_eq!(ProjectsRoot::resolve(None, None), None);
    }

    #[test]
    fn sanitized_project_names() {
        assert_eq!(sanitize_cwd("/home/user/project"), "-home-user-project");
        assert_eq!(sanitize_cwd("/a b/é_x.y"), "-a-b---x-y");
    }

    #[test]
    fn subagent_dirs_are_recognized() {
        assert!(in_subagents_dir(Path::new(
            "/r/p/s/subagents/agent-a.jsonl"
        )));
        assert!(in_subagents_dir(Path::new(
            "/r/p/s/subagents/workflows/w1/agent-a.jsonl"
        )));
        assert!(!in_subagents_dir(Path::new("/subagents/w/x/p/s.jsonl")));
        assert!(!in_subagents_dir(Path::new("/r/p/s.jsonl")));
    }
}
