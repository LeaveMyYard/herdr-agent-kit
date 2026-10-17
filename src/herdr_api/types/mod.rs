//! What herdr answers, typed. Lenient on purpose, like the Python clients
//! that read these with `.get()`: a missing or mistyped field becomes its
//! default (or `None`), a list item that is not an object is skipped, and
//! every field not named here is kept in `extra`.

mod agent;
mod lenient;
mod outcome;
mod process;
mod registry;
mod session;

#[cfg(test)]
mod tests;

use serde_json::{Map, Value};

pub use agent::{AgentInfo, AgentSession, AgentStatus};
pub use outcome::{MoveResult, NotificationResult, SwapResult};
pub use process::{ForegroundProc, ProcessInfo};
pub use registry::{Machine, PluginInfo};
pub use session::{LayoutInfo, PaneInfo, Snapshot, TabInfo, WorkspaceInfo};

/// Fields this crate does not name, as herdr sent them.
pub type Extra = Map<String, Value>;
