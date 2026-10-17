//! [`Events`]: the reconnecting `events.subscribe` stream.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::Instant;

use super::{Event, EventKind, FIRST_BACKOFF, HerdrEvent, MAX_BACKOFF};
use crate::herdr_api::wire::{self, MAX_REPLY, truthy};

/// A reconnecting subscription. Never ends and never fails: it reconnects
/// (backing off) for as long as it is polled. Drop it to stop.
///
/// [`Events::next`] is cancel-safe: a partly read line is kept for the next
/// call, so it can sit in a `tokio::select!`.
///
/// As in panorama:
///
/// - only an acknowledgement or an event proves the stream works: one with a
///   truthy `result` or `data` marks it live ([`HerdrEvent::Connected`]) and
///   resets the backoff;
/// - a line that is not JSON is skipped;
/// - an error reply (an unknown subscription type) is not live and does not
///   end the connection; herdr keeps it open, so nothing more arrives on it.
pub struct Events {
    socket: PathBuf,
    request: Vec<u8>,
    conn: Option<BufReader<UnixStream>>,
    line: Vec<u8>,
    live: bool,
    backoff: Duration,
    /// When the next connect may start: kept here, not in a sleeping future,
    /// so a cancelled `next()` resumes the same wait instead of restarting it.
    retry_at: Option<Instant>,
    ready: Option<HerdrEvent>,
}

impl Events {
    pub(crate) fn new(socket: PathBuf, client: &str, kinds: &[EventKind]) -> Self {
        let subscriptions: Vec<Value> = kinds.iter().map(|k| json!({"type": k.as_str()})).collect();
        let request = wire::encode(
            &format!("{client}:events"),
            "events.subscribe",
            &json!({"subscriptions": subscriptions}),
        );
        Self {
            socket,
            request,
            conn: None,
            line: Vec::new(),
            live: false,
            backoff: FIRST_BACKOFF,
            retry_at: None,
            ready: None,
        }
    }

    /// The subscribe request line this stream sends on every connect.
    pub fn request(&self) -> &[u8] {
        &self.request
    }

    /// Wait the current backoff before the next connect (panorama sleeps
    /// after every connection that ends or fails), then double it.
    fn back_off(&mut self) {
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }

    async fn connect(&self) -> Option<BufReader<UnixStream>> {
        let mut stream = UnixStream::connect(&self.socket).await.ok()?;
        stream.write_all(&self.request).await.ok()?;
        Some(BufReader::new(stream))
    }

    /// The next event or marker.
    pub async fn next(&mut self) -> HerdrEvent {
        loop {
            if let Some(event) = self.ready.take() {
                return event;
            }
            let Some(conn) = self.conn.as_mut() else {
                if let Some(at) = self.retry_at {
                    tokio::time::sleep_until(at).await;
                }
                // cancelled from here on, the deadline has passed: the next
                // call simply connects again
                self.line.clear();
                self.conn = self.connect().await;
                self.retry_at = None;
                if self.conn.is_none() {
                    self.back_off();
                }
                continue;
            };
            let room = MAX_REPLY.saturating_sub(self.line.len()) as u64;
            let read = (&mut *conn)
                .take(room)
                .read_until(b'\n', &mut self.line)
                .await;
            let complete = self.line.ends_with(b"\n");
            match read {
                Ok(n) if n > 0 && complete => {
                    let line = std::mem::take(&mut self.line);
                    if let Some(event) = self.handle(&line) {
                        return event;
                    }
                }
                // the end of the stream, an error, or a line over the cap
                _ => {
                    self.conn = None;
                    self.back_off();
                    if std::mem::take(&mut self.live) {
                        return HerdrEvent::Lost;
                    }
                }
            }
        }
    }

    /// One line from herdr: the markers and events it means, the first
    /// returned and a second (an event right after `Connected`) kept.
    fn handle(&mut self, line: &[u8]) -> Option<HerdrEvent> {
        let Ok(Value::Object(mut msg)) = serde_json::from_slice::<Value>(line) else {
            return None;
        };
        let proves = msg.get("result").is_some_and(truthy) || msg.get("data").is_some_and(truthy);
        let event = match msg.remove("data") {
            Some(Value::Object(data)) => Some(HerdrEvent::Event(Event {
                kind: data
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                data,
            })),
            _ => None,
        };
        if proves {
            self.backoff = FIRST_BACKOFF;
            if !self.live {
                self.live = true;
                self.ready = event;
                return Some(HerdrEvent::Connected);
            }
        }
        event
    }
}
