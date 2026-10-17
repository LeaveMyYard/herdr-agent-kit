//! The plugin's environment. **IO** (environment, files, locks).
//!
//! The herdr plugin environment variables, `config.toml` with reload, state
//! and config directories, `flock`-based locks and private socket directories.
//! Code outside this module and [`crate::herdr_api`] does not read the
//! environment: it reads a [`PluginEnv`] instead.
//!
//! Every path and lock here is the one the Python plugins use, byte for byte,
//! so that during the migration (plan §5.4) a Python process and a Rust
//! process see the same state and exclude each other:
//!
//! | What | Python | Here |
//! |---|---|---|
//! | inbox delivery worker | `worker._lock_path` | [`inbox::worker_lock_path`] |
//! | inbox rewake watcher | `rewake._dir() / "<session>.lock"` | [`inbox::rewake_lock_path`] |
//! | panorama core | `protocol.lock_path` | [`panorama::core_lock_path`] |
//!
//! - [`vars`]: [`PluginEnv`], a snapshot of the environment taken once.
//! - [`panorama`] and [`inbox`]: each plugin's state directory, server key
//!   and lock paths. The two plugins key their state differently (panorama by
//!   `sha1(HERDR_SOCKET_PATH or "default")`, the inbox by `sha1(server_id())`
//!   next to its database), and both keep doing so.
//! - [`lock`]: `flock(2)` locks with the Python plugins' open modes and probes.
//! - [`config`]: `config.toml` in the plugin config directory, re-read when it
//!   changes, parsed as TOML 1.0 by [`toml10`] (what Python's `tomllib` reads).
//! - [`private_dir`]: the private (0700) directory a Unix socket lives in.

pub mod config;
pub mod inbox;
pub mod lock;
pub mod panorama;
pub mod private_dir;
pub mod toml10;
pub mod vars;

pub use config::ConfigFile;
pub use lock::{HeldLock, LockFile};
pub use vars::PluginEnv;

/// herdr-panorama's plugin id.
pub const PANORAMA_ID: &str = "leavemyyard.herdr-panorama";
/// herdr-inbox's plugin id.
pub const INBOX_ID: &str = "leavemyyard.herdr-inbox";

/// The first 12 hex digits of the SHA-1 of `bytes`: the key both plugins use
/// to name per-server state (`hashlib.sha1(s.encode()).hexdigest()[:12]`).
pub fn short_sha1(bytes: &[u8]) -> String {
    let mut hex = sha1_smol::Sha1::from(bytes).digest().to_string();
    hex.truncate(12);
    hex
}

/// A Claude Code session id as it may appear in a file name: a canonical
/// 8-4-4-4-12 hex UUID, any case in, lowercase out; anything else is `None`.
///
/// The same rule as herdr-inbox's `transcript.session_id_or_none`. It is
/// crate-private so a lock path is never built from anything else; the public
/// validator belongs to `agent_session` (`SessionId`).
pub(crate) fn session_file_name(value: &str) -> Option<String> {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = value.split('-');
    for want in GROUPS {
        let part = parts.next()?;
        if part.len() != want || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
    }
    if parts.next().is_some() {
        return None;
    }
    Some(value.to_ascii_lowercase())
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

    #[test]
    fn session_file_name_is_the_canonical_uuid_rule() {
        let id = "1CD70DE7-aaaa-4bbb-8ccc-0123456789AB";
        assert_eq!(
            session_file_name(id).as_deref(),
            Some("1cd70de7-aaaa-4bbb-8ccc-0123456789ab")
        );
        for bad in [
            "",
            "../x",
            "*",
            "1cd70de7-aaaa-4bbb-8ccc-0123456789ab ",
            " 1cd70de7-aaaa-4bbb-8ccc-0123456789ab",
            "1cd70de7-aaaa-4bbb-8ccc-0123456789ab-",
            "1cd70de7-aaaa-4bbb-8ccc-0123456789a",
            "1cd70de7aaaa4bbb8ccc0123456789ab",
            "1cd70de7-aaaa-4bbb-8ccc-0123456789ag",
            "1cd70de7-aaaa-4bbb-8ccc/0123456789ab",
            "\u{ff11}cd70de7-aaaa-4bbb-8ccc-0123456789ab",
        ] {
            assert_eq!(session_file_name(bad), None, "{bad:?}");
        }
    }
}
