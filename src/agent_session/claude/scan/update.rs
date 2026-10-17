//! [`Scanner::update`]: one incremental pass over the session's files, in
//! three phases (discover, read, rebuild when a file was rewritten).

use super::Scanner;
use crate::agent_session::Ts;

/// One incremental pass over the session's files.
impl Scanner {
    /// Reads what changed. `since`: agent files last written before it and
    /// never read are skipped (they cannot change anything inside a done
    /// window that starts later; the main transcript is always read in full),
    /// unless the agent was never seen to end and its file holds a
    /// background launch. Skipped files are read once they change.
    pub fn update(&mut self, since: Option<Ts>) {
        self.discover();
        if self.read_changed(since) {
            self.rebuild(since);
        }
    }

    /// Reads what was appended to every file, journal and the hook records;
    /// `true` when a file was rewritten.
    fn read_changed(&mut self, since: Option<Ts>) -> bool {
        let mut reset = false;
        let mut skipped = Vec::new();
        for i in 0..self.files.len() {
            match self.outside_window(i, since) {
                Some(true) => skipped.push(i),
                Some(false) => reset |= self.read_file(i),
                None => {}
            }
        }
        for i in skipped {
            if self.still_waiting(i) {
                reset |= self.read_file(i);
            }
        }
        for j in 0..self.journals.len() {
            reset |= self.read_journal(j);
        }
        self.read_hooks();
        reset
    }
}
