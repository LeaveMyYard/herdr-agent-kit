//! A pane's processes (`pane.process_info`).

use serde::{Deserialize, Serialize};

use super::Extra;
use super::lenient::{argv, lenient, lenient_opt_vec};
use crate::herdr_api::ids::PaneId;

/// One foreground process of a pane.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ForegroundProc {
    /// Its pid.
    #[serde(default, deserialize_with = "lenient")]
    pub pid: Option<i64>,
    /// Its process name.
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// Its argv (strings and integers, as strings).
    #[serde(default, deserialize_with = "argv")]
    pub argv: Vec<String>,
    /// Everything else (`cmdline`, `cwd`).
    #[serde(flatten)]
    pub extra: Extra,
}

/// `pane.process_info`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProcessInfo {
    /// The pane.
    #[serde(default, deserialize_with = "lenient")]
    pub pane_id: Option<PaneId>,
    /// The pane's shell.
    #[serde(default, deserialize_with = "lenient")]
    pub shell_pid: Option<i64>,
    /// The foreground processes. `None` when herdr gave no list (mid-exec or
    /// mid-restore: unknown, not "nothing").
    #[serde(default, deserialize_with = "lenient_opt_vec")]
    pub foreground_processes: Option<Vec<ForegroundProc>>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}
