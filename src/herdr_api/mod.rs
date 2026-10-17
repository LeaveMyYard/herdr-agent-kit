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
mod error;
mod events;
#[cfg(feature = "test-support")]
pub mod fake;
mod ids;
mod sanitize;
mod transport;
mod types;
mod wire;

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

pub use calls::{Call, Direction, MoveTo, OpenPluginPane, Placement};
pub use cli::MACHINE_SNAPSHOT_TIMEOUT;
pub use error::{HerdrError, SERVER_NOT_RUNNING};
pub use events::{Event, EventKind, Events, FIRST_BACKOFF, HerdrEvent, MAX_BACKOFF};
pub use ids::{PaneId, ServerId, TabId, WorkspaceId};
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

/// Where and how to reach one herdr server; shared by both clients.
#[derive(Clone, Debug)]
struct Target {
    server: ServerId,
    bin: PathBuf,
    timeout: Duration,
    client: String,
}

impl Target {
    fn new(server: ServerId) -> Self {
        Self {
            server,
            bin: PathBuf::from("herdr"),
            timeout: TIMEOUT,
            client: DEFAULT_CLIENT.to_owned(),
        }
    }

    fn resolve(var: impl Fn(&str) -> Option<OsString>) -> Self {
        let bin = var("HERDR_BIN_PATH").filter(|b| !b.is_empty());
        let mut target = Self::new(ServerId::resolve(&var));
        if let Some(bin) = bin {
            target.bin = PathBuf::from(bin);
        }
        target
    }

    fn line<T>(&self, call: &Call<T>) -> Vec<u8> {
        wire::encode(&format!("{}:1", self.client), call.method(), call.params())
    }

    fn timeout_of<T>(&self, call: &Call<T>) -> Duration {
        call.timeout().unwrap_or(self.timeout)
    }
}

/// The constructors and settings both clients share.
macro_rules! client_config {
    () => {
        /// A client of the server at `server`, with the `herdr` binary from
        /// `PATH` for the CLI path and the default timeout.
        pub fn new(server: ServerId) -> Self {
            Self::from_target(crate::herdr_api::Target::new(server))
        }

        /// A client of this process's server ([`ServerId::from_env`]), with
        /// `$HERDR_BIN_PATH` (else `herdr`) for the CLI path.
        pub fn from_env() -> Self {
            Self::resolve(|name| std::env::var_os(name))
        }

        /// `from_env` over `var` instead of the environment.
        pub fn resolve(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
            Self::from_target(crate::herdr_api::Target::resolve(var))
        }

        /// The `herdr` binary for the CLI path.
        pub fn with_bin(mut self, bin: impl Into<std::path::PathBuf>) -> Self {
            self.target.bin = bin.into();
            self
        }

        /// The timeout of each request and CLI call (default
        /// [`TIMEOUT`](crate::herdr_api::TIMEOUT)).
        pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
            self.target.timeout = timeout;
            self
        }

        /// The client name in request ids: `panorama` sends `panorama:1`.
        pub fn with_client(mut self, client: impl Into<String>) -> Self {
            self.target.client = client.into();
            self
        }

        /// The server.
        pub fn server(&self) -> &ServerId {
            &self.target.server
        }

        /// The `herdr` binary of the CLI path.
        pub fn bin(&self) -> &std::path::Path {
            &self.target.bin
        }
    };
}
use client_config;

/// The typed methods, one per constructor in [`calls`], generated for both
/// clients from this one list.
macro_rules! typed_methods {
    ($mac:ident) => {
        $mac! {
            snapshot() -> Snapshot;
            workspaces() -> Vec<WorkspaceInfo>;
            tabs(workspace: &WorkspaceId) -> Vec<TabInfo>;
            tab(tab: &TabId) -> TabInfo;
            create_tab(label: &Sanitized, cwd: Option<&std::path::Path>, focus: bool, workspace: Option<&WorkspaceId>) -> TabInfo;
            rename_tab(tab: &TabId, label: &Sanitized) -> Option<TabInfo>;
            focus_tab(tab: &TabId) -> Option<TabInfo>;
            move_tab(tab: &TabId, insert_index: u32) -> Vec<TabInfo>;
            close_tab(tab: &TabId) -> ();
            panes() -> Vec<PaneInfo>;
            pane(pane: &PaneId) -> PaneInfo;
            focus_pane(pane: &PaneId) -> Option<PaneInfo>;
            rename_pane(pane: &PaneId, label: &Sanitized) -> Option<PaneInfo>;
            toggle_zoom(pane: &PaneId) -> Extra;
            read_pane(pane: &PaneId, lines: u16, ansi: bool) -> String;
            process_info(pane: &PaneId) -> ProcessInfo;
            move_pane(pane: &PaneId, to: &MoveTo, focus: bool) -> MoveResult;
            swap_panes(source: &PaneId, target: &PaneId) -> SwapResult;
            pane_layout(pane: &PaneId) -> LayoutInfo;
            layout_export(tab: &TabId) -> LayoutInfo;
            set_split_ratio(tab: &TabId, path: &[bool], ratio: f64) -> Option<LayoutInfo>;
            apply_layout(tab_label: &Sanitized, focus: bool, root: serde_json::Value) -> LayoutInfo;
            set_token(pane: &PaneId, source: &str, key: &str, value: Option<&Sanitized>) -> ();
            agents() -> Vec<AgentInfo>;
            agent(pane: &PaneId) -> Option<AgentInfo>;
            agent_named(name: &str) -> Option<AgentInfo>;
            focus_agent(pane: &PaneId) -> Option<AgentInfo>;
            prompt(pane: &PaneId, text: &Sanitized) -> ();
            plugins() -> Vec<PluginInfo>;
            invoke_action(action_id: &str) -> ();
            open_plugin_pane(open: &OpenPluginPane) -> Option<PaneInfo>;
            close_plugin_pane(pane: &PaneId) -> ();
            notify(title: &Sanitized, body: &Sanitized, sound: Option<&str>) -> NotificationResult;
            reload_config() -> Extra;
        }
    };
}
use typed_methods;

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
        call.parse(transport::exchange(self.target.server.socket(), &line, timeout).await)
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

    /// `herdr --machine <id> api snapshot` (timeout [`MACHINE_SNAPSHOT_TIMEOUT`]).
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
        cli::run(&self.target.bin, inv, self.target.timeout).await
    }
}

typed_methods!(async_methods);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn from_env_reads_the_socket_and_the_binary() {
        let vars: HashMap<&str, OsString> = [
            ("HERDR_SOCKET_PATH", OsString::from("/run/h.sock")),
            ("HERDR_BIN_PATH", OsString::from("/opt/herdr")),
        ]
        .into();
        let h = Herdr::resolve(|n| vars.get(n).cloned());
        assert_eq!(h.server().socket(), Path::new("/run/h.sock"));
        assert_eq!(h.bin(), Path::new("/opt/herdr"));
        let h = Herdr::resolve(|n| (n == "HOME").then(|| OsString::from("/home/u")));
        assert_eq!(h.bin(), Path::new("herdr"));
        assert_eq!(
            h.server().socket(),
            Path::new("/home/u/.config/herdr/herdr.sock")
        );
    }

    #[test]
    fn request_ids_name_the_client() {
        let h = Herdr::new(ServerId::new("/x")).with_client("panorama");
        let line = h.target.line(&calls::snapshot());
        let v: Value = serde_json::from_slice(&line).unwrap();
        assert_eq!(v["id"], "panorama:1");
        assert_eq!(h.blocking().server(), h.server());
    }
}
