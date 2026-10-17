//! Reading one session's files into the [`fold`](super::fold). **IO.**
//!
//! panorama's `Session.update` and its readers (`scan/session.py`): the main
//! transcript, every subagent transcript and `meta.json`, the workflow
//! journals and run summaries, and panorama's hook records, each tailed
//! incrementally ([`Tailer`]) so an update reads only what was appended.
//!
//! Performance as in panorama: a full-file pass only JSON-parses lines that
//! hold a marker ([`wire::wanted_lines`]); each agent's activity, pending
//! calls, last text and model come from the end of its file
//! ([`summarize_tail`]), re-read only when the file grew; tool calls are a
//! byte count of `"type":"tool_use"`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Map, Value};

use super::SessionId;
use super::Ts;
use super::fold::{Owner, Session};
use super::io::{Tail, Tailer, related, summarize_tail, workflow_summary_path};
use super::wire::{self, Line, contains};

/// A Bash moved to the background (its result record): an old agent file
/// holding it is read even when it is outside the done window.
pub const BG_MARK: &[u8] = br#""backgroundTaskId""#;
const TOOL_USE: &[u8] = br#""type":"tool_use""#;
const ASSISTANT: &[u8] = br#""type":"assistant""#;

/// A file's modification time in epoch seconds, `0.0` when it cannot be read
/// (panorama's `_mtime`; computed as CPython's `st_mtime`).
pub fn mtime(path: &Path) -> f64 {
    fs::metadata(path).map_or(0.0, |m| mtime_of(&m))
}

fn mtime_of(meta: &fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |d| {
            d.as_secs() as f64 + f64::from(d.subsec_nanos()) * 1e-9
        })
}

/// A JSON object file (`meta.json`, a run summary): panorama's
/// `json.load(open(path, encoding="utf-8"))`. It goes through [`Line::parse`],
/// so a lone UTF-16 surrogate escape reads as Python reads it; a leading UTF-8
/// BOM does not (Python's `utf-8` codec keeps it and `json` rejects it).
fn read_object(path: &Path) -> Option<Map<String, Value>> {
    let data = fs::read(path).ok()?;
    if data.starts_with(b"\xef\xbb\xbf") {
        return None;
    }
    Line::parse(&data).map(Line::into_map)
}

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

    /// Reads what changed. `since`: agent files last written before it and
    /// never read are skipped (they cannot change anything inside a done
    /// window that starts later; the main transcript is always read in full),
    /// unless the agent was never seen to end and its file holds a
    /// background launch. Skipped files are read once they change.
    pub fn update(&mut self, since: Option<f64>) {
        if self.files.is_empty() {
            // main is always the first file
            self.add_file(self.root.clone(), Owner::Main);
        }
        let (agent_files, journals) = related(&self.root);
        for af in agent_files {
            self.session
                .set_agent(&af.agent_id, af.transcript.clone(), af.workflow.clone());
            let unread = self
                .session
                .agents()
                .get(&af.agent_id)
                .is_none_or(|r| r.meta.is_empty());
            if unread {
                let m = mtime(&af.meta);
                if self.meta_mtime.get(&af.agent_id) != Some(&m) {
                    // an empty or half-written meta is retried when it changes
                    let meta = read_object(&af.meta).unwrap_or_default();
                    self.session.set_meta(&af.agent_id, meta);
                    self.meta_mtime.insert(af.agent_id.clone(), m);
                }
            }
            if !self.file_paths.contains(&af.transcript) {
                self.add_file(af.transcript, Owner::Agent(af.agent_id));
            }
        }
        for (run, path) in journals {
            if self.journal_runs.insert(run.clone()) {
                self.journals.push(Journal {
                    run: run.clone(),
                    path,
                    tail: Tailer::new(),
                });
            }
            self.read_summary(&run);
        }
        let mut reset = false;
        let mut skipped = Vec::new();
        for i in 0..self.files.len() {
            let f = &self.files[i];
            if let Some(since) = since
                && f.owner != Owner::Main
                && f.tail.identity().is_none()
            {
                match fs::metadata(&f.path) {
                    Ok(m) if mtime_of(&m) < since => {
                        skipped.push(i);
                        continue;
                    }
                    Ok(_) => {}
                    Err(_) => continue,
                }
            }
            reset |= self.read_file(i);
        }
        for i in skipped {
            // an old file of an agent nobody saw end (no notice, no completed acknowledgement)
            // can still be waiting on its own background work: read it after all when it holds
            // a background launch (a byte search, once per file version)
            let Owner::Agent(owner) = &self.files[i].owner else {
                continue;
            };
            let ended = !self.session.notices(owner).is_empty()
                || self
                    .session
                    .ack(owner)
                    .is_some_and(|a| a.status == "completed");
            if ended {
                continue;
            }
            let path = self.files[i].path.clone();
            if !self
                .bg_scanned
                .insert((path.clone(), mtime(&path).to_bits()))
            {
                continue;
            }
            if fs::read(&path).is_ok_and(|data| contains(&data, BG_MARK)) {
                reset |= self.read_file(i);
            }
        }
        for j in 0..self.journals.len() {
            reset |= self.read_journal(j);
        }
        self.read_hooks();
        if reset {
            // a file was rewritten: everything derived from it is suspect; rebuild
            self.session.reset();
            for f in &mut self.files {
                f.tail = Tailer::new();
                f.info_size = None;
            }
            for j in &mut self.journals {
                j.tail = Tailer::new();
            }
            self.hook_tail = Tailer::new();
            // as panorama: a file read only by the background re-read above is not read
            // again here and stays skipped (herdr-panorama#60; fixed in both together)
            for i in 0..self.files.len() {
                let f = &self.files[i];
                if since.is_none_or(|s| f.owner == Owner::Main || mtime(&f.path) >= s) {
                    self.read_file(i);
                }
            }
            for j in 0..self.journals.len() {
                self.read_journal(j);
                let run = self.journals[j].run.clone();
                self.read_summary(&run);
            }
            self.read_hooks(); // the rebuild reset the hook stores too
        }
    }

    fn add_file(&mut self, path: PathBuf, owner: Owner) {
        self.session.file_mut(owner.clone());
        self.file_paths.insert(path.clone());
        self.files.push(FileState {
            path,
            owner,
            tail: Tailer::new(),
            info_size: None,
        });
    }

    /// Reads file `i`'s appended lines into the fold; `true` when the file
    /// was rewritten (the caller rebuilds).
    fn read_file(&mut self, i: usize) -> bool {
        let f = &mut self.files[i];
        let lines = match f.tail.read(&f.path) {
            Tail::Reset => return true,
            Tail::Lines(lines) => lines,
            Tail::Missing | Tail::NoChange => Vec::new(),
        };
        let owner = f.owner.clone();
        let (path, size) = (f.path.clone(), f.tail.size());
        let facts = self.session.file_mut(owner.clone());
        if owner != Owner::Main {
            // the agent's own calls: tool_use blocks in assistant records
            facts.tool_calls += lines
                .iter()
                .filter(|raw| contains(raw, ASSISTANT))
                .map(|raw| memchr::memmem::find_iter(raw, TOOL_USE).count() as u64)
                .sum::<u64>();
        }
        for idx in wire::wanted_lines(&lines) {
            let Some(rec) = Line::parse(&lines[idx]) else {
                continue;
            };
            let facts = self.session.file_mut(owner.clone());
            // `ts or last_ts`: an undated record (or one dated 0) takes the stream time
            let when = match rec.ts() {
                Some(t) if t.secs() != 0.0 => Some(t.secs()),
                _ => facts.last_ts,
            };
            if when.is_some() {
                facts.last_ts = when;
            }
            self.session.apply(owner.clone(), &rec, when.map(Ts));
        }
        if owner == Owner::Main {
            self.session.feed_main(&lines);
        }
        let f = &mut self.files[i];
        if size > 0 && f.info_size != Some(size) {
            self.session.file_mut(owner).info = summarize_tail(&path, self.cwd.as_deref());
            f.info_size = Some(size);
        }
        false
    }

    fn read_journal(&mut self, j: usize) -> bool {
        let journal = &mut self.journals[j];
        let lines = match journal.tail.read(&journal.path) {
            Tail::Reset => return true,
            Tail::Lines(lines) => lines,
            Tail::Missing | Tail::NoChange => return false,
        };
        let run = journal.run.clone();
        for raw in lines {
            self.session.apply_journal(&run, Line::parse(&raw).as_ref());
        }
        false
    }

    fn read_summary(&mut self, run: &str) {
        let path = workflow_summary_path(&self.root, run);
        let Ok(meta) = fs::metadata(&path) else {
            return;
        };
        let m = mtime_of(&meta);
        if self.session.summary_mtime(run) == Some(m) {
            return;
        }
        if let Some(summary) = read_object(&path) {
            self.session.set_summary(run, summary, m); // mid-write: retried next update
        }
    }

    fn read_hooks(&mut self) {
        let Some(path) = &self.hooks else {
            return;
        };
        let mut read = self.hook_tail.read(path);
        if read == Tail::Reset {
            self.session.reset_hooks();
            read = self.hook_tail.read(path);
        }
        if let Tail::Lines(lines) = read {
            for raw in lines {
                if let Some(rec) = Line::parse(&raw) {
                    self.session.apply_hook(&rec);
                }
            }
        }
    }
}
