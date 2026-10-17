//! The socket's framing: one JSON object per line each way.

use serde_json::{Map, Value, json};

use super::error::HerdrError;

/// A reply line longer than this is not read further (a snapshot of a big
/// session is well under 1 MiB).
pub const MAX_REPLY: usize = 64 << 20;

/// One request line: `{"id", "method", "params"}` plus `\n`.
pub fn encode(id: &str, method: &str, params: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&json!({"id": id, "method": method, "params": params}))
        .expect("a JSON value always serializes");
    line.push(b'\n');
    line
}

/// Read what herdr sent back for one request (panorama `_socket_call`):
///
/// - nothing (the connection closed with no reply): [`HerdrError::NoReply`];
/// - a first line that is not a JSON object: [`HerdrError::Garbled`];
/// - `{"error": ...}`: [`HerdrError::Refused`];
/// - otherwise `result`, with a missing or empty result read as `{}`
///   (Python's `reply.get("result") or {}`).
pub fn decode(buf: &[u8]) -> Result<Value, HerdrError> {
    if buf.iter().all(u8::is_ascii_whitespace) {
        return Err(HerdrError::NoReply("closed without a reply".into()));
    }
    let line = buf.split(|&b| b == b'\n').next().unwrap_or_default();
    let reply: Value =
        serde_json::from_slice(line).map_err(|e| HerdrError::Garbled(e.to_string()))?;
    let Value::Object(mut reply) = reply else {
        return Err(HerdrError::Garbled(format!("not an object: {reply}")));
    };
    if let Some(error) = reply.remove("error") {
        return Err(refusal(error));
    }
    Ok(match reply.remove("result") {
        Some(result) if truthy(&result) => result,
        _ => Value::Object(Map::new()),
    })
}

/// herdr's `{"code", "message"}` error object as a refusal.
pub fn refusal(error: Value) -> HerdrError {
    match error {
        Value::Object(e) => HerdrError::Refused {
            code: e
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            message: match e.get("message") {
                Some(Value::String(m)) => m.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            },
        },
        Value::String(message) => HerdrError::Refused {
            code: String::new(),
            message,
        },
        other => HerdrError::Refused {
            code: String::new(),
            message: other.to_string(),
        },
    }
}

/// Python truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests;
