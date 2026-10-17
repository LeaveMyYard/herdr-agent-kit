//! The reader's inputs besides transcript records: agent files and metas,
//! a transcript's facts, workflow journals and summaries, hook records.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::{FileFacts, Owner, Session};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Line;

/// Inputs other than transcript records.
impl Session {
    /// Records an agent file of the session (its transcript, and the workflow
    /// run it is under), keeping a meta already read.
    pub fn set_agent(&mut self, id: &str, transcript: PathBuf, workflow: Option<String>) {
        self.children.take();
        let rec = self.agents.entry(id.to_string()).or_default();
        rec.transcript = transcript;
        rec.workflow = workflow;
    }

    /// Records an agent's `meta.json` object.
    pub fn set_meta(&mut self, id: &str, meta: Map<String, Value>) {
        self.children.take();
        self.agents.entry(id.to_string()).or_default().meta = meta;
    }

    /// The facts of `owner`'s transcript, for the reader to update.
    pub fn file_mut(&mut self, owner: Owner) -> &mut FileFacts {
        self.files.entry(owner).or_default()
    }

    /// Folds one line of workflow run `run`'s `journal.jsonl` (`None`: a line
    /// that did not parse).
    pub fn apply_journal(&mut self, run: &str, rec: Option<&Line>) {
        self.workflows.apply_journal(run, rec);
    }

    /// Records workflow run `run`'s summary (`workflows/<run>.json`) and the
    /// file's modification time.
    pub fn set_summary(&mut self, run: &str, summary: Map<String, Value>, mtime: Ts) {
        self.workflows.set_summary(run, summary, mtime);
    }

    /// Forgets the hook records (the hook file was rewritten).
    pub fn reset_hooks(&mut self) {
        self.hooks.clear();
    }

    /// Folds one hook record (the kit's
    /// [hook-record format](super#hook-records-format-v1), version
    /// [`HOOK_VERSION`](super::HOOK_VERSION)). A record of another version,
    /// without an `agent` string or without a numeric `ts` is skipped.
    pub fn apply_hook(&mut self, rec: &Line) {
        self.hooks.apply(rec);
    }
}
