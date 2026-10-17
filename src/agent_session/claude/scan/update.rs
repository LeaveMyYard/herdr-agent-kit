//! [`Scanner::update`]: one incremental pass over the session's files.

use std::fs;

use super::disk::{mtime, mtime_of, read_object};
use super::{BG_MARK, Journal, Scanner};
use crate::agent_session::Ts;
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::claude::io::{Tailer, related};
use crate::agent_session::claude::wire::contains;

/// One incremental pass over the session's files.
impl Scanner {
    /// Reads what changed. `since`: agent files last written before it and
    /// never read are skipped (they cannot change anything inside a done
    /// window that starts later; the main transcript is always read in full),
    /// unless the agent was never seen to end and its file holds a
    /// background launch. Skipped files are read once they change.
    pub fn update(&mut self, since: Option<Ts>) {
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
                .agent(&af.agent_id)
                .is_none_or(|r| r.meta.is_empty());
            if unread {
                let m = mtime(&af.meta).0;
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
                    Ok(m) if mtime_of(&m) < since.0 => {
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
                .insert((path.clone(), mtime(&path).0.to_bits()))
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
}
