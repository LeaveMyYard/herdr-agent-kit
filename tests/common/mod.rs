//! Claude Code project dirs for tests, built only from captured templates
//! (`fixtures/transcripts/templates.json`, see its README): a port of
//! herdr-panorama's `tests/scenario.py`, as far as these tests use it.
//! Nothing about the wire format is invented here.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Map, Value};

/// 2026-10-14 09:00 UTC (scenario.py's BASE).
pub const BASE: i64 = 1_791_968_400;
pub const CWD: &str = "/home/user/project";
pub const SLUG: &str = "-home-user-project";

pub fn templates() -> &'static Map<String, Value> {
    static T: OnceLock<Map<String, Value>> = OnceLock::new();
    T.get_or_init(|| {
        let text = include_str!("../../fixtures/transcripts/templates.json");
        match serde_json::from_str(text) {
            Ok(Value::Object(m)) => m,
            _ => panic!("templates.json is not an object"),
        }
    })
}

/// A deep copy of the captured template of `kind`.
pub fn tpl(kind: &str) -> Map<String, Value> {
    match templates().get(kind) {
        Some(Value::Object(m)) => m.clone(),
        _ => panic!("no template {kind}"),
    }
}

/// `t` (whole seconds since the epoch) as Claude Code writes it:
/// `2026-10-14T09:00:00.000Z`.
pub fn iso(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

static UUIDS: AtomicUsize = AtomicUsize::new(1);

/// scenario.stamp: the time, a fresh record uuid, the session id and the cwd.
pub fn stamp(mut rec: Map<String, Value>, t: i64, session: &str) -> Map<String, Value> {
    rec.insert("timestamp".into(), Value::String(iso(t)));
    let n = UUIDS.fetch_add(1, Ordering::Relaxed);
    rec.insert(
        "uuid".into(),
        Value::String(format!("{n:08x}-0000-4000-8000-000000000000")),
    );
    for key in ["sessionId", "session_id"] {
        if rec.contains_key(key) {
            rec.insert(key.into(), Value::String(session.into()));
        }
    }
    if rec.contains_key("cwd") {
        rec.insert("cwd".into(), Value::String(CWD.into()));
    }
    rec
}

pub fn dumps(rec: &Map<String, Value>) -> String {
    serde_json::to_string(rec).expect("serializable")
}

/// A fresh, empty directory for one test, inside cargo's target dir (never
/// the system temp dir).
pub fn tmp_dir(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("agent_session")
        .join(format!(
            "{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test dir");
    dir
}

/// One transcript (a session's main file, or an agent's own file).
pub struct Transcript {
    pub path: PathBuf,
    pub session: String,
    pub lines: Vec<String>,
}

impl Transcript {
    pub fn new(path: PathBuf, session: &str) -> Transcript {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the dir");
        Transcript {
            path,
            session: session.into(),
            lines: Vec::new(),
        }
    }

    pub fn add(&mut self, rec: Map<String, Value>, t: i64) {
        let rec = stamp(rec, t, &self.session.clone());
        self.lines.push(dumps(&rec));
    }

    pub fn raw(&mut self, line: String) {
        self.lines.push(line);
    }

    /// An operator prompt (`origin.kind == "human"`).
    pub fn prompt(&mut self, t: i64, text: &str) {
        let mut rec = tpl("user_prompt_human");
        rec["message"]["content"] = Value::String(text.into());
        self.add(rec, t);
    }

    /// An assistant text reply.
    pub fn text(&mut self, t: i64, text: &str) {
        let mut rec = tpl("assistant_text");
        let content = rec["message"]["content"].as_array().expect("blocks");
        let mut block = content
            .iter()
            .find(|b| b["type"] == "text")
            .expect("a text block")
            .clone();
        block["text"] = Value::String(text.into());
        rec["message"]["content"] = Value::Array(vec![block]);
        self.add(rec, t);
    }

    /// Writes every line so far (the file is rewritten in place: same inode,
    /// so a reader sees an append).
    pub fn write(&self) {
        let mut body = self.lines.join("\n");
        body.push('\n');
        std::fs::write(&self.path, body).expect("write the transcript");
    }
}

/// A Claude Code projects dir with one project (scenario.Project).
pub struct Project {
    pub root: PathBuf,
    pub dir: PathBuf,
}

impl Project {
    pub fn new(root: PathBuf) -> Project {
        let dir = root.join(SLUG);
        std::fs::create_dir_all(&dir).expect("create the project dir");
        Project { root, dir }
    }

    pub fn main(&self, session: &str) -> Transcript {
        Transcript::new(self.dir.join(format!("{session}.jsonl")), session)
    }

    /// An agent's own transcript: `<session>/subagents/agent-<id>.jsonl`.
    pub fn agent(&self, session: &str, agent_id: &str) -> Transcript {
        Transcript::new(
            self.dir
                .join(session)
                .join("subagents")
                .join(format!("agent-{agent_id}.jsonl")),
            session,
        )
    }
}
