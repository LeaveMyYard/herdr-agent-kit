//! The blocking client, for processes that run no async runtime: the hooks
//! and the rewake watcher (plan section 2.5). The same calls, the same
//! errors and the same timeouts as [`super::Herdr`]; no event stream.

use std::path::PathBuf;

use serde_json::Value;

use super::calls::{self, Call, MoveTo, OpenPluginPane};
use super::cli;
use super::error::HerdrError;
use super::ids::{PaneId, ServerId, TabId, WorkspaceId};
use super::observe::{self, Transport};
use super::sanitize::Sanitized;
use super::target::{Target, client_config};
use super::transport;
use super::typed_methods::typed_methods;
use super::types::{
    AgentInfo, Extra, LayoutInfo, Machine, MoveResult, NotificationResult, PaneInfo, PluginInfo,
    ProcessInfo, Snapshot, SwapResult, TabInfo, WorkspaceInfo,
};

macro_rules! blocking_methods {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        impl Herdr {
            $(
                #[doc = concat!("[`calls::", stringify!($name), "`], sent.")]
                pub fn $name(&self, $($arg: $ty),*) -> Result<$ret, HerdrError> {
                    self.send(calls::$name($($arg),*))
                }
            )*
        }
    };
}

/// The blocking herdr client: one Unix socket connection per request.
#[derive(Clone, Debug)]
pub struct Herdr {
    target: Target,
}

impl Herdr {
    client_config!();

    pub(super) fn from_target(target: Target) -> Self {
        Self { target }
    }

    /// Send one call and read its reply.
    pub fn send<T>(&self, call: Call<T>) -> Result<T, HerdrError> {
        let line = self.target.line(&call);
        let timeout = self.target.timeout_of(&call);
        let timing = self.target.watch.start();
        let reply = transport::exchange_blocking(self.target.server.socket(), &line, timeout);
        let w = &self.target.watch;
        w.report(timing, Transport::Socket, || call.method().to_owned());
        call.parse(reply)
    }

    /// Any method, params as given (not cleaned), the raw `result`.
    pub fn raw(&self, method: &'static str, params: Value) -> Result<Value, HerdrError> {
        self.send(calls::raw_call(method, params))
    }

    /// [`super::Herdr::start_agent`], blocking.
    pub fn start_agent(
        &self,
        name: &Sanitized,
        kind: &str,
        pane: &PaneId,
        timeout_ms: u32,
    ) -> Result<AgentInfo, HerdrError> {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(u64::from(timeout_ms));
        let mut agent = self.send(calls::start_agent(name, kind, pane, timeout_ms))?;
        while !agent.is_ready() {
            if std::time::Instant::now() + super::START_POLL > deadline {
                return Err(calls::not_ready(timeout_ms));
            }
            std::thread::sleep(super::START_POLL);
            if let Some(polled) = self.agent_named(name.as_str())? {
                agent = polled;
            }
        }
        Ok(agent)
    }

    /// `herdr machine list --json`: every saved machine, unfiltered.
    pub fn machine_list(&self) -> Result<Vec<Machine>, HerdrError> {
        cli::parse_machines(&self.cli(&cli::machine_list())?)
    }

    /// `herdr --machine <id> api snapshot` (timeout
    /// [`MACHINE_SNAPSHOT_TIMEOUT`](super::MACHINE_SNAPSHOT_TIMEOUT)).
    pub fn machine_snapshot(&self, machine: &str) -> Result<Snapshot, HerdrError> {
        cli::parse_snapshot(&self.cli(&cli::machine_snapshot(machine))?)
    }

    /// `herdr --machine <id> agent focus <pane>`.
    pub fn machine_focus_agent(&self, machine: &str, pane: &PaneId) -> Result<(), HerdrError> {
        self.cli(&cli::machine_focus_agent(machine, pane)).map(drop)
    }

    /// `herdr plugin config-dir <id>`.
    pub fn plugin_config_dir(&self, plugin_id: &str) -> Result<Option<PathBuf>, HerdrError> {
        Ok(cli::parse_config_dir(
            &self.cli(&cli::plugin_config_dir(plugin_id))?,
        ))
    }

    fn cli(&self, inv: &cli::Invocation) -> Result<String, HerdrError> {
        let timing = self.target.watch.start();
        let out = cli::run_blocking(&self.target.bin, inv, self.target.timeout);
        let w = &self.target.watch;
        w.report(timing, Transport::Cli, || observe::cli_op(&inv.args));
        out
    }
}

typed_methods!(blocking_methods);
