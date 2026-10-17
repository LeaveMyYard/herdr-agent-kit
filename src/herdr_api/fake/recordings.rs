//! The herdr socket and CLI recordings of `fixtures/herdr` (captured on an
//! isolated herdr 0.9.3), embedded in the crate, and readers for them.

use serde_json::Value;

/// The herdr recordings, embedded: `(file name, contents)`.
const RECORDINGS: &[(&str, &str)] = &[
    (
        "agent.jsonl",
        include_str!("../../../fixtures/herdr/agent.jsonl"),
    ),
    (
        "api.jsonl",
        include_str!("../../../fixtures/herdr/api.jsonl"),
    ),
    (
        "errors.jsonl",
        include_str!("../../../fixtures/herdr/errors.jsonl"),
    ),
    (
        "events.jsonl",
        include_str!("../../../fixtures/herdr/events.jsonl"),
    ),
    (
        "machine.jsonl",
        include_str!("../../../fixtures/herdr/machine.jsonl"),
    ),
    (
        "meta.jsonl",
        include_str!("../../../fixtures/herdr/meta.jsonl"),
    ),
    (
        "notification.jsonl",
        include_str!("../../../fixtures/herdr/notification.jsonl"),
    ),
    (
        "pane.jsonl",
        include_str!("../../../fixtures/herdr/pane.jsonl"),
    ),
    (
        "plugin.jsonl",
        include_str!("../../../fixtures/herdr/plugin.jsonl"),
    ),
    (
        "server.jsonl",
        include_str!("../../../fixtures/herdr/server.jsonl"),
    ),
    (
        "socket.jsonl",
        include_str!("../../../fixtures/herdr/socket.jsonl"),
    ),
    (
        "tab.jsonl",
        include_str!("../../../fixtures/herdr/tab.jsonl"),
    ),
    (
        "workspace.jsonl",
        include_str!("../../../fixtures/herdr/workspace.jsonl"),
    ),
];

/// Every record of the embedded recordings, parsed: `(file name, record)`.
pub fn recordings() -> Vec<(&'static str, Value)> {
    RECORDINGS
        .iter()
        .flat_map(|(file, text)| {
            text.lines().filter(|l| !l.trim().is_empty()).map(|l| {
                (
                    *file,
                    serde_json::from_str(l).expect("recordings are JSON lines"),
                )
            })
        })
        .collect()
}

/// One record: `file` is `agent.jsonl`, `seq` its `seq`.
pub fn recording(file: &str, seq: u64) -> Option<Value> {
    recordings()
        .into_iter()
        .find(|(f, r)| *f == file && r["seq"].as_u64() == Some(seq))
        .map(|(_, r)| r)
}

/// The socket `(request frame, reply frame)` pairs of one record, in order
/// (the CLI's `ping` included).
pub fn exchanges(record: &Value) -> Vec<(Value, Value)> {
    let frames = record["socket"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        if f["dir"] != "request" || f.get("frame").is_none() {
            continue;
        }
        let reply = frames[i + 1..]
            .iter()
            .find(|r| r["conn"] == f["conn"] && r["dir"] == "reply" && r.get("frame").is_some());
        if let Some(reply) = reply {
            out.push((f["frame"].clone(), reply["frame"].clone()));
        }
    }
    out
}
