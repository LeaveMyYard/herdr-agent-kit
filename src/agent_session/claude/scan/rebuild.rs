//! The rebuild after a rewrite: a file was replaced or truncated, so
//! everything derived from it is suspect and the fold is read again from the
//! start.

use super::Scanner;
use super::disk::mtime;
use crate::agent_session::Ts;
use crate::agent_session::claude::fold::Owner;
use crate::agent_session::tail::Tailer;

impl Scanner {
    /// Resets the fold and every tail, then reads every file in the window,
    /// every journal and summary, and the hook records again.
    pub(super) fn rebuild(&mut self, since: Option<Ts>) {
        self.session.reset();
        for f in &mut self.files {
            f.tail = Tailer::new();
            f.info_size = None;
        }
        for j in &mut self.journals {
            j.tail = Tailer::new();
        }
        self.hook_tail = Tailer::new();
        // as panorama: a file read only by the background re-read of a pass
        // is not read again here and stays skipped (herdr-panorama#60; fixed
        // in both together)
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
