//! One connection to the fake: read requests, record them, answer from the
//! script, and keep `events.subscribe` connections as subscribers.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::script::{Reply, Request, State, frame};

/// One connection: requests line by line until it closes.
pub(super) fn serve(conn: UnixStream, state: &Mutex<State>, stop: &AtomicBool) {
    let lock = || state.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(mut out) = conn.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    loop {
        line.clear();
        if matches!(reader.read_line(&mut line), Ok(0) | Err(_)) || stop.load(Ordering::SeqCst) {
            return;
        }
        let text = line.trim_end_matches('\n').to_owned();
        let Some(req) = serde_json::from_str::<Value>(&text)
            .ok()
            .filter(Value::is_object)
        else {
            lock().requests.push(Request {
                id: Value::Null,
                method: String::new(),
                params: Value::Null,
                line: text,
            });
            // herdr answers a line it cannot parse with an empty id
            let reply = Reply::error("invalid_request", "invalid request: not a JSON request");
            match frame(&json!(""), &reply) {
                Some(f) if out.write_all(f.as_bytes()).is_ok() => continue,
                _ => return,
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req["method"].as_str().unwrap_or_default().to_owned();
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let reply = {
            let mut st = lock();
            st.requests.push(Request {
                id: id.clone(),
                method: method.clone(),
                params: params.clone(),
                line: text,
            });
            st.reply_for(&method, &params)
        };
        let reply_id = if id.is_null() { json!("") } else { id };
        match &reply {
            Reply::Close => return,
            Reply::Raw(bytes) => {
                let _ = out.write_all(bytes);
                return;
            }
            Reply::Hang => {
                // until the client closes its end
                let mut rest = Vec::new();
                let _ = std::io::Read::read_to_end(&mut reader, &mut rest);
                return;
            }
            Reply::Result(_) | Reply::Error { .. } => {
                let Some(f) = frame(&reply_id, &reply) else {
                    return;
                };
                // the subscriber joins before its ack goes out, so an event
                // emitted right after the client saw the ack reaches it
                if method == "events.subscribe" && matches!(reply, Reply::Result(_)) {
                    let mut st = lock();
                    if out.write_all(f.as_bytes()).is_err() {
                        return;
                    }
                    if let Ok(sub) = out.try_clone() {
                        st.subscribers.push(sub);
                    }
                } else if out.write_all(f.as_bytes()).is_err() {
                    return;
                }
            }
        }
    }
}
