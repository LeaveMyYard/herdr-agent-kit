//! One request, one reply, over herdr's socket: async (tokio) and blocking
//! (std). Both map failures the same way:
//!
//! - connecting fails with "no such file" or "connection refused":
//!   [`HerdrError::NotRunning`]; any other connect or write failure:
//!   [`HerdrError::Unreachable`] (nothing was sent);
//! - after the request is written, a timeout or a connection closed without
//!   a reply: [`HerdrError::NoReply`]; an unreadable reply:
//!   [`HerdrError::Garbled`].
//!
//! The timeout covers the whole exchange.

use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::error::HerdrError;
use super::wire::{self, MAX_REPLY};

/// How a failed connect reads.
pub(crate) fn connect_error(e: &io::Error) -> HerdrError {
    match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => HerdrError::NotRunning,
        _ => HerdrError::Unreachable(e.to_string()),
    }
}

fn timed_out() -> HerdrError {
    HerdrError::NoReply("timed out".into())
}

fn finish(buf: &[u8]) -> Result<Value, HerdrError> {
    if buf.len() >= MAX_REPLY && !buf.contains(&b'\n') {
        return Err(HerdrError::Garbled(format!(
            "reply longer than {MAX_REPLY} bytes"
        )));
    }
    wire::decode(buf)
}

/// Send `line` and read the reply (async).
pub(crate) async fn exchange(
    socket: &Path,
    line: &[u8],
    timeout: Duration,
) -> Result<Value, HerdrError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut stream =
        match tokio::time::timeout_at(deadline, tokio::net::UnixStream::connect(socket)).await {
            Err(_) => return Err(HerdrError::Unreachable("connect timed out".into())),
            Ok(Err(e)) => return Err(connect_error(&e)),
            Ok(Ok(s)) => s,
        };
    match tokio::time::timeout_at(deadline, stream.write_all(line)).await {
        Err(_) => return Err(HerdrError::Unreachable("write timed out".into())),
        Ok(Err(e)) => return Err(HerdrError::Unreachable(e.to_string())),
        Ok(Ok(())) => {}
    }
    let mut reader = tokio::io::BufReader::new(stream).take(MAX_REPLY as u64);
    let mut buf = Vec::new();
    match tokio::time::timeout_at(deadline, reader.read_until(b'\n', &mut buf)).await {
        Err(_) => Err(timed_out()),
        Ok(Err(e)) => Err(HerdrError::NoReply(e.to_string())),
        Ok(Ok(_)) => finish(&buf),
    }
}

/// A blocking connect, bounded: a Unix-socket connect blocks while the
/// listener's backlog is full (a wedged herdr that stopped accepting). It runs
/// on a helper thread; past `timeout` the thread is left to finish on its own
/// and the caller gets [`HerdrError::Unreachable`] (nothing was sent), as the
/// async path and Python's `settimeout` before `connect` give.
fn connect_blocking(
    socket: &Path,
    timeout: Duration,
) -> Result<std::os::unix::net::UnixStream, HerdrError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let path = socket.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(std::os::unix::net::UnixStream::connect(path));
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(e)) => Err(connect_error(&e)),
        Err(_) => Err(HerdrError::Unreachable("connect timed out".into())),
    }
}

/// Send `line` and read the reply (blocking).
pub(crate) fn exchange_blocking(
    socket: &Path,
    line: &[u8],
    timeout: Duration,
) -> Result<Value, HerdrError> {
    let deadline = Instant::now() + timeout;
    let mut stream = connect_blocking(socket, timeout)?;
    let left = |deadline: Instant| {
        deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1))
    };
    stream
        .set_write_timeout(Some(left(deadline)))
        .and_then(|()| stream.write_all(line))
        .map_err(|e| HerdrError::Unreachable(e.to_string()))?;
    let mut reader = io::BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err(timed_out());
        }
        reader
            .get_ref()
            .set_read_timeout(Some(left(deadline)))
            .map_err(|e| HerdrError::NoReply(e.to_string()))?;
        let room = (MAX_REPLY - buf.len()) as u64;
        match (&mut reader).take(room).read_until(b'\n', &mut buf) {
            // a newline, the end of the stream or the size cap
            Ok(_) => return finish(&buf),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(timed_out());
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(HerdrError::NoReply(e.to_string())),
        }
    }
}
