//! herdr's ids: opaque strings herdr hands out (`w1:p2`, `w1:t3`, `w1`), and
//! [`ServerId`], the socket of one herdr server.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wrap an id as herdr gave it. Ids are opaque: nothing parses them.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id as herdr spells it.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
    };
}

string_id!(
    /// A pane (`w1:p2`). herdr gives a moved pane a new id.
    PaneId
);
string_id!(
    /// A tab (`w1:t2`).
    TabId
);
string_id!(
    /// A workspace (`w1`).
    WorkspaceId
);

/// One herdr server, named by its socket path. The inbox runs one delivery
/// worker per server.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerId(PathBuf);

impl ServerId {
    /// A server by its socket path.
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self(socket.into())
    }

    /// The server this process talks to, from its environment: herdr-inbox
    /// `herdr.server_id()`. `HERDR_SOCKET_PATH` when set and non-empty, else
    /// `$XDG_CONFIG_HOME/herdr/herdr.sock`, else `~/.config/herdr/herdr.sock`.
    ///
    /// `var` looks up one environment variable; [`ServerId::from_env`] passes
    /// [`std::env::var_os`]. Tests pass a map, so they never read the real
    /// environment.
    pub fn resolve(var: impl Fn(&str) -> Option<OsString>) -> Self {
        let set = |name: &str| var(name).filter(|v| !v.is_empty());
        if let Some(socket) = set("HERDR_SOCKET_PATH") {
            return Self(socket.into());
        }
        let base = set("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home(&var).join(".config"));
        Self(base.join("herdr").join("herdr.sock"))
    }

    /// [`ServerId::resolve`] over this process's environment.
    pub fn from_env() -> Self {
        Self::resolve(|name| std::env::var_os(name))
    }

    /// The socket path.
    pub fn socket(&self) -> &Path {
        &self.0
    }
}

/// `os.path.expanduser("~")`: `$HOME`, else `/` (no passwd lookup: a plugin
/// process always has `HOME`).
fn home(var: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    var("HOME")
        .filter(|h| !h.is_empty())
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

impl fmt::Display for ServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

#[cfg(test)]
mod tests;
