//! Agent processes. **IO** (reads the process table).
//!
//! "Is this Claude Code", nested Claude detection and liveness checks, as one
//! implementation for both plugins. It provides the `Liveness` implementation
//! that [`crate::claude_session::fold`] takes as an argument.
