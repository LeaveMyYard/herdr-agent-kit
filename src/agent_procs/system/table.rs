//! [`SystemProcesses`]: this machine's process table for one walk, and the
//! start token of a pid.

use std::cell::OnceCell;
use std::fs;
use std::io::ErrorKind;

use super::PROC;
use super::procfs::{parse_proc_stat, proc_cmdline};
use super::ps::{PsRunner, PsTable, parse_ps_stat, run_ps};
use crate::agent_procs::{Pid, ProcStat, ProcessTable};

/// The pid of this process's parent (a hook's parent is the shell or Claude
/// that ran it): where the walks start.
pub fn parent_pid() -> Pid {
    std::os::unix::process::parent_id()
}

/// The start token of `pid` (panorama `hooks._stat`'s third value): from
/// `/proc/<pid>/stat`, or, where that does not exist (no procfs), from
/// `ps -o ppid=,lstart=,comm= -p <pid>`. `None` if unknown. Panorama's hooks
/// write this token into their records, so its format is a wire format.
pub fn start_token(pid: Pid) -> Option<String> {
    SystemProcesses::new().start(pid)
}

/// This machine's process table, for one walk up the tree. Reads procfs;
/// where there is none, it reads the whole table with `ps` once, on first use,
/// and keeps it for the life of the value: make a new one per walk (the
/// `*_here` functions do), so no walk sees an old table.
#[derive(Debug)]
pub struct SystemProcesses {
    proc: &'static str,
    run: PsRunner,
    ps: OnceCell<PsTable>,
}

impl SystemProcesses {
    /// A table for one walk.
    pub fn new() -> Self {
        Self {
            proc: PROC,
            run: run_ps,
            ps: OnceCell::new(),
        }
    }

    fn ps(&self) -> &PsTable {
        self.ps.get_or_init(|| PsTable::read(self.run))
    }

    fn proc_stat(&self, pid: Pid) -> std::io::Result<Option<(ProcStat, String)>> {
        Ok(parse_proc_stat(&fs::read(format!(
            "{}/{pid}/stat",
            self.proc
        ))?))
    }
}

impl Default for SystemProcesses {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable for SystemProcesses {
    fn stat(&self, pid: Pid) -> Option<ProcStat> {
        match self.proc_stat(pid) {
            Ok(st) => st.map(|(st, _)| st),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                self.ps().0.get(&pid).map(|(st, _)| st.clone())
            }
            Err(_) => None,
        }
    }

    fn cmdline(&self, pid: Pid) -> Vec<String> {
        match proc_cmdline(self.proc, pid) {
            Ok(argv) => argv,
            Err(e) if e.kind() == ErrorKind::NotFound => self
                .ps()
                .0
                .get(&pid)
                .map(|(_, argv)| argv.clone())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// procfs, or the one-pid `ps` read (panorama `_stat`), never the walk's
    /// table: the token must be byte-identical to what the Python hooks wrote.
    fn start(&self, pid: Pid) -> Option<String> {
        match self.proc_stat(pid) {
            Ok(st) => st.map(|(_, start)| start),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let pid = pid.to_string();
                let out = (self.run)(&["-o", "ppid=,lstart=,comm=", "-p", &pid])?;
                parse_ps_stat(&out).map(|(_, start)| start)
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
