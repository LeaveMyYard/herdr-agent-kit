//! This machine's process table: procfs where there is one, `ps` elsewhere
//! (macOS). Read-only.
//!
//! - `table`: [`SystemProcesses`], the process table a walk reads, and start
//!   tokens.
//! - `procfs`: `/proc/<pid>/stat` and `cmdline`, and the tree below a pid.
//! - `ps`: the `ps` reads where there is no procfs.
//! - `alive`: is a recorded process (or a bare pid) still running.

mod alive;
mod procfs;
mod ps;
mod table;

pub use alive::{own_pidns, pid_alive, process_alive};
pub use procfs::descendants;
pub use table::{SystemProcesses, parent_pid, start_token};

pub(super) const PROC: &str = "/proc";
