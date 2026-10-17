//! Is a process alive: `kill(pid, 0)`, the pid namespace and the start token.

use std::fs;

use super::PROC;
use super::table::start_token;
use crate::agent_procs::{Pid, ProcessIdentity};

/// This process's pid namespace (`readlink /proc/self/ns/pid`, such as
/// `pid:[4026531836]`), or `None` where it cannot be read (macOS).
pub fn own_pidns() -> Option<String> {
    fs::read_link(format!("{PROC}/self/ns/pid"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// What `kill(pid, 0)` says.
enum Signal0 {
    Delivered,
    NoSuchProcess,
    NotPermitted,
    Failed,
}

fn signal0(pid: Pid) -> Signal0 {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return Signal0::Failed;
    };
    // SAFETY: signal 0 sends nothing; kill only checks that `pid` exists and
    // may be signalled.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Signal0::Delivered;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Signal0::NoSuchProcess,
        Some(libc::EPERM) => Signal0::NotPermitted,
        _ => Signal0::Failed,
    }
}

/// Is the recorded process still running (panorama `hooks.process_alive`)?
///
/// - `None` when it was recorded in another pid namespace than ours, when the
///   check fails, or when its start token cannot be read.
/// - `Some(false)` when no process has its pid, or one does with another
///   start token (the pid was reused).
/// - `Some(true)` when its pid exists with its start token, even if we may not
///   signal it.
pub fn process_alive(id: &ProcessIdentity) -> Option<bool> {
    if let Some(ns) = &id.pidns
        && own_pidns().as_deref() != Some(ns.as_str())
    {
        return None;
    }
    match signal0(id.pid) {
        Signal0::NoSuchProcess => return Some(false),
        Signal0::Failed => return None,
        Signal0::Delivered | Signal0::NotPermitted => {}
    }
    Some(start_token(id.pid)? == id.start)
}

/// Is `pid` alive, by `kill(pid, 0)` alone (inbox `proc.alive`)? An unknown
/// pid (`None` or 0) and a process we may not signal count as alive: the
/// inbox's other bounds (supersede, max age) still apply to them.
pub fn pid_alive(pid: Option<Pid>) -> bool {
    match pid {
        None | Some(0) => true,
        Some(pid) => !matches!(signal0(pid), Signal0::NoSuchProcess),
    }
}

#[cfg(test)]
mod tests;
