//! The readers of one file each: a transcript, a workflow journal, a run
//! summary, the hook records.

use std::fs;

use super::Scanner;
use super::disk::{mtime_of, read_object};
use crate::agent_session::Ts;
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::claude::io::{Tail, summarize_tail, workflow_summary_path};
use crate::agent_session::claude::wire::{self, Line, contains};

const TOOL_USE: &[u8] = br#""type":"tool_use""#;
const ASSISTANT: &[u8] = br#""type":"assistant""#;

/// One reader per file kind.
impl Scanner {
    /// Reads file `i`'s appended lines into the fold; `true` when the file
    /// was rewritten (the caller rebuilds).
    pub(super) fn read_file(&mut self, i: usize) -> bool {
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
                Some(t) if t.secs() != 0.0 => Some(t),
                _ => facts.last_ts,
            };
            if when.is_some() {
                facts.last_ts = when;
            }
            self.session.apply(owner.clone(), &rec, when);
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

    pub(super) fn read_journal(&mut self, j: usize) -> bool {
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

    pub(super) fn read_summary(&mut self, run: &str) {
        let path = workflow_summary_path(&self.root, run);
        let Ok(meta) = fs::metadata(&path) else {
            return;
        };
        let m = Ts(mtime_of(&meta));
        if self.session.summary_mtime(run) == Some(m) {
            return;
        }
        if let Some(summary) = read_object(&path) {
            self.session.set_summary(run, summary, m); // mid-write: retried next update
        }
    }

    pub(super) fn read_hooks(&mut self) {
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
