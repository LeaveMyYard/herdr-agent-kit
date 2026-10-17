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

pub use config::ConfigFile;
pub use lock::{HeldLock, LockFile};
pub use vars::PluginEnv;

/// The first 12 hex digits of the SHA-1 of `bytes`: the key both plugins use
/// to name per-server state (`hashlib.sha1(s.encode()).hexdigest()[:12]`).
pub fn short_sha1(bytes: &[u8]) -> String {
    let mut hex = sha1_smol::Sha1::from(bytes).digest().to_string();
    hex.truncate(12);
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sha1_matches_python_hashlib() {
        // python3 -c 'import hashlib; print(hashlib.sha1(s.encode()).hexdigest()[:12])'
        assert_eq!(short_sha1(b"default"), "7505d64a54e0");
        assert_eq!(short_sha1(b"/run/herdr.sock"), "837fc5258150");
        assert_eq!(short_sha1(b""), "da39a3ee5e6b");
    }
}
