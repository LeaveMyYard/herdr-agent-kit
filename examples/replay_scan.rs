//! The Rust side of the differential replay (`tools/replay/`, plan §5.2 item 4).
//!
//! Reads one request per stdin line and answers one JSON line per request on
//! stdout. A request is
//! `{"id": <session id>, "root": <main transcript>, "hooks": <hook file or null>,
//! "since": <f64 or null>, "now": <f64>}`; the answer is
//! `{"agents": {<agent id>: {"status", "ended", "inferred", "parent"}}}`, or
//! `{"error": <text>}`. Each request scans its files from scratch with
//! [`Scanner`], as one panorama `Session.update(since)` does, and asks the
//! status rules with [`NoProcs`]: a past instant's processes cannot be checked.
//!
//! The answer carries ids, status codes and times only, never transcript text.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use herdr_agent_kit::agent_procs::NoProcs;
use herdr_agent_kit::claude_session::{Owner, Scanner, SessionId};
use serde_json::{Map, Value, json};

fn answer(req: &Value) -> Result<Value, String> {
    let id = req
        .get("id")
        .and_then(SessionId::from_value)
        .ok_or("id: not a session id")?;
    let root = req
        .get("root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or("root: missing")?;
    let hooks = req.get("hooks").and_then(Value::as_str).map(PathBuf::from);
    let since = req.get("since").and_then(Value::as_f64);
    let now = req
        .get("now")
        .and_then(Value::as_f64)
        .ok_or("now: missing")?;
    let mut scanner = Scanner::new(id, root, None, hooks);
    scanner.update(since);
    let session = scanner.session();
    let mut agents = Map::new();
    for agent in session.agents().keys() {
        let st = session.status_of(agent, now, &NoProcs);
        let parent = session.parent_of(agent).map(|o| match o {
            Owner::Main => "main".to_string(),
            Owner::Agent(a) => a,
        });
        agents.insert(
            agent.clone(),
            json!({
                "status": st.status.code(),
                "ended": st.ended,
                "inferred": st.inferred,
                "parent": parent,
            }),
        );
    }
    Ok(json!({ "agents": agents }))
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = serde_json::from_str::<Value>(&line)
            .map_err(|e| format!("request: {e}"))
            .and_then(|req| answer(&req))
            .unwrap_or_else(|e| json!({ "error": e }));
        serde_json::to_writer(&mut out, &reply)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }
    Ok(())
}
