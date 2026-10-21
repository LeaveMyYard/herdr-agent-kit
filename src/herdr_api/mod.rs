//! The only code that talks to herdr. **IO.**
//!
//! - **Socket** for every method herdr's socket API has: one [`calls::Call`]
//!   per method the plugins use, sent by [`Herdr`] (async, tokio) or
//!   [`blocking::Herdr`] (std: the hooks and the rewake watcher run no
//!   runtime). Both have one typed method per call, of the same name.
//! - **Events**: [`Herdr::events`], a reconnecting `events.subscribe` stream
//!   with [`HerdrEvent::Connected`] and [`HerdrEvent::Lost`] markers.
//! - **CLI** (`$HERDR_BIN_PATH`) only for what the socket lacks:
//!   `herdr machine list --json`, `herdr --machine <id> api snapshot` (and
//!   `agent focus`) and `herdr plugin config-dir <id>`, with today's timeouts
//!   and the same [`HerdrError`] mapping.
//! - [`clean`] and [`Sanitized`]: every prompt and label is built through
//!   `clean()`, and every string param passes it again on the way out.
//! - `fake` (feature `test-support`): `FakeHerdr`, a scripted herdr socket seeded
//!   from the recordings in `fixtures/herdr` that records every request, and
//!   `CliStub` for the CLI path.
//!
//! Errors: [`HerdrError`] documents how it maps the Python clients' errors.
//!
//! Minimum herdr version: 0.9.4.

pub mod blocking;
pub mod calls;
mod cli;
mod client;
mod error;
mod events;
#[cfg(feature = "test-support")]
pub mod fake;
mod ids;
pub mod observe;
mod sanitize;
mod target;
mod transport;
mod typed_methods;
mod types;
mod wire;

use std::time::Duration;

pub use calls::{Call, Direction, MoveTo, OpenPluginPane, Placement};
pub use cli::MACHINE_SNAPSHOT_TIMEOUT;
pub use client::Herdr;
pub use error::{HerdrError, SERVER_NOT_RUNNING};
pub use events::{Event, EventKind, Events, FIRST_BACKOFF, HerdrEvent, MAX_BACKOFF};
pub use ids::{PaneId, ServerId, TabId, WorkspaceId};
pub use observe::{CallTrace, Transport};
pub use sanitize::{MAX_ARG, Sanitized, clean};
pub use types::{
    AgentInfo, AgentSession, AgentStatus, Extra, ForegroundProc, LayoutInfo, Machine, MoveResult,
    NotificationResult, PaneInfo, PluginInfo, ProcessInfo, Snapshot, SwapResult, TabInfo,
    WorkspaceInfo,
};

/// The default timeout of a socket request or a CLI call (Python `TIMEOUT`).
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// How often `start_agent` asks whether a new agent is ready (herdr's CLI
/// asked about every 0.2 s in the recording).
pub const START_POLL: Duration = Duration::from_millis(200);

/// The client name in request ids when none is given (`<client>:1`,
/// `<client>:events`).
pub const DEFAULT_CLIENT: &str = "herdr-agent-kit";
