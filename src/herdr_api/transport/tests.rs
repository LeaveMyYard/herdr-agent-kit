//! The blocking read when herdr has already answered and gone.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use serde_json::json;

use super::read_reply;
use crate::herdr_api::error::HerdrError;

/// herdr answers and closes at once, so its reply and its close can both be
/// there before the client looks. macOS refuses `setsockopt` on such a socket
/// (EINVAL), which must not cost the client a reply that arrived.
#[test]
fn a_reply_from_a_peer_that_already_closed_is_read() {
    let (mut server, client) = UnixStream::pair().unwrap();
    writeln!(server, r#"{{"id": "x:1", "result": {{"ok": true}}}}"#).unwrap();
    drop(server);
    let deadline = Instant::now() + Duration::from_secs(3);
    assert_eq!(read_reply(client, deadline).unwrap(), json!({"ok": true}));
}

/// A client held up past its deadline still has the reply herdr sent: the
/// call was carried out, and reading it costs nothing.
#[test]
fn a_reply_already_here_is_read_past_the_deadline() {
    let (mut server, client) = UnixStream::pair().unwrap();
    writeln!(server, r#"{{"id": "x:1", "result": {{"ok": true}}}}"#).unwrap();
    assert_eq!(
        read_reply(client, Instant::now()).unwrap(),
        json!({"ok": true})
    );
}

#[test]
fn a_peer_that_closed_without_a_reply_is_no_reply() {
    let (server, client) = UnixStream::pair().unwrap();
    drop(server);
    let deadline = Instant::now() + Duration::from_secs(3);
    let error = read_reply(client, deadline).unwrap_err();
    let closed = HerdrError::NoReply("closed without a reply".into());
    assert_eq!(error.to_string(), closed.to_string());
}

/// The read timeout set before the request is the whole budget: the read
/// must still tighten it, or a silent peer holds the client past `deadline`.
#[test]
fn a_silent_peer_is_a_timeout_at_the_deadline() {
    let (_server, client) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let start = Instant::now();
    let error = read_reply(client, start + Duration::from_millis(50)).unwrap_err();
    assert_eq!(
        error.to_string(),
        HerdrError::NoReply("timed out".into()).to_string()
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}
