//! Watching the calls a client makes: an [`Observer`] hears one [`CallTrace`] per socket
//! request and per CLI run, after it ended, failed or not. Event streams are not calls.
//!
//! ```
//! use herdr_agent_kit::herdr_api::{CallTrace, Herdr, ServerId};
//! use std::sync::{Arc, Mutex};
//!
//! let seen = Arc::new(Mutex::new(Vec::new()));
//! let log = seen.clone();
//! let herdr = Herdr::new(ServerId::new("/nowhere/herdr.sock")).with_observer(Arc::new(
//!     move |t: &CallTrace| log.lock().unwrap().push((t.op.clone(), t.took)),
//! ));
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
    /// arguments joined by a space (`api snapshot`, `--machine m1`).
    pub op: String,
    /// When it started (the wall clock).
    pub started: SystemTime,
    /// How long it took, reply or failure included.
    pub took: Duration,
}

/// A function that hears every call a client makes. It runs on the calling thread after the
/// call, outside any lock the client holds; a panic in it is caught and dropped, never the
/// call's. It should be quick: the caller waits for it.
pub type Observer = Arc<dyn Fn(&CallTrace) + Send + Sync>;

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

/// A call under way.
pub(crate) struct Timing {
    started: SystemTime,
    at: Instant,
}

impl Watch {
    /// Starts timing a call (cheap when nobody watches).
    pub(crate) fn start(&self) -> Option<Timing> {
        self.0.as_ref().map(|_| Timing {
            started: SystemTime::now(),
            at: Instant::now(),
        })
    }

    /// The call is over: the observer hears it.
    pub(crate) fn report(
        &self,
        timing: Option<Timing>,
        transport: Transport,
        op: impl FnOnce() -> String,
    ) {
        let (Some(observer), Some(t)) = (&self.0, timing) else {
            return;
        };
        let trace = CallTrace {
            transport,
            op: op(),
            started: t.started,
            took: t.at.elapsed(),
        };
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
