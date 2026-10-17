//! Claude Code's background tasks: the output files that a Claude's
//! descendants hold open.

use std::collections::BTreeSet;
use std::fs;

use super::PROC;
use super::procfs::descendants;
use crate::agent_procs::{Pid, basename};

/// The task id of a background task's output file, `…/tasks/<id>.output`
/// (panorama `bgproc.task_of`).
pub fn task_of(path: &str) -> Option<&str> {
    let (head, name) = path.rsplit_once('/')?;
    let head = match head.trim_end_matches('/') {
        "" => head,
        trimmed => trimmed,
    };
    if basename(head) != "tasks" {
        return None;
    }
    name.strip_suffix(".output").filter(|id| !id.is_empty())
}

/// The ids of the background tasks whose output file a descendant of `pid`
/// holds open (panorama `bgproc.live_tasks`): Claude Code runs a background
/// Bash as a shell below its own process, writing to
/// `<tmp>/claude-<uid>/<project>/<session>/tasks/<task id>.output`, and the
/// shell (or a child) holds that file open while it lives. `None` when unknown
/// (no procfs, `pid` unreadable): never "dead".
///
/// A shell left over from an earlier Claude process (the session was resumed)
/// is not a descendant of the current one, so it does not count.
pub fn live_tasks(pid: Pid) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for p in descendants(pid)? {
        let Ok(fds) = fs::read_dir(format!("{PROC}/{p}/fd")) else {
            continue; // exited meanwhile, or not ours to read
        };
        for fd in fds.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else {
                continue;
            };
            // lossy, as Python's surrogate escapes: the `tasks/<id>.output` tail is ASCII
            if let Some(task) = task_of(&target.to_string_lossy()) {
                out.insert(task.to_owned());
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
