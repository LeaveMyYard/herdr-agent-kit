//! The two plugins' own state layouts: plugin ids, state directories, server
//! keys, lock and socket paths. **Plugin-only: not public API, outside
//! semver** (see [`crate::internal`]).
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
//! The two plugins key their state differently (panorama by
//! `sha1(HERDR_SOCKET_PATH or "default")`, the inbox by `sha1(server_id())`
//! next to its database), and both keep doing so. The general pieces (the
//! environment, locks, private directories, `short_sha1`) are public in
//! [`crate::plugin_env`].

pub mod inbox;
pub mod panorama;

/// herdr-panorama's plugin id.
pub const PANORAMA_ID: &str = "leavemyyard.herdr-panorama";
/// herdr-inbox's plugin id.
pub const INBOX_ID: &str = "leavemyyard.herdr-inbox";

/// A Claude Code session id as it may appear in a file name: a canonical
/// 8-4-4-4-12 hex UUID, any case in, lowercase out; anything else is `None`.
///
/// The same rule as herdr-inbox's `transcript.session_id_or_none`. It is
/// crate-private so a lock path is never built from anything else; the public
/// validator is [`crate::agent_session::SessionId`].
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
