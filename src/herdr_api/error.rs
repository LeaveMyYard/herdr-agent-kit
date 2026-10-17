//! [`HerdrError`]: what went wrong talking to herdr, and whether herdr may
//! have acted anyway.

use std::fmt;

/// The error code herdr's CLI prints when its server is not running
/// (herdr-inbox `herdr.SERVER_GONE`).
pub const SERVER_NOT_RUNNING: &str = "server_not_running";

/// A failed herdr call.
///
/// The split that matters to callers is [`HerdrError::maybe_done`]: was the
/// request sent and left unanswered (herdr may have acted), or was it refused
/// or never sent (herdr did nothing)?
///
/// Mapping from the Python clients:
///
/// | Python | Rust |
/// |---|---|
/// | inbox `server_gone()` True (`server_not_running`) | [`NotRunning`](Self::NotRunning) |
/// | socket connect failed, no herdr binary (a plain error string, "not gone") | [`Unreachable`](Self::Unreachable) |
/// | herdr answered `{"error": ...}`; CLI exit != 0 | [`Refused`](Self::Refused) |
/// | panorama `NoReply` (sent; timed out or closed unanswered) | [`NoReply`](Self::NoReply) |
/// | panorama `NoReply("garbled reply")` (sent; unreadable answer) | [`Garbled`](Self::Garbled) |
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HerdrError {
    /// herdr's server is not running: the socket does not exist or refuses
    /// connections, or the CLI said `server_not_running`. Nothing was sent.
    NotRunning,
    /// herdr could not be reached for another reason (no `herdr` binary, a
    /// socket permission error, the request could not be written). Nothing
    /// was acted on, and this is **not** proof that the server is gone.
    Unreachable(String),
    /// herdr answered with an error. `code` is herdr's error code
    /// (`pane_not_found`, `ui_busy`, ...); it is empty when herdr gave none
    /// (a CLI usage error, `--machine <unknown>`: plain text on stderr).
    Refused {
        /// herdr's error code, or empty.
        code: String,
        /// herdr's message, or the CLI's stderr.
        message: String,
    },
    /// The request was sent and no reply came (a timeout, a connection closed
    /// without a reply): herdr may have done it all the same.
    NoReply(String),
    /// The request was sent and the reply could not be read: herdr may have
    /// done it all the same.
    Garbled(String),
}

impl HerdrError {
    /// The request went out and herdr may have acted on it ([`NoReply`] or
    /// [`Garbled`]): Python panorama's `NoReply`. Never a success.
    ///
    /// [`NoReply`]: Self::NoReply
    /// [`Garbled`]: Self::Garbled
    pub fn maybe_done(&self) -> bool {
        matches!(self, Self::NoReply(_) | Self::Garbled(_))
    }

    /// herdr's error code when it refused the request.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Refused { code, .. } if !code.is_empty() => Some(code),
            _ => None,
        }
    }

    /// Whether herdr refused with this error code.
    pub fn is_code(&self, code: &str) -> bool {
        self.code() == Some(code)
    }
}

impl fmt::Display for HerdrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => f.write_str("herdr is not running"),
            Self::Unreachable(why) => write!(f, "herdr unreachable: {why}"),
            Self::Refused { code, message } if code.is_empty() => f.write_str(message),
            Self::Refused { code, message } => write!(f, "{code}: {message}"),
            Self::NoReply(why) => write!(f, "no reply from herdr: {why}"),
            Self::Garbled(why) => write!(f, "garbled reply from herdr: {why}"),
        }
    }
}

impl std::error::Error for HerdrError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sent_and_unanswered_requests_may_have_happened() {
        assert!(HerdrError::NoReply("timed out".into()).maybe_done());
        assert!(HerdrError::Garbled("x".into()).maybe_done());
        assert!(!HerdrError::NotRunning.maybe_done());
        assert!(!HerdrError::Unreachable("x".into()).maybe_done());
        let refused = HerdrError::Refused {
            code: "ui_busy".into(),
            message: "a popup pane is already open".into(),
        };
        assert!(!refused.maybe_done());
        assert!(refused.is_code("ui_busy"));
        assert_eq!(refused.to_string(), "ui_busy: a popup pane is already open");
        let plain = HerdrError::Refused {
            code: String::new(),
            message: "error: unknown machine 'nosuch'".into(),
        };
        assert_eq!(plain.code(), None);
        assert_eq!(plain.to_string(), "error: unknown machine 'nosuch'");
    }
}
