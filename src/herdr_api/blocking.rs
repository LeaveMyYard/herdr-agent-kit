//! The blocking client, for processes that run no async runtime: the hooks
//! and the rewake watcher (plan section 2.5). The same calls, the same
//! errors and the same timeouts as [`super::Herdr`]; no event stream.

use std::path::PathBuf;

use serde_json::Value;

use super::calls::{self, Call, MoveTo, OpenPluginPane};
use super::cli;
use super::error::HerdrError;
use super::ids::{PaneId, ServerId, TabId, WorkspaceId};
use super::sanitize::Sanitized;
use super::transport;
use super::types::{
    AgentInfo, Extra, LayoutInfo, Machine, MoveResult, NotificationResult, PaneInfo, PluginInfo,
    ProcessInfo, Snapshot, SwapResult, TabInfo, WorkspaceInfo,
};
use super::{Target, client_config, typed_methods};

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
        call.parse(transport::exchange_blocking(
            self.target.server.socket(),
            &line,
            timeout,
        ))
    }

    /// Any method, params as given (not cleaned), the raw `result`.
    pub fn raw(&self, method: &'static str, params: Value) -> Result<Value, HerdrError> {
        self.send(calls::raw_call(method, params))
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
        cli::run_blocking(&self.target.bin, inv, self.target.timeout)
    }
}

typed_methods!(blocking_methods);
