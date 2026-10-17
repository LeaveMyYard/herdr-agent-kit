//! The fake's script: [`Reply`], how it answers one request; [`Request`],
//! one request as it received it; and the replies queued, set and
//! recorded, of which one answers each request.

use std::collections::{HashMap, VecDeque};
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

/// How the fake answers one request.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// `{"id", "result": value}`.
    Result(Value),
    /// `{"id", "error": {"code", "message"}}`.
    Error {
        /// herdr's error code.
        code: String,
        /// herdr's message.
        message: String,
    },
    /// Close the connection without a reply.
    Close,
    /// Send these bytes as they are (a garbled reply), then close.
    Raw(Vec<u8>),
    /// Never answer: hold the connection until the client gives up.
    Hang,
}

impl Reply {
    /// A refusal with herdr's error code.
    pub fn error(code: &str, message: &str) -> Self {
        Self::Error {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }

    /// A recorded reply frame as a reply.
    pub(super) fn from_frame(frame: &Value) -> Self {
        match frame.get("error") {
            Some(e) => Self::Error {
                code: e["code"].as_str().unwrap_or_default().to_owned(),
                message: e["message"].as_str().unwrap_or_default().to_owned(),
            },
            None => Self::Result(frame.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}

/// One request as the fake received it.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The request's `id` (`Null` when missing).
    pub id: Value,
    /// The method (empty when the line was not a JSON request).
    pub method: String,
    /// The params (`Null` when missing).
    pub params: Value,
    /// The line as received, without its newline.
    pub line: String,
}

pub(super) fn frame(id: &Value, reply: &Reply) -> Option<String> {
    let frame = match reply {
        Reply::Result(v) => json!({"id": id, "result": v}),
        Reply::Error { code, message } => {
            json!({"id": id, "error": {"code": code, "message": message}})
        }
        Reply::Close | Reply::Raw(_) | Reply::Hang => return None,
    };
    Some(format!("{frame}\n"))
}

#[derive(Default)]
pub(super) struct State {
    /// `(method, params, reply)` from the recordings; the last one wins.
    pub(super) recorded: Vec<(String, Value, Reply)>,
    pub(super) sticky: HashMap<String, Reply>,
    pub(super) once: HashMap<String, VecDeque<Reply>>,
    pub(super) requests: Vec<Request>,
    pub(super) subscribers: Vec<UnixStream>,
}

impl State {
    pub(super) fn reply_for(&mut self, method: &str, params: &Value) -> Reply {
        if let Some(r) = self.once.get_mut(method).and_then(VecDeque::pop_front) {
            return r;
        }
        if let Some(r) = self.sticky.get(method) {
            return r.clone();
        }
        if method == "events.subscribe" {
            return Reply::Result(json!({"type": "subscription_started"}));
        }
        let exact = self
            .recorded
            .iter()
            .rev()
            .find(|(m, p, _)| m == method && p == params);
        let found = exact.or_else(|| self.recorded.iter().rev().find(|(m, _, _)| m == method));
        if let Some((_, _, r)) = found {
            return r.clone();
        }
        Reply::error(
            "invalid_request",
            &format!("FakeHerdr: no reply scripted or recorded for `{method}`"),
        )
    }
}
