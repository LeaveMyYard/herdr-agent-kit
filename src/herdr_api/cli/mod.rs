//! The CLI path (`$HERDR_BIN_PATH`, else `herdr`), only for what herdr's
//! socket has no method for:
//!
//! - `herdr machine list --json` (panorama `machines.saved`, herdr-inbox
//!   `remote.machines`);
//! - `herdr --machine <id> api snapshot` (panorama `machines.remote_snapshot`,
//!   8 s) and `herdr --machine <id> agent focus <pane>` (panorama's remote
//!   focus);
//! - `herdr plugin config-dir <id>` for another plugin's id (herdr-inbox
//!   radar).
//!
//! Like the Python `run_raw`: every argument goes through [`clean`], the
//! default timeout is 3 s, and a failure is an error, never a panic.
//!
//! - The binary cannot be started: [`HerdrError::Unreachable`].
//! - Timed out (the process is killed): [`HerdrError::NoReply`].
//! - Exit status != 0: [`HerdrError::Refused`] with herdr's `{"error":
//!   {"code", "message"}}` from stderr, or with an empty code and the plain
//!   text (usage errors, `--machine <unknown>`: exit 2). The code
//!   `server_not_running` is [`HerdrError::NotRunning`] (herdr-inbox
//!   `server_gone`).
//!
//! [`HerdrError::Unreachable`]: super::HerdrError::Unreachable
//! [`HerdrError::NoReply`]: super::HerdrError::NoReply
//! [`HerdrError::Refused`]: super::HerdrError::Refused
//! [`HerdrError::NotRunning`]: super::HerdrError::NotRunning

mod output;
mod run;

#[cfg(test)]
mod tests;

use std::time::Duration;

use super::ids::PaneId;
use super::sanitize::{MAX_ARG, clean};

pub(crate) use output::{parse_config_dir, parse_machines, parse_snapshot};
pub(crate) use run::{run, run_blocking};

/// The timeout of `--machine <id> api snapshot` (panorama
/// `machines.SNAPSHOT_TIMEOUT`).
pub const MACHINE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);

/// One CLI invocation: its arguments (after the binary) and its timeout.
pub(crate) struct Invocation {
    pub args: Vec<String>,
    pub timeout: Option<Duration>,
}

impl Invocation {
    fn new(args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|a| clean(a, MAX_ARG)).collect(),
            timeout: None,
        }
    }
}

pub(crate) fn machine_list() -> Invocation {
    Invocation::new(&["machine", "list", "--json"])
}

pub(crate) fn machine_snapshot(machine: &str) -> Invocation {
    let mut inv = Invocation::new(&["--machine", machine, "api", "snapshot"]);
    inv.timeout = Some(MACHINE_SNAPSHOT_TIMEOUT);
    inv
}

pub(crate) fn machine_focus_agent(machine: &str, pane: &PaneId) -> Invocation {
    Invocation::new(&["--machine", machine, "agent", "focus", pane.as_str()])
}

pub(crate) fn plugin_config_dir(plugin_id: &str) -> Invocation {
    Invocation::new(&["plugin", "config-dir", plugin_id])
}
