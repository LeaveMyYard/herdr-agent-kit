//! [`Target`]: where and how to reach one herdr server, and the
//! constructors and settings both clients share (`client_config!`).

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use super::calls::Call;
use super::ids::ServerId;
use super::wire;
use super::{DEFAULT_CLIENT, TIMEOUT};

/// Where and how to reach one herdr server; shared by both clients.
#[derive(Clone, Debug)]
pub(super) struct Target {
    pub(super) server: ServerId,
    pub(super) bin: PathBuf,
    pub(super) timeout: Duration,
    pub(super) client: String,
}

impl Target {
    pub(super) fn new(server: ServerId) -> Self {
        Self {
            server,
            bin: PathBuf::from("herdr"),
            timeout: TIMEOUT,
            client: DEFAULT_CLIENT.to_owned(),
        }
    }

    pub(super) fn resolve(var: impl Fn(&str) -> Option<OsString>) -> Self {
        let bin = var("HERDR_BIN_PATH").filter(|b| !b.is_empty());
        let mut target = Self::new(ServerId::resolve(&var));
        if let Some(bin) = bin {
            target.bin = PathBuf::from(bin);
        }
        target
    }

    pub(super) fn line<T>(&self, call: &Call<T>) -> Vec<u8> {
        wire::encode(&format!("{}:1", self.client), call.method(), call.params())
    }

    pub(super) fn timeout_of<T>(&self, call: &Call<T>) -> Duration {
        call.timeout().unwrap_or(self.timeout)
    }
}

/// The constructors and settings both clients share.
macro_rules! client_config {
    () => {
        /// A client of the server at `server`, with the `herdr` binary from
        /// `PATH` for the CLI path and the default timeout.
        pub fn new(server: ServerId) -> Self {
            Self::from_target(crate::herdr_api::target::Target::new(server))
        }

        /// A client of this process's server ([`ServerId::from_env`]), with
        /// `$HERDR_BIN_PATH` (else `herdr`) for the CLI path.
        pub fn from_env() -> Self {
            Self::resolve(|name| std::env::var_os(name))
        }

        /// `from_env` over `var` instead of the environment.
        pub fn resolve(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
            Self::from_target(crate::herdr_api::target::Target::resolve(var))
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
pub(super) use client_config;
