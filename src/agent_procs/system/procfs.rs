//! procfs: `/proc/<pid>/stat` and `cmdline`, and the process tree below a pid
//! (`/proc/<pid>/task/*/children`).

use std::collections::HashSet;
use std::fs;

use super::PROC;
use crate::agent_procs::{Pid, ProcStat};

/// A runaway process tree never makes one liveness pass slow.
const MAX_PROCS: usize = 4096;

/// `/proc/<pid>/stat`: the command name sits between the first `(` and the
/// last `)` (it may contain both); after it come state, ppid, …, and the 22nd
/// field, starttime (the start token). Not UTF-8 is unreadable, as in the
/// Python.
pub(super) fn parse_proc_stat(raw: &[u8]) -> Option<(ProcStat, String)> {
    let text = std::str::from_utf8(raw).ok()?;
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_whitespace().collect();
    let stat = ProcStat {
        ppid: fields.get(1)?.parse().ok()?,
        comm: comm.to_owned(),
    };
    Some((stat, (*fields.get(19)?).to_owned()))
}

/// The argv of `pid` from `<proc>/<pid>/cmdline` (empty arguments dropped,
/// invalid UTF-8 replaced). `Err(NotFound)` when there is no such file.
pub(super) fn proc_cmdline(proc: &str, pid: Pid) -> std::io::Result<Vec<String>> {
    let raw = fs::read(format!("{proc}/{pid}/cmdline"))?;
    Ok(raw
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect())
}

fn children(pid: Pid) -> Vec<Pid> {
    let mut out = Vec::new();
    let Ok(tids) = fs::read_dir(format!("{PROC}/{pid}/task")) else {
        return out;
    };
    for tid in tids.flatten() {
        let path = tid.path().join("children");
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        // one bad word drops that thread's list, as in the Python
        if let Ok(kids) = text
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<Pid>, _>>()
        {
            out.extend(kids);
        }
    }
    out
}

/// Every process below `pid`, ascending, at most 4096 of them (panorama
/// `bgproc.descendants`); `None` when `pid` cannot be read (gone, or no
/// procfs).
pub fn descendants(pid: Pid) -> Option<Vec<Pid>> {
    if !fs::metadata(format!("{PROC}/{pid}/task")).is_ok_and(|m| m.is_dir()) {
        return None;
    }
    Some(below(pid, children))
}

/// Every pid below `pid` by `children`, ascending, at most [`MAX_PROCS`].
fn below(pid: Pid, children: impl Fn(Pid) -> Vec<Pid>) -> Vec<Pid> {
    let mut seen = HashSet::new();
    let mut stack = children(pid);
    while seen.len() < MAX_PROCS {
        let Some(p) = stack.pop() else { break };
        if seen.insert(p) {
            stack.extend(children(p));
        }
    }
    let mut out: Vec<Pid> = seen.into_iter().collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests;
