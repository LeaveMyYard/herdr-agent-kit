//! Discovery: the session's files as they are on disk now (subagent
//! transcripts and their `meta.json`, workflow journals and run summaries).

use super::disk::{mtime, read_object};
use super::{Journal, Scanner};
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::claude::io::{Related, related};
use crate::agent_session::tail::Tailer;

impl Scanner {
    /// Adds the files that appeared since the last pass, reads each unread
    /// agent's `meta.json` when it changed, and reads the run summaries.
    pub(super) fn discover(&mut self) {
        if self.files.is_empty() {
            // main is always the first file
            self.add_file(self.root.clone(), Owner::Main);
        }
        let Related { agents, journals } = related(&self.root);
        for af in agents {
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
    }
}
