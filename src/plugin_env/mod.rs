//! The plugin's environment. **IO** (environment, files, locks).
//!
//! The herdr plugin environment variables, `config.toml` with reload, state
//! and config directories, `flock`-based locks and private socket directories.
//! Code outside this module and [`crate::herdr_api`] does not read the
//! environment: it reads a [`PluginEnv`] instead.
//!
//! The locks and paths are the ones the Python plugins use, byte for byte
//! (plan §5.4). Each plugin's own state layout (its ids, state directory,
//! server key and lock paths) is plugin-only and lives in the `internal`
//! module (feature `internal`), outside semver.
//!
//! - [`vars`]: [`PluginEnv`], a snapshot of the environment taken once.
//! - [`lock`]: `flock(2)` locks with the Python plugins' open modes and probes.
//! - [`config`]: `config.toml` in the plugin config directory, re-read when it
//!   changes, parsed as TOML 1.0 (what Python's `tomllib` reads).
//! - [`private_dir`]: the private (0700) directory a Unix socket lives in.

pub mod config;
pub mod lock;
pub mod private_dir;
mod toml10;
pub mod vars;

pub use config::{ConfigError, ConfigFile};
pub use lock::{HeldLock, LockFile};
pub use vars::PluginEnv;
