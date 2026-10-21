//! Watching the calls a client makes: an observer set with `with_observer` hears one
//! [`CallTrace`] per socket request and per CLI run once it is over: answered, failed, timed
//! out, or dropped unfinished (an async call whose future was cancelled). Event streams are
//! not calls.
//!
//! The observer runs on the thread that ends the call, outside any lock the client holds, and
//! the caller waits for it: keep it quick. A panic in it is caught and never reaches the call;
//! the process's panic hook still runs first (it prints the panic as usual), and under
//! `panic = "abort"` a panic aborts the process as any panic does.
//!
//! [`CallTrace::op`] is the bare method or arguments (`session.snapshot`, `api snapshot`); a
//! log that wants panorama's `PANORAMA_TRACE` lines adds the `socket ` or `cli ` prefix
//! from [`CallTrace::transport`], and takes its time stamp as `started + took` (the end).
//!
//! ```
//! use herdr_agent_kit::herdr_api::{CallTrace, Herdr, ServerId};
//! use std::sync::{Arc, Mutex};
//!
//! let seen = Arc::new(Mutex::new(Vec::new()));
//! let log = seen.clone();
//! let herdr = Herdr::new(ServerId::new("/nowhere/herdr.sock"))
//!     .with_observer(move |t: &CallTrace| log.lock().unwrap().push((t.op.clone(), t.took)));
//! # drop(herdr);
//! ```

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// How a call reached herdr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Transport {
    /// A request over the server's socket.
    Socket,
    /// A run of the `herdr` binary.
    Cli,
}

/// One call, as an observer hears it once it is over.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallTrace {
    /// The socket or the CLI.
    pub transport: Transport,
    /// What was called: the socket method (`session.snapshot`), or the CLI's first two
    /// arguments joined by a space (`api snapshot`, `--machine m1`). No prefix.
    pub op: String,
    /// When it started (the wall clock).
    pub started: SystemTime,
    /// How long it took, until it was answered, failed, timed out or was dropped.
    pub took: Duration,
}

/// A client's observer.
pub(crate) type Observer = Arc<dyn Fn(&CallTrace) + Send + Sync>;

/// A client's observer, if any.
#[derive(Clone, Default)]
pub(crate) struct Watch(pub(crate) Option<Observer>);

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Watch(observer)"
        } else {
            "Watch(none)"
        })
    }
}

/// A call under way: the observer hears it when this is dropped (the call ended, or its
/// future was dropped unfinished).
pub(crate) struct Pending {
    observer: Observer,
    transport: Transport,
    op: String,
    started: SystemTime,
    at: Instant,
}

impl Watch {
    /// A call starts (nothing, not even its op, is made when nobody watches).
    pub(crate) fn start(
        &self,
        transport: Transport,
        op: impl FnOnce() -> String,
    ) -> Option<Pending> {
        let observer = self.0.clone()?;
        Some(Pending {
            observer,
            transport,
            op: op(),
            started: SystemTime::now(),
            at: Instant::now(),
        })
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        let trace = CallTrace {
            transport: self.transport,
            op: std::mem::take(&mut self.op),
            started: self.started,
            took: self.at.elapsed(),
        };
        let observer = &self.observer;
        let _ = catch_unwind(AssertUnwindSafe(|| observer(&trace)));
    }
}

/// The CLI's op: its first two arguments.
pub(crate) fn cli_op(args: &[String]) -> String {
    args.iter()
        .take(2)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
