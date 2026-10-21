//! [`Herdr`], the async client: the typed calls, the event stream,
//! `start_agent` and the CLI path.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::calls::{self, Call, MoveTo, OpenPluginPane};
use super::cli;
use super::error::HerdrError;
use super::events::{EventKind, Events};
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
use super::{START_POLL, blocking};

#[cfg(test)]
mod tests;

macro_rules! async_methods {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        impl Herdr {
            $(
                #[doc = concat!("[`calls::", stringify!($name), "`], sent.")]
                pub async fn $name(&self, $($arg: $ty),*) -> Result<$ret, HerdrError> {
                    self.send(calls::$name($($arg),*)).await
                }
            )*
        }
    };
}

/// The async herdr client: one Unix socket connection per request, as
/// herdr's own CLI does. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Herdr {
    target: Target,
}

impl Herdr {
    client_config!();

    fn from_target(target: Target) -> Self {
        Self { target }
    }

    /// The blocking client of the same server, binary, timeout and name.
    pub fn blocking(&self) -> blocking::Herdr {
        blocking::Herdr::from_target(self.target.clone())
    }

    /// Send one call and read its reply.
    pub async fn send<T>(&self, call: Call<T>) -> Result<T, HerdrError> {
        let line = self.target.line(&call);
        let timeout = self.target.timeout_of(&call);
        let timing = self.target.watch.start();
        let reply = transport::exchange(self.target.server.socket(), &line, timeout).await;
        let w = &self.target.watch;
        w.report(timing, Transport::Socket, || call.method().to_owned());
        call.parse(reply)
    }

    /// Any method, params as given (not cleaned), the raw `result`.
    pub async fn raw(&self, method: &'static str, params: Value) -> Result<Value, HerdrError> {
        self.send(calls::raw_call(method, params)).await
    }

    /// herdr's event stream for these subscription types, reconnecting.
    pub fn events(&self, kinds: &[EventKind]) -> Events {
        Events::new(
            self.target.server.socket().to_path_buf(),
            &self.target.client,
            kinds,
        )
    }

    /// Start an agent of `kind` named `name` in `pane` and wait, up to
    /// `timeout_ms`, until it is ready for a prompt, as `herdr agent start
    /// --timeout` does (recording `agent.jsonl:5`): `agent.start`, then
    /// `agent.get <name>` every [`START_POLL`] until the agent is ready
    /// ([`AgentInfo::is_ready`]). panorama types the first prompt right
    /// after this returns. Not ready in time: [`HerdrError::NoReply`] (the
    /// agent may still come up).
    pub async fn start_agent(
        &self,
        name: &Sanitized,
        kind: &str,
        pane: &PaneId,
        timeout_ms: u32,
    ) -> Result<AgentInfo, HerdrError> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        let mut agent = self
            .send(calls::start_agent(name, kind, pane, timeout_ms))
            .await?;
        while !agent.is_ready() {
            if tokio::time::Instant::now() + START_POLL > deadline {
                return Err(calls::not_ready(timeout_ms));
            }
            tokio::time::sleep(START_POLL).await;
            if let Some(polled) = self.agent_named(name.as_str()).await? {
                agent = polled;
            }
        }
        Ok(agent)
    }

    // -- the CLI path ----------------------------------------------------------

    /// `herdr machine list --json`: every saved machine, unfiltered.
    pub async fn machine_list(&self) -> Result<Vec<Machine>, HerdrError> {
        cli::parse_machines(&self.cli(&cli::machine_list()).await?)
    }

    /// `herdr --machine <id> api snapshot` (timeout
    /// [`MACHINE_SNAPSHOT_TIMEOUT`](super::MACHINE_SNAPSHOT_TIMEOUT)).
    pub async fn machine_snapshot(&self, machine: &str) -> Result<Snapshot, HerdrError> {
        cli::parse_snapshot(&self.cli(&cli::machine_snapshot(machine)).await?)
    }

    /// `herdr --machine <id> agent focus <pane>`.
    pub async fn machine_focus_agent(
        &self,
        machine: &str,
        pane: &PaneId,
    ) -> Result<(), HerdrError> {
        self.cli(&cli::machine_focus_agent(machine, pane))
            .await
            .map(drop)
    }

    /// `herdr plugin config-dir <id>`: another plugin's config dir (a plugin's
    /// own is `$HERDR_PLUGIN_CONFIG_DIR`).
    pub async fn plugin_config_dir(&self, plugin_id: &str) -> Result<Option<PathBuf>, HerdrError> {
        Ok(cli::parse_config_dir(
            &self.cli(&cli::plugin_config_dir(plugin_id)).await?,
        ))
    }

    async fn cli(&self, inv: &cli::Invocation) -> Result<String, HerdrError> {
        let timing = self.target.watch.start();
        let out = cli::run(&self.target.bin, inv, self.target.timeout).await;
        let w = &self.target.watch;
        w.report(timing, Transport::Cli, || observe::cli_op(&inv.args));
        out
    }
}

typed_methods!(async_methods);
