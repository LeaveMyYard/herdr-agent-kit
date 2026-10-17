//! Everything about Claude Code's session files, in one implementation.
//!
//! Discovery, tailing, the transcript wire format, continuation and
//! session-id validation. Split along the purity boundary:
//!
//! - [`fold`] is **pure**: records in, session state and status out.
//! - [`io`] does the **IO**: finding the projects root and tailing files.
//!
//! Planned types: `SessionId` (the one validator: canonical UUID, any case
//! in, lowercase out), `Record` (lenient: unknown kinds become `Other`),
//! `Continuation` (the one continuation rule, D6, used by both plugins) and
//! `Session`.

pub mod fold;
pub mod io;
