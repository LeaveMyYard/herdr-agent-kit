//! A minimal, test-only port of herdr-panorama's model builder: what
//! `panorama snapshot --json` prints (`collect.py`, `threads.py`, the
//! `events.py` channel reader and `model.py`'s `to_json`), on top of the kit's
//! `agent_session::claude` fold. It exists to prove the fold against panorama's model
//! goldens (plan §5.2 item 3) until `panorama-core::model` is ported; it
//! covers what one `snapshot --json` run does (one collect, this machine, no
//! remote machines, no docking).
//!
//! The structure follows the Python file by file, so a reader can check it
//! side by side; names are the Python ones.

#![allow(dead_code)]

pub mod pyjson;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use herdr_agent_kit::agent_procs::NoProcs;
use herdr_agent_kit::agent_session::claude::Scanner;
use herdr_agent_kit::agent_session::claude::fold::{BgLaunch, Owner, Session};
use herdr_agent_kit::agent_session::claude::io::ProjectsRoot;
use herdr_agent_kit::agent_session::claude::wire::{self, Ending, Line, ToolCall};
use herdr_agent_kit::agent_session::tail::{Tail, Tailer};
use herdr_agent_kit::agent_session::{SessionId, Status, Ts};
use indexmap::IndexMap;
use regex::Regex;
use serde_json::{Map, Value};

use pyjson::J;

pub const DONE_WINDOW: f64 = 3600.0; // config.DEFAULT_DONE_WINDOW
pub const STALE_AFTER: f64 = 12.0 * 3600.0; // config.DEFAULT_STALE_AFTER
const CONTINUE_HOPS: usize = 8;
const QUIET_AFTER: f64 = 30.0 * 60.0;
const EVENTS_VERSION: f64 = 1.0;
const MIN_PREFIX: usize = 4;

// -- model.py --------------------------------------------------------------------------------

const RUN: &str = "run";
const REV: &str = "rev";
const WAIT: &str = "wait";
const BLK: &str = "blk";
const FAIL: &str = "fail";
const DONE: &str = "done";
const CXL: &str = "cxl";
const INF: &str = "inf";
const LOST: &str = "lost";
const BG: &str = "bg";
const ORDER: [&str; 11] = [FAIL, BLK, WAIT, REV, RUN, BG, "q", LOST, INF, DONE, CXL];

fn live(status: &str) -> bool {
    ![DONE, CXL, INF, LOST].contains(&status)
}

fn urgency(status: &str) -> usize {
    ORDER
        .iter()
        .position(|s| *s == status)
        .unwrap_or(ORDER.len())
}

#[derive(Clone, Debug)]
pub struct BgTask {
    id: String,
    kind: &'static str,
    description: String,
    started: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Phase {
    name: String,
    state: String,
    start: Option<f64>,
    end: Option<f64>,
    note: Option<String>,
}

impl Phase {
    fn new(name: &str, state: &str, start: Option<f64>, end: Option<f64>) -> Phase {
        Phase {
            name: name.into(),
            state: state.into(),
            start,
            end,
            note: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    id: String,
    description: String,
    role: String,
    agent_type: J,
    model: Option<String>,
    status: String,
    tool_use_id: J,
    started: Option<f64>,
    ended: Option<f64>,
    last_at: Option<f64>,
    tool_calls: i64,
    activity: String,
    last_message: String,
    questions: Vec<String>,
    pending: Vec<String>,
    question_ids: Vec<i64>,
    error: Option<String>,
    phases: Vec<i64>,
    parent: Option<String>,
    depth: i64,
    resumes: i64,
    inferred: bool,
    transcript: Option<String>,
    seq: i64,
    bg: Vec<BgTask>,
}

impl Agent {
    fn new(id: &str) -> Agent {
        Agent {
            id: id.into(),
            description: String::new(),
            role: "agent".into(),
            agent_type: J::Null,
            model: None,
            status: RUN.into(),
            tool_use_id: J::Null,
            started: None,
            ended: None,
            last_at: None,
            tool_calls: 0,
            activity: String::new(),
            last_message: String::new(),
            questions: Vec::new(),
            pending: Vec::new(),
            question_ids: Vec::new(),
            error: None,
            phases: Vec::new(),
            parent: None,
            depth: 1,
            resumes: 0,
            inferred: false,
            transcript: None,
            seq: 0,
            bg: Vec::new(),
        }
    }
}

/// A thread; its agents are ids into the collect's agent arena.
#[derive(Clone, Debug)]
pub struct Thread {
    id: String,
    name: String,
    source: &'static str,
    phases: Vec<Phase>,
    agents: Vec<String>,
    status: String,
    started: Option<f64>,
    ended: Option<f64>,
    worktree: Option<String>,
    pct: Option<i64>,
    flavour: Option<String>,
}

impl Thread {
    fn new(id: String, name: String, source: &'static str) -> Thread {
        Thread {
            id,
            name,
            source,
            phases: Vec::new(),
            agents: Vec::new(),
            status: String::new(),
            started: None,
            ended: None,
            worktree: None,
            pct: None,
            flavour: None,
        }
    }

    fn done(&self) -> bool {
        self.status == "done" || self.status == "cancelled"
    }
}

/// An orchestrator with its threads and its own agent arena.
pub struct Orchestrator {
    pane_id: String,
    tab_id: J,
    tab_label: String,
    session_id: J,
    herdr_status: J,
    workspace_id: J,
    focused: bool,
    cwd: Option<String>,
    flavour: String,
    transcript: Option<String>,
    pending: Vec<String>,
    inbox: Vec<i64>,
    inbox_texts: Vec<String>,
    threads: Vec<Thread>,
    agents: IndexMap<String, Agent>,
    hidden_done: i64,
    tab_no: Option<i64>,
    bg: Vec<BgTask>,
    last_activity: Option<f64>,
}

/// An open inbox question (panorama `inbox.Question`).
pub struct Question {
    pub id: i64,
    pub session_id: Option<String>,
    pub pane_id: Option<String>,
    pub question: String,
}

// -- small Python helpers --------------------------------------------------------------------

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python's `str()` of a value that is a string or a number.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => pyjson::float_repr(n.as_f64().unwrap_or(0.0)),
        },
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

fn str_of<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

/// Python's `a or b` over optional times.
fn or_time(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match a {
        Some(x) if x != 0.0 => Some(x),
        _ => b,
    }
}

/// A truthy time.
fn truthy_time(t: Option<f64>) -> Option<f64> {
    t.filter(|x| *x != 0.0)
}

fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = iter.peek() {
                    iter.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Python's `int(value)` with a default (`collect._int`).
fn py_int(v: Option<&Value>, default: i64) -> i64 {
    match v {
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| {
                n.as_f64()
                    .filter(|f| f.is_finite())
                    .map(|f| f.trunc() as i64)
            })
            .unwrap_or(default),
        Some(Value::String(s)) => s.trim().replace('_', "").parse().unwrap_or(default),
        _ => default,
    }
}

// -- collect.py ------------------------------------------------------------------------------

fn short_model(model: Option<&str>) -> Option<String> {
    let model = model.filter(|m| !m.is_empty())?;
    let low = model.to_lowercase();
    const FAMILIES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
    let b = low.as_bytes();
    let digits_at = |i: usize, max: usize| -> Vec<usize> {
        // the ends of a run of 1..=max digits at i, longest first (greedy)
        let mut n = 0;
        while n < max && b.get(i + n).is_some_and(u8::is_ascii_digit) {
            n += 1;
        }
        (1..=n).rev().map(|k| i + k).collect()
    };
    let not_digit_at = |i: usize| !b.get(i).is_some_and(u8::is_ascii_digit);
    // _MODEL_AFTER: (opus|sonnet|haiku|fable)[-_ ]?(\d{1,2})(?:[-_.](\d{1,2}))?(?!\d)
    for i in 0..b.len() {
        for fam in FAMILIES {
            if !low[i..].starts_with(fam) {
                continue;
            }
            let after = i + fam.len();
            let seps: Vec<usize> = if b.get(after).is_some_and(|c| b"-_ ".contains(c)) {
                vec![after + 1, after]
            } else {
                vec![after]
            };
            for s in seps {
                for e1 in digits_at(s, 2) {
                    let major = &low[s..e1];
                    if b.get(e1).is_some_and(|c| b"-_.".contains(c)) {
                        for e2 in digits_at(e1 + 1, 2) {
                            if not_digit_at(e2) {
                                return Some(format!("{fam} {major}.{}", &low[e1 + 1..e2]));
                            }
                        }
                    }
                    if not_digit_at(e1) {
                        return Some(format!("{fam} {major}"));
                    }
                }
            }
        }
    }
    // _MODEL_BEFORE: (?<!\d)(\d{1,2})(?:[-_.](\d{1,2}))?[-_ ](opus|sonnet|haiku|fable)
    let fam_at = |i: usize| FAMILIES.into_iter().find(|f| low[i..].starts_with(f));
    for i in 0..b.len() {
        if i > 0 && b[i - 1].is_ascii_digit() {
            continue;
        }
        for e1 in digits_at(i, 2) {
            let major = &low[i..e1];
            if b.get(e1).is_some_and(|c| b"-_.".contains(c)) {
                for e2 in digits_at(e1 + 1, 2) {
                    if b.get(e2).is_some_and(|c| b"-_ ".contains(c))
                        && let Some(fam) = fam_at(e2 + 1)
                    {
                        return Some(format!("{fam} {major}.{}", &low[e1 + 1..e2]));
                    }
                }
            }
            if b.get(e1).is_some_and(|c| b"-_ ".contains(c))
                && let Some(fam) = fam_at(e1 + 1)
            {
                return Some(format!("{fam} {major}"));
            }
        }
    }
    for fam in FAMILIES {
        if low.contains(fam) {
            return Some(fam.into());
        }
    }
    Some(model.into())
}

fn dur(seconds: f64) -> String {
    let m = (seconds.max(0.0) / 60.0).floor() as i64;
    if m >= 60 {
        format!("{}h{:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

fn paragraph(text: &str) -> String {
    const LIMIT: usize = 400;
    let first = text
        .trim_matches(py_isspace)
        .split("\n\n")
        .next()
        .unwrap_or("");
    let para = first
        .split(py_isspace)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if para.chars().count() <= LIMIT {
        para
    } else {
        format!("{}…", chars(&para, LIMIT - 1))
    }
}

fn inbox_mark() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:❓\d+\s+)+").expect("inbox mark"))
}

pub struct Collector {
    projects: ProjectsRoot,
    events_dir: PathBuf,
    hooks_dir: PathBuf,
    now: f64,
    window: f64,
    sessions: HashMap<String, Scanner>,
}

impl Collector {
    pub fn new(projects: &Path, events_dir: &Path, hooks_dir: &Path, now: f64) -> Collector {
        Collector {
            projects: ProjectsRoot::new(projects),
            events_dir: events_dir.into(),
            hooks_dir: hooks_dir.into(),
            now,
            window: DONE_WINDOW,
            sessions: HashMap::new(),
        }
    }

    /// The session's scanner, updated; `None` when no transcript is found.
    fn session(&mut self, session_id: &str, cwd: Option<&str>) -> Option<&Scanner> {
        if !self
            .sessions
            .get(session_id)
            .is_some_and(|s| s.root().is_file())
        {
            let id = SessionId::parse(session_id)?;
            let path = self.projects.find(&id, cwd)?;
            let safe: String = session_id
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let hooks = self.hooks_dir.join(format!("{safe}.jsonl"));
            let scanner = Scanner::new(id, path, cwd.map(str::to_string), Some(hooks));
            self.sessions.insert(session_id.to_string(), scanner);
        }
        let since = self.now - self.window - 600.0;
        let s = self.sessions.get_mut(session_id)?;
        s.update(Some(Ts(since)));
        Some(s)
    }

    /// Every session id on the way (oldest first) and the key of the last
    /// session found: `Collector.live_session`.
    fn live_session(
        &mut self,
        session_id: &str,
        cwd: Option<&str>,
    ) -> (Vec<String>, Option<String>) {
        let mut ids = vec![session_id.to_string()];
        let mut key = self
            .session(session_id, cwd)
            .map(|_| session_id.to_string());
        while let Some(k) = key.clone() {
            let Some(next) = self.sessions[&k]
                .session()
                .conversation()
                .continued_in()
                .map(|s| s.to_string())
            else {
                break;
            };
            if ids.len() > CONTINUE_HOPS || ids.contains(&next) {
                break;
            }
            if self.session(&next, cwd).is_none() {
                break; // not written yet (or another machine's): the last one found
            }
            ids.push(next.clone());
            key = Some(next);
        }
        (ids, key)
    }

    pub fn collect(&mut self, snapshot: Option<&Value>, open_qs: Option<&[Question]>) -> J {
        let now = self.now;
        let mut orchestrators = Vec::new();
        if let Some(snap) = snapshot
            .and_then(Value::as_object)
            .filter(|s| !s.is_empty())
        {
            let mut tabs: Vec<(Value, &Map<String, Value>)> = Vec::new();
            for t in snap
                .get("tabs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(t) = t.as_object() else { continue };
                let tid = t.get("tab_id").cloned().unwrap_or(Value::Null);
                match tabs.iter_mut().find(|(k, _)| *k == tid) {
                    Some(slot) => slot.1 = t,
                    None => tabs.push((tid, t)),
                }
            }
            let order = |tid: &Value| tabs.iter().position(|(k, _)| k == tid).unwrap_or(999);
            let agents: Vec<&Map<String, Value>> = snap
                .get("agents")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
                .filter(|a| str_of(a, "agent") == Some("claude"))
                .collect();
            let tab_of = |a: &Map<String, Value>| a.get("tab_id").cloned().unwrap_or(Value::Null);
            let per_tab: Vec<usize> = agents
                .iter()
                .map(|a| agents.iter().filter(|b| tab_of(b) == tab_of(a)).count())
                .collect();
            let mut indexed: Vec<(usize, &Map<String, Value>)> =
                agents.into_iter().enumerate().collect();
            indexed.sort_by(|(_, a), (_, b)| {
                let ka = (order(&tab_of(a)), str_of(a, "pane_id").unwrap_or(""));
                let kb = (order(&tab_of(b)), str_of(b, "pane_id").unwrap_or(""));
                ka.cmp(&kb)
            });
            let tab_no = |tid: &Value| -> Option<i64> {
                let (_, t) = tabs.iter().find(|(k, _)| k == tid)?;
                let ws = t.get("workspace_id");
                let pos = tabs
                    .iter()
                    .filter(|(_, x)| x.get("workspace_id") == ws)
                    .position(|(k, _)| k == tid)?;
                Some(pos as i64 + 1)
            };
            for (i, a) in indexed {
                let tab = tabs.iter().find(|(k, _)| *k == tab_of(a)).map(|(_, t)| *t);
                let mut o = self.orchestrator(a, tab, per_tab[i], open_qs.unwrap_or(&[]), now);
                o.tab_no = tab_no(&tab_of(a));
                orchestrators.push(o);
            }
        }
        swarm_json(now, snapshot.is_some(), open_qs.is_some(), &orchestrators)
    }

    fn orchestrator(
        &mut self,
        a: &Map<String, Value>,
        tab: Option<&Map<String, Value>>,
        per_tab: usize,
        open_qs: &[Question],
        now: f64,
    ) -> Orchestrator {
        let empty = Map::new();
        let tab = tab.unwrap_or(&empty);
        let sess_info = a
            .get("agent_session")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let kind_id = sess_info.get("kind").is_none_or(|k| k == "id");
        let sid = if kind_id {
            sess_info.get("value").cloned()
        } else {
            None
        };
        let pane_id = match a.get("pane_id") {
            Some(v) if truthy(v) => py_str(v),
            _ => String::new(),
        };
        let mut o = Orchestrator {
            pane_id: pane_id.clone(),
            tab_id: a.get("tab_id").map_or(J::Null, J::from),
            tab_label: String::new(),
            session_id: sid.as_ref().map_or(J::Null, J::from),
            herdr_status: match a.get("agent_status") {
                Some(v) if truthy(v) => J::from(v),
                _ => J::str("unknown"),
            },
            workspace_id: a.get("workspace_id").map_or(J::Null, J::from),
            focused: a.get("focused").is_some_and(truthy),
            cwd: str_of(a, "cwd").map(str::to_string),
            flavour: "agents".into(),
            transcript: None,
            pending: Vec::new(),
            inbox: Vec::new(),
            inbox_texts: Vec::new(),
            threads: Vec::new(),
            agents: IndexMap::new(),
            hidden_done: 0,
            tab_no: None,
            bg: Vec::new(),
            last_activity: None,
        };
        let cwd = str_of(a, "cwd");
        let (ids, key) = match sid.as_ref().filter(|v| truthy(v)) {
            Some(Value::String(s)) => self.live_session(s, cwd),
            Some(other) => (vec![py_str(other)], None),
            None => (Vec::new(), None),
        };
        if let Some(k) = &key {
            o.session_id = J::str(k.clone());
        }
        let mine: Vec<&Question> = open_qs
            .iter()
            .filter(
                |q| match q.session_id.as_deref().filter(|s| !s.is_empty()) {
                    Some(s) => ids.iter().any(|i| i == s),
                    None => q.pane_id.as_deref() == Some(pane_id.as_str()),
                },
            )
            .collect();
        o.inbox = mine.iter().map(|q| q.id).collect();
        o.inbox_texts = mine.iter().map(|q| q.question.clone()).collect();
        let scanner = key.as_ref().map(|k| &self.sessions[k]);
        let titles = scanner
            .map(|s| s.session().conversation().titles().clone())
            .unwrap_or_default();
        let tab_label = inbox_mark()
            .replace(str_of(tab, "label").unwrap_or(""), "")
            .into_owned();
        let mut label = [Some(tab_label), titles.custom, titles.agent, titles.ai]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
            .or_else(|| {
                a.get("terminal_title_stripped")
                    .filter(|v| truthy(v))
                    .map(py_str)
            })
            .unwrap_or_else(|| {
                if pane_id.is_empty() {
                    "?".into()
                } else {
                    pane_id.clone()
                }
            });
        if per_tab > 1 {
            label = format!("{label} · {pane_id}");
        }
        o.tab_label = label;
        let Some(scanner) = scanner else {
            return o;
        };
        let sess = scanner.session();
        o.transcript = Some(scanner.root().to_string_lossy().into_owned());
        o.bg = bg_tasks(
            sess,
            &sess.background().waiting(&Owner::Main, Ts(now), &NoProcs),
        );
        o.last_activity = sess.last_activity().map(Ts::secs);
        if o.herdr_status == J::str("blocked")
            && let Some(main) = sess.file(&Owner::Main)
        {
            o.pending = pending_prompt(&main.info.pending);
        }
        let qtext: HashMap<i64, String> = mine.iter().map(|q| (q.id, q.question.clone())).collect();
        let channel = Channel::read(&events_path(
            &self.events_dir,
            &key.clone().unwrap_or_default(),
        ));
        let (threads, agents, hidden) = threads_of(sess, &channel, &qtext, now, self.window);
        o.threads = threads;
        o.agents = agents;
        o.hidden_done = hidden;
        o.flavour = flavour(&o.threads).into();
        if let Some((card, ag)) = main_session_thread(&o, sess) {
            o.agents.insert(ag.id.clone(), ag);
            o.threads.insert(0, card);
        }
        o
    }
}

fn bg_tasks(sess: &Session, launches: &[&BgLaunch]) -> Vec<BgTask> {
    launches
        .iter()
        .map(|l| BgTask {
            id: l.task.clone(),
            kind: l.kind.name(),
            description: sess.background().description(l).into(),
            started: l.ts.map(Ts::secs),
        })
        .collect()
}

fn pending_prompt(pending: &[ToolCall]) -> Vec<String> {
    let Some(last) = pending.last() else {
        return Vec::new();
    };
    if last.name == "AskUserQuestion" {
        return last
            .input
            .get("questions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(|q| match q.get("question") {
                Some(v) if truthy(v) => py_str(v),
                _ => String::new(),
            })
            .filter(|q| !q.is_empty())
            .collect();
    }
    vec![format!(
        "wants {}",
        wire::summarize_tool(&last.name, Some(&last.input), None)
    )]
}

fn ending_code(e: Ending) -> &'static str {
    match e {
        Ending::Done => DONE,
        Ending::Failed => FAIL,
        Ending::Cancelled => CXL,
        other => panic!("an ending this port does not know: {other:?}"),
    }
}

/// `Collector._agent`: one agent, or `None` when it finished outside the
/// done window and `keep_old` is false.
fn build_agent(
    sess: &Session,
    qtext: &HashMap<i64, String>,
    now: f64,
    window: f64,
    seq: usize,
    aid: &str,
    keep_old: bool,
) -> Option<Agent> {
    let rec = sess.agent(aid).expect("a recorded agent");
    let f = sess.file_of(aid);
    let st = sess.status_of(aid, Ts(now), &NoProcs);
    let status = match st.status {
        Status::Bg => BG,
        Status::Done if st.inferred => INF,
        Status::Lost => LOST,
        other => other.code(),
    };
    let last = sess.last_ts(aid).map(Ts::secs);
    if !keep_old
        && !live(status)
        && or_time(st.ended.map(Ts::secs), last).unwrap_or(0.0) < now - window
    {
        return None;
    }
    let (sp, ack) = (sess.parentage().spawn_of(aid), sess.parentage().ack(aid));
    let meta = &rec.meta;
    let mut ag = Agent::new(aid);
    ag.description = match meta.get("description") {
        Some(v) if truthy(v) => py_str(v),
        _ => sp.map(|s| s.description.clone()).unwrap_or_default(),
    };
    ag.agent_type = match meta.get("agentType") {
        Some(v) if truthy(v) => J::from(v),
        _ => J::opt_str(sp.and_then(|s| s.subagent_type.clone())),
    };
    let model = [
        str_of(meta, "model"),
        ack.and_then(|a| a.model.as_deref()),
        f.and_then(|f| f.info.model.as_deref()),
        sp.and_then(|s| s.model.as_deref()),
    ]
    .into_iter()
    .flatten()
    .find(|m| !m.is_empty());
    ag.model = short_model(model);
    ag.status = status.into();
    ag.tool_use_id = match meta.get("toolUseId") {
        Some(v) if truthy(v) => J::from(v),
        _ => J::opt_str(sp.map(|s| s.call.clone())),
    };
    ag.started = or_time(
        sp.and_then(|s| s.ts).map(Ts::secs),
        f.and_then(|f| f.info.first_ts).map(|t| t.secs()),
    );
    ag.ended = if live(status) {
        None
    } else {
        st.ended.map(Ts::secs)
    };
    ag.last_at = last;
    ag.inferred = st.inferred;
    ag.transcript = Some(rec.transcript.to_string_lossy().into_owned());
    ag.seq = seq as i64;
    ag.resumes = sess.evidence().resumes(aid).len() as i64;
    ag.depth = py_int(meta.get("spawnDepth"), 1);
    // the count of the latest notice that has one (Python: max over (time, status, count))
    let counted = sess
        .evidence()
        .notices(aid)
        .filter(|n| n.2.is_some())
        .max_by(|a, b| {
            a.0.0
                .total_cmp(&b.0.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        })
        .and_then(|n| n.2);
    ag.tool_calls = [
        f.map_or(0, |f| f.tool_calls as i64),
        counted.map_or(0, |c| c as i64),
        ack.and_then(|a| a.tool_count).unwrap_or(0),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    ag.parent = match sess.parentage().parent_of(aid) {
        Some(Owner::Agent(p)) => Some(p),
        _ => None,
    };
    if let Some(f) = f {
        ag.activity = f.info.activity.clone();
        ag.last_message = paragraph(&f.info.last_text);
    }
    ag.question_ids = sess
        .qids(&Owner::Agent(aid.into()))
        .into_iter()
        .flatten()
        .filter_map(|q| i64::try_from(*q).ok())
        .filter(|q| qtext.contains_key(q))
        .collect();
    ag.questions = ag.question_ids.iter().map(|q| qtext[q].clone()).collect();
    if status == FAIL {
        let msg = if ag.last_message.is_empty() {
            "failed"
        } else {
            &ag.last_message
        };
        ag.error = Some(chars(
            py_splitlines(msg).first().copied().unwrap_or(""),
            200,
        ));
    }
    if status == BG {
        ag.bg = bg_tasks(
            sess,
            &sess
                .background()
                .waiting(&Owner::Agent(aid.into()), Ts(now), &NoProcs),
        );
        let n = ag.bg.len();
        ag.activity = format!(
            "turn ended · {n} task{} in background",
            if n != 1 { "s" } else { "" }
        );
    }
    if live(status) && !ag.question_ids.is_empty() {
        ag.status = WAIT.into();
    }
    let label = rec
        .workflow
        .as_deref()
        .and_then(|run| sess.workflows().started(run, aid))
        .map_or("", |(l, _)| l);
    let fallback = match &ag.agent_type {
        J::Str(s) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    };
    ag.role = role_of(&[label, &ag.description], fallback);
    if ag.status == RUN && ag.role == "reviewer" {
        ag.status = REV.into();
    }
    if live(&ag.status)
        && ag.status != BG
        && let Some(l) = truthy_time(last)
        && now - l > QUIET_AFTER
    {
        let rest = if ag.activity.is_empty() {
            String::new()
        } else {
            format!(" · {}", ag.activity)
        };
        ag.activity = format!("quiet {}{rest}", dur(now - l));
    }
    Some(ag)
}

/// `Collector._agents`: the agents shown, by seq.
fn agents_of(
    sess: &Session,
    qtext: &HashMap<i64, String>,
    now: f64,
    window: f64,
) -> IndexMap<String, Agent> {
    let mut out: IndexMap<String, Agent> = IndexMap::new();
    let mut old: HashMap<String, usize> = HashMap::new();
    // collect.py `_agents`: `sorted(sess.agents.items())`
    let mut ids: Vec<&str> = sess.agents().map(|(id, _)| id).collect();
    ids.sort();
    for (seq, aid) in ids.into_iter().enumerate() {
        match build_agent(sess, qtext, now, window, seq, aid, false) {
            Some(ag) => {
                out.insert(aid.to_owned(), ag);
            }
            None => {
                old.insert(aid.to_owned(), seq);
            }
        }
    }
    // an agent outside the window still shows while an agent it spawned does
    let shown: Vec<String> = out.keys().cloned().collect();
    for aid in shown {
        let mut seen: HashSet<String> = HashSet::from([aid.clone()]);
        let mut cur = sess.parentage().parent_of(&aid);
        while let Some(Owner::Agent(c)) = cur {
            if seen.contains(&c) {
                break;
            }
            seen.insert(c.clone());
            if let Some(&seq) = old.get(&c)
                && !out.contains_key(&c)
                && let Some(ag) = build_agent(sess, qtext, now, window, seq, &c, true)
            {
                out.insert(c.clone(), ag);
            }
            cur = sess.parentage().parent_of(&c);
        }
    }
    out.sort_by(|_, a, _, b| a.seq.cmp(&b.seq));
    out
}

/// `(start, end, outcome)` of one launch or resume.
type SpanOf = (Option<f64>, Option<f64>, String);

/// `agent_spans`: `(start, end, outcome)` per launch and per resume.
fn agent_spans(sess: &Session, a: &Agent) -> Vec<SpanOf> {
    let mut starts: Vec<f64> = a
        .started
        .into_iter()
        .chain(sess.evidence().resumes(&a.id).map(Ts::secs))
        .collect();
    starts.sort_by(f64::total_cmp);
    starts.dedup();
    let mut ends: Vec<(f64, Ending)> = sess
        .evidence()
        .notices(&a.id)
        .map(|n| (n.0.0, n.1))
        .collect();
    ends.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let mut out = Vec::new();
    for (i, &s) in starts.iter().enumerate() {
        let nxt = starts.get(i + 1).copied().unwrap_or(f64::INFINITY);
        if let Some((t, st)) = ends.iter().find(|(t, _)| s <= *t && *t < nxt) {
            out.push((Some(s), Some(*t), ending_code(*st).to_string()));
        } else if i + 1 < starts.len() {
            out.push((Some(s), Some(nxt), DONE.to_string()));
        } else if live(&a.status) {
            out.push((Some(s), None, RUN.to_string()));
        } else {
            out.push((Some(s), a.ended, a.status.clone()));
        }
    }
    out
}

fn threads_of(
    sess: &Session,
    channel: &Channel,
    qtext: &HashMap<i64, String>,
    now: f64,
    window: f64,
) -> (Vec<Thread>, IndexMap<String, Agent>, i64) {
    let mut by_id = agents_of(sess, qtext, now, window);
    let mut claimed: HashSet<String> = HashSet::new();
    let mut out: Vec<Thread> = Vec::new();
    for d in &channel.threads {
        out.push(declared_thread(d, &mut by_id, &mut claimed, now));
    }
    let mut runs: Vec<String> = sess
        .agents()
        .filter_map(|(_, r)| r.workflow.clone())
        .chain(sess.workflows().launches().map(|w| w.run_id.clone()))
        .collect();
    runs.sort();
    runs.dedup();
    for run in runs {
        if let Some(t) = workflow_thread(sess, &run, &mut by_id, &mut claimed) {
            out.push(t);
        }
    }
    let top: Vec<String> = by_id
        .values()
        .filter(|a| !claimed.contains(&a.id) && a.parent.is_none())
        .map(|a| a.id.clone())
        .collect();
    let mut turn_of: HashMap<String, i64> = HashMap::new();
    let mut spans: HashMap<String, Vec<SpanOf>> = HashMap::new();
    for id in &top {
        let a = &by_id[id];
        if let Some((i, _)) = sess.conversation().prompt_before(a.started.map(Ts)) {
            turn_of.insert(id.clone(), i as i64);
        }
        spans.insert(id.clone(), agent_spans(sess, a));
    }
    out.extend(group_refs_and_turns(&top, &turn_of, &spans, &mut by_id));
    let mut where_: HashMap<String, usize> = HashMap::new();
    for (i, t) in out.iter().enumerate() {
        for a in &t.agents {
            where_.insert(a.clone(), i);
        }
    }
    let chain = |id: &str, by_id: &IndexMap<String, Agent>| -> Vec<String> {
        let mut c = vec![id.to_string()];
        let mut seen: HashSet<String> = HashSet::from([id.to_string()]);
        while let Some(p) = by_id[c.last().expect("non-empty").as_str()].parent.clone() {
            if !by_id.contains_key(&p) || seen.contains(&p) {
                break;
            }
            seen.insert(p.clone());
            c.push(p);
        }
        c
    };
    // parents first (so a nested agent can take its parent's phase), by the spawn chain
    let mut rest: Vec<(usize, i64, String)> = by_id
        .values()
        .map(|a| (chain(&a.id, &by_id).len(), a.seq, a.id.clone()))
        .collect();
    rest.sort_by_key(|x| (x.0, x.1));
    for (_, _, id) in rest {
        if where_.contains_key(&id) {
            continue;
        }
        let root = chain(&id, &by_id).last().expect("non-empty").clone();
        let ti = match where_.get(&root) {
            Some(&ti) => ti,
            None => {
                let r = &by_id[&root];
                let name = if r.description.is_empty() {
                    r.id.clone()
                } else {
                    r.description.clone()
                };
                out.push(Thread::new(format!("orphan:{root}"), name, "turn"));
                where_.insert(root.clone(), out.len() - 1);
                out.len() - 1
            }
        };
        out[ti].agents.push(id.clone());
        let parent_phase = by_id[&id]
            .parent
            .as_ref()
            .and_then(|p| by_id.get(p))
            .and_then(|p| p.phases.last().copied());
        if let Some(ph) = parent_phase {
            by_id[&id].phases = vec![ph];
        }
        where_.insert(id, ti);
    }
    let mut kept = Vec::new();
    let mut hidden = 0;
    for mut t in out {
        finish(&mut t, &by_id);
        if (t.done() || t.status == "failed") && t.ended.unwrap_or(0.0) < now - window {
            hidden += 1;
            continue;
        }
        kept.push(t);
    }
    (sort_threads(kept, &by_id), by_id, hidden)
}

fn workflow_thread(
    sess: &Session,
    run: &str,
    by_id: &mut IndexMap<String, Agent>,
    claimed: &mut HashSet<String>,
) -> Option<Thread> {
    let members: Vec<&str> = sess
        .agents()
        .filter(|(_, r)| r.workflow.as_deref() == Some(run))
        .map(|(id, _)| id)
        .collect();
    let ags: Vec<String> = members
        .into_iter()
        .filter(|id| by_id.contains_key(*id) && !claimed.contains(*id))
        .map(str::to_owned)
        .collect();
    let live_run = sess.workflows().live(run);
    if ags.is_empty() && live_run != Some(true) {
        return None;
    }
    claimed.extend(ags.iter().cloned());
    let empty = Map::new();
    let summary = sess.workflows().summary(run).unwrap_or(&empty);
    let mut phase_of: IndexMap<String, Option<String>> = IndexMap::new();
    for id in &ags {
        let journal = sess
            .workflows()
            .started(run, id)
            .and_then(|(_, p)| p)
            .filter(|p| !p.is_empty());
        let meta_phase = str_of(
            &sess.agent(id).expect("a recorded agent").meta,
            "workflowPhase",
        );
        phase_of.insert(id.clone(), journal.or(meta_phase).map(str::to_string));
    }
    let summary_phases: Vec<&Map<String, Value>> = summary
        .get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|p| p.get("title").is_some_and(truthy))
        .collect();
    let mut order: Vec<String> = summary_phases.iter().map(|p| py_str(&p["title"])).collect();
    let extra: Vec<String> = sess
        .workflows()
        .phases(run)
        .iter()
        .cloned()
        .chain(
            phase_of
                .values()
                .flatten()
                .filter(|p| !p.is_empty())
                .cloned(),
        )
        .collect();
    for ph in extra {
        if !order.contains(&ph) {
            order.push(ph);
        }
    }
    let detail: HashMap<String, String> = summary_phases
        .iter()
        .map(|p| {
            let d = match p.get("detail") {
                Some(v) if truthy(v) => py_str(v),
                _ => String::new(),
            };
            (py_str(&p["title"]), d)
        })
        .collect();
    let mut launches: Vec<_> = sess
        .workflows()
        .launches()
        .filter(|w| w.run_id == run)
        .collect();
    launches.sort_by(|a, b| {
        a.ts.map_or(0.0, Ts::secs)
            .total_cmp(&b.ts.map_or(0.0, Ts::secs))
    });
    let name = launches
        .iter()
        .rev()
        .find_map(|w| w.name.clone().filter(|n| !n.is_empty()))
        .or_else(|| {
            summary
                .get("workflowName")
                .filter(|v| truthy(v))
                .map(py_str)
        })
        .unwrap_or_else(|| run.to_string());
    let end = sess.workflows().end(run);
    let run_is_live = match live_run {
        Some(l) => l,
        None => ags.iter().any(|id| live(&by_id[id].status)),
    };
    let mut t = Thread::new(format!("wf:{run}"), name, "workflow");
    t.flavour = Some("workflow".into());
    t.phases = workflow_phases(&order, &ags, &phase_of, run_is_live, &detail, by_id);
    t.agents = ags;
    match live_run {
        None => t.status = String::new(),
        Some(true) => t.status = "active".into(),
        Some(false) => {
            t.status = match end {
                Some((_, Ending::Failed)) => "failed",
                Some((_, Ending::Cancelled)) => "cancelled",
                _ => "done",
            }
            .into();
            t.ended = end.map(|e| e.0.0);
        }
    }
    t.started = launches.first().and_then(|w| w.ts).map(Ts::secs);
    Some(t)
}

// -- threads.py ------------------------------------------------------------------------------

const ROLE_WORDS: [(&str, &[&str]); 6] = [
    ("reviewer", &["review", "verif"]),
    ("fixer", &["fix", "cifix"]),
    ("merger", &["merge"]),
    (
        "author",
        &["author", "implement", "build", "write", "draft", "rebase"],
    ),
    (
        "explorer",
        &[
            "explor",
            "research",
            "investigat",
            "find",
            "map",
            "search",
            "survey",
            "read",
            "look",
        ],
    ),
    ("planner", &["plan"]),
];

fn phase_word(role: &str) -> &str {
    match role {
        "reviewer" => "review",
        "fixer" => "fix",
        "author" => "author",
        "explorer" => "explore",
        "merger" => "merge",
        "planner" => "plan",
        other => other,
    }
}

/// `re.search(r"(?<![a-z])" + word, low)`: the first match's start.
fn word_at(low: &str, word: &str) -> Option<usize> {
    low.match_indices(word).map(|(i, _)| i).find(|&i| {
        !low[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_lowercase())
    })
}

fn role_of(texts: &[&str], fallback: Option<&str>) -> String {
    for text in texts {
        let low = text.to_lowercase();
        let mut best: Option<(usize, &str)> = None;
        for (role, words) in ROLE_WORDS {
            for w in words {
                if let Some(at) = word_at(&low, w)
                    && best.is_none_or(|(b, _)| at < b)
                {
                    best = Some((at, role));
                }
            }
        }
        if let Some((_, role)) = best {
            return role.into();
        }
    }
    fallback.unwrap_or("agent").into()
}

fn ref_key(description: &str) -> Option<(String, String)> {
    static R: OnceLock<Regex> = OnceLock::new();
    const STOP: [&str; 35] = [
        "review",
        "re-review",
        "rereview",
        "merge",
        "fix",
        "fixes",
        "rebase",
        "author",
        "for",
        "of",
        "on",
        "in",
        "the",
        "and",
        "to",
        "pr",
        "issue",
        "check",
        "ci",
        "a",
        "an",
        "re",
        "reviewer",
        "close",
        "author-rebase",
        "with",
        "from",
        "after",
        "before",
        "about",
        "is",
        "at",
        "by",
        "into",
        "draft",
    ];
    let re = R.get_or_init(|| {
        Regex::new(r"(?i)(?:\b([A-Za-z][\w.-]*)\s+)?(?:\bPR\s*#?\s*|pull/|#)(\d{1,6})\b")
            .expect("ref")
    });
    let m = re.captures(description)?;
    let num = m.get(2)?.as_str();
    let repo = m.get(1).map(|r| r.as_str()).filter(|r| {
        let low = r.to_lowercase();
        !STOP.contains(&low.as_str()) && !r.chars().all(|c| c.is_ascii_digit())
    });
    Some(match repo {
        Some(r) => (format!("{}#{num}", r.to_lowercase()), format!("{r} #{num}")),
        None => (format!("#{num}"), format!("#{num}")),
    })
}

struct Span {
    agent: String,
    start: Option<f64>,
    end: Option<f64>,
    outcome: String,
}

fn heuristic_phases(mut spans: Vec<Span>, by_id: &mut IndexMap<String, Agent>) -> Vec<Phase> {
    spans.sort_by(|a, b| a.start.unwrap_or(0.0).total_cmp(&b.start.unwrap_or(0.0)));
    let mut phases: Vec<(Phase, Vec<usize>)> = Vec::new();
    let mut roles: Vec<String> = Vec::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (si, sp) in spans.iter().enumerate() {
        let word = phase_word(&by_id[&sp.agent].role).to_string();
        if roles.last() != Some(&word) {
            let n = {
                let c = counts.entry(word.clone()).or_insert(0);
                *c += 1;
                *c
            };
            let name = if word == "review" || word == "fix" {
                format!("{word} r{n}")
            } else if n == 1 {
                word.clone()
            } else {
                format!("{word} {n}")
            };
            phases.push((Phase::new(&name, "now", sp.start, sp.end), vec![si]));
            roles.push(word);
        } else {
            phases.last_mut().expect("a phase").1.push(si);
        }
        let idx = phases.len() as i64 - 1;
        let a = &mut by_id[&sp.agent];
        if !a.phases.contains(&idx) {
            a.phases.push(idx);
        }
    }
    let total = phases.len();
    phases
        .into_iter()
        .enumerate()
        .map(|(i, (mut ph, sps))| {
            let sps: Vec<&Span> = sps.into_iter().map(|k| &spans[k]).collect();
            let running = sps.iter().any(|s| s.end.is_none());
            ph.start = sps
                .iter()
                .filter_map(|s| truthy_time(s.start))
                .reduce(f64::min);
            ph.end = if running {
                None
            } else {
                sps.iter()
                    .filter_map(|s| truthy_time(s.end))
                    .reduce(f64::max)
            };
            let last = sps
                .iter()
                .fold(None::<&&Span>, |best, s| match best {
                    Some(b) if b.end.unwrap_or(0.0) >= s.end.unwrap_or(0.0) => Some(b),
                    _ => Some(s),
                })
                .expect("a span");
            ph.state = if running {
                "now"
            } else if last.outcome == FAIL && i == total - 1 {
                "failed"
            } else if sps.iter().all(|s| s.outcome == CXL) {
                "skipped"
            } else {
                "done"
            }
            .into();
            ph
        })
        .collect()
}

fn thread_status(agents: &[&Agent]) -> &'static str {
    if agents.iter().any(|a| live(&a.status) && a.status != FAIL) {
        return "active";
    }
    if agents.is_empty() {
        return "done";
    }
    let key = |a: &Agent| or_time(a.ended, a.last_at).unwrap_or(0.0);
    let newest = agents
        .iter()
        .fold(None::<&&Agent>, |best, a| match best {
            Some(b) if key(b) >= key(a) => Some(b),
            _ => Some(a),
        })
        .expect("an agent");
    if newest.status == FAIL {
        return "failed";
    }
    if agents.iter().all(|a| a.status == CXL) {
        return "cancelled";
    }
    "done"
}

fn finish(t: &mut Thread, by_id: &IndexMap<String, Agent>) {
    let ags: Vec<&Agent> = t.agents.iter().map(|id| &by_id[id]).collect();
    if t.status.is_empty() {
        t.status = thread_status(&ags).into();
    }
    let started = ags
        .iter()
        .filter_map(|a| truthy_time(a.started))
        .reduce(f64::min);
    t.started = or_time(t.started, started);
    if t.done() || t.status == "failed" {
        let ended = ags
            .iter()
            .filter_map(|a| or_time(a.ended, a.last_at))
            .filter(|e| *e != 0.0)
            .reduce(f64::max);
        t.ended = or_time(t.ended, ended.or(t.started));
    }
    t.agents.sort_by(|a, b| {
        let (a, b) = (&by_id[a], &by_id[b]);
        (urgency(&a.status), a.started.unwrap_or(0.0))
            .partial_cmp(&(urgency(&b.status), b.started.unwrap_or(0.0)))
            .expect("comparable")
    });
}

fn sort_threads(mut threads: Vec<Thread>, by_id: &IndexMap<String, Agent>) -> Vec<Thread> {
    let key = |t: &Thread| {
        let top = t
            .agents
            .iter()
            .map(|a| urgency(&by_id[a].status))
            .min()
            .unwrap_or(99);
        let second = if t.status == "failed" { 0 } else { top };
        let started = t.started.unwrap_or(0.0);
        (t.done(), second, if t.done() { -started } else { started })
    };
    threads.sort_by(|a, b| key(a).partial_cmp(&key(b)).expect("comparable"));
    threads
}

fn group_refs_and_turns(
    top: &[String],
    turn_of: &HashMap<String, i64>,
    spans_of: &HashMap<String, Vec<SpanOf>>,
    by_id: &mut IndexMap<String, Agent>,
) -> Vec<Thread> {
    let mut keyed: IndexMap<String, Vec<String>> = IndexMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut rest: Vec<String> = Vec::new();
    for id in top {
        match ref_key(&by_id[id].description) {
            Some((k, name)) => {
                keyed.entry(k.clone()).or_default().push(id.clone());
                names.entry(k).or_insert(name);
            }
            None => rest.push(id.clone()),
        }
    }
    // a bare "#123" joins the one repo-qualified "<repo>#123" thread, if exactly one exists
    let bare: Vec<String> = keyed
        .keys()
        .filter(|k| k.starts_with('#'))
        .cloned()
        .collect();
    for key in bare {
        let full: Vec<String> = keyed
            .keys()
            .filter(|k| !k.starts_with('#') && k.ends_with(key.as_str()))
            .cloned()
            .collect();
        if full.len() == 1 {
            let moved = keyed.shift_remove(&key).unwrap_or_default();
            keyed[&full[0]].extend(moved);
        }
    }
    let mut threads = Vec::new();
    for (key, ags) in keyed {
        let name = names[&key].clone();
        threads.push(heuristic_thread(
            format!("ref:{key}"),
            name,
            "ref",
            ags,
            spans_of,
            by_id,
        ));
    }
    let mut turns: IndexMap<i64, Vec<String>> = IndexMap::new();
    for id in rest {
        turns
            .entry(turn_of.get(&id).copied().unwrap_or(-1))
            .or_default()
            .push(id);
    }
    for (idx, mut ags) in turns {
        ags.sort_by(|a, b| {
            by_id[a]
                .started
                .unwrap_or(0.0)
                .total_cmp(&by_id[b].started.unwrap_or(0.0))
        });
        let first = &by_id[&ags[0]];
        let mut name = if first.description.is_empty() {
            first.id.clone()
        } else {
            first.description.clone()
        };
        if ags.len() > 1 {
            name += &format!(" +{}", ags.len() - 1);
        }
        threads.push(heuristic_thread(
            format!("turn:{idx}"),
            name,
            "turn",
            ags,
            spans_of,
            by_id,
        ));
    }
    threads
}

fn heuristic_thread(
    tid: String,
    name: String,
    source: &'static str,
    agents: Vec<String>,
    spans_of: &HashMap<String, Vec<SpanOf>>,
    by_id: &mut IndexMap<String, Agent>,
) -> Thread {
    let mut spans = Vec::new();
    for id in &agents {
        let a = &by_id[id];
        let own = spans_of
            .get(id)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                let ended = if live(&a.status) { None } else { a.ended };
                let outcome = if live(&a.status) {
                    RUN.to_string()
                } else {
                    a.status.clone()
                };
                vec![(a.started, ended, outcome)]
            });
        for (start, end, outcome) in own {
            spans.push(Span {
                agent: id.clone(),
                start,
                end,
                outcome,
            });
        }
    }
    let mut t = Thread::new(tid, name, source);
    t.phases = heuristic_phases(spans, by_id);
    t.agents = agents;
    t
}

fn workflow_phases(
    order: &[String],
    agents: &[String],
    phase_of: &IndexMap<String, Option<String>>,
    run_live: bool,
    detail: &HashMap<String, String>,
    by_id: &mut IndexMap<String, Agent>,
) -> Vec<Phase> {
    let mut phases = Vec::new();
    for (i, title) in order.iter().enumerate() {
        let ids: Vec<&String> = agents
            .iter()
            .filter(|id| phase_of.get(*id).and_then(Option::as_deref) == Some(title.as_str()))
            .collect();
        for id in &ids {
            let a = &mut by_id[*id];
            if !a.phases.contains(&(i as i64)) {
                a.phases.push(i as i64);
            }
        }
        let ags: Vec<&Agent> = ids.iter().map(|id| &by_id[*id]).collect();
        let starts = ags
            .iter()
            .filter_map(|a| truthy_time(a.started))
            .reduce(f64::min);
        let ends = ags
            .iter()
            .filter_map(|a| or_time(a.ended, a.last_at))
            .filter(|e| *e != 0.0)
            .reduce(f64::max);
        let state = if ags.is_empty() {
            if run_live { "next" } else { "skipped" }
        } else if ags.iter().any(|a| live(&a.status)) {
            "now"
        } else {
            let newest = ags
                .iter()
                .fold(None::<&&Agent>, |best, a| match best {
                    Some(b) if b.ended.unwrap_or(0.0) >= a.ended.unwrap_or(0.0) => Some(b),
                    _ => Some(a),
                })
                .expect("an agent");
            if newest.status == FAIL {
                "failed"
            } else {
                "done"
            }
        };
        let mut ph = Phase::new(
            title,
            state,
            starts,
            if state == "now" { None } else { ends },
        );
        ph.note = detail.get(title).filter(|d| !d.is_empty()).cloned();
        phases.push(ph);
    }
    phases
}

fn flavour(threads: &[Thread]) -> &str {
    for t in threads {
        if t.source == "explicit"
            && let Some(f) = t.flavour.as_deref().filter(|f| !f.is_empty())
        {
            return f;
        }
    }
    if threads.iter().any(|t| t.source == "workflow") {
        return "workflow";
    }
    if threads.iter().any(|t| t.source == "ref") {
        return "pr-review";
    }
    "agents"
}

fn main_session_thread(o: &Orchestrator, sess: &Session) -> Option<(Thread, Agent)> {
    let attributed: HashSet<i64> = o
        .threads
        .iter()
        .flat_map(|t| t.agents.iter())
        .flat_map(|id| o.agents[id].question_ids.iter().copied())
        .collect();
    let loose: Vec<(i64, String)> = o
        .inbox
        .iter()
        .zip(&o.inbox_texts)
        .filter(|(q, _)| !attributed.contains(q))
        .map(|(q, t)| (*q, t.clone()))
        .collect();
    let blocked = o.herdr_status == J::str("blocked");
    if !blocked && loose.is_empty() {
        return None;
    }
    let since = sess
        .conversation()
        .prompts()
        .map(|(t, _)| t.0)
        .reduce(f64::max);
    let id = format!("main:{}", o.pane_id);
    let mut ag = Agent::new(&id);
    ag.description = "main session".into();
    ag.role = "orchestrator".into();
    ag.status = if blocked { BLK } else { WAIT }.into();
    ag.started = since;
    ag.phases = vec![0];
    ag.question_ids = loose.iter().map(|(q, _)| *q).collect();
    ag.questions = loose.into_iter().map(|(_, t)| t).collect();
    if blocked {
        ag.activity = "waiting for you in the pane".into();
        ag.pending = o.pending.clone();
    }
    let mut t = Thread::new(id.clone(), o.tab_label.clone(), "session");
    t.phases = vec![Phase::new("waiting on you", "now", since, None)];
    t.agents = vec![id];
    t.status = "active".into();
    t.started = since;
    Some((t, ag))
}

// -- events.py -------------------------------------------------------------------------------

struct Declared {
    thread: String,
    name: Option<String>,
    flavour: Option<String>,
    status: Option<String>,
    worktree: Option<String>,
    pct: Option<i64>,
    phases: Vec<Map<String, Value>>,
    agents: Vec<Map<String, Value>>,
    ts: Option<f64>,
}

struct Channel {
    threads: Vec<Declared>,
}

fn events_path(dir: &Path, session_id: &str) -> PathBuf {
    let safe = match SessionId::parse(session_id) {
        Some(id) => id.to_string(),
        None => session_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect(),
    };
    dir.join(format!("{safe}.jsonl"))
}

impl Channel {
    fn read(path: &Path) -> Channel {
        let mut ch = Channel {
            threads: Vec::new(),
        };
        if let Tail::Lines(lines) = Tailer::new().read(path) {
            for raw in lines {
                if let Some(rec) = Line::parse(&raw) {
                    ch.apply(rec.map());
                }
            }
        }
        ch
    }

    fn apply(&mut self, rec: &Map<String, Value>) {
        let version_ok = match rec.get("v") {
            Some(Value::Bool(b)) => *b,
            Some(v) => v.as_f64() == Some(EVENTS_VERSION),
            None => false,
        };
        let Some(tid) = str_of(rec, "thread").filter(|t| !t.is_empty()) else {
            return;
        };
        if !version_ok {
            return;
        }
        let i = match self.threads.iter().position(|d| d.thread == tid) {
            Some(i) => i,
            None => {
                self.threads.push(Declared {
                    thread: tid.into(),
                    name: None,
                    flavour: None,
                    status: None,
                    worktree: None,
                    pct: None,
                    phases: Vec::new(),
                    agents: Vec::new(),
                    ts: None,
                });
                self.threads.len() - 1
            }
        };
        let d = &mut self.threads[i];
        let text = |k: &str| str_of(rec, k).map(str::to_string);
        if let Some(v) = text("name") {
            d.name = Some(v);
        }
        if let Some(v) = text("flavour") {
            d.flavour = Some(v);
        }
        if let Some(v) = text("worktree") {
            d.worktree = Some(v);
        }
        if let Some(s) =
            str_of(rec, "status").filter(|s| ["active", "done", "failed", "cancelled"].contains(s))
        {
            d.status = Some(s.into());
        }
        if let Some(p) = rec
            .get("pct")
            .and_then(Value::as_f64)
            .filter(|p| p.is_finite())
        {
            d.pct = Some((p.trunc() as i64).clamp(0, 100));
        }
        if let Some(phases) = rec.get("phases").and_then(Value::as_array) {
            d.phases = phases
                .iter()
                .filter_map(Value::as_object)
                .filter(|p| p.get("name").is_some_and(Value::is_string))
                .cloned()
                .collect();
        }
        if let Some(agents) = rec.get("agents").and_then(Value::as_array) {
            d.agents = agents
                .iter()
                .filter_map(Value::as_object)
                .filter(|a| a.get("match").is_some_and(Value::is_string))
                .cloned()
                .collect();
        }
        d.ts = or_time(rec.get("ts").and_then(wire::ts).map(|t| t.secs()), d.ts);
    }
}

fn matches(pattern: &str, agent_id: &str, description: &str) -> bool {
    if pattern == agent_id {
        return true;
    }
    if pattern.ends_with('*') {
        let prefix = pattern.trim_end_matches('*');
        return prefix.chars().count() >= MIN_PREFIX && description.starts_with(prefix);
    }
    description == pattern
}

/// `HH:MM` (on the reference day; the goldens run with TZ=UTC) or ISO-8601.
fn phase_time(value: Option<&Value>, reference: Option<f64>, now: f64) -> Option<f64> {
    let value = value.and_then(Value::as_str).filter(|v| !v.is_empty())?;
    if value.chars().count() <= 5 && value.contains(':') {
        let parts: Vec<&str> = value.split(':').collect();
        let [hh, mm] = parts[..] else { return None };
        let (hh, mm): (i64, i64) = (hh.trim().parse().ok()?, mm.trim().parse().ok()?);
        let base = or_time(reference, Some(now)).unwrap_or(now);
        let day = (base / 86_400.0).floor() * 86_400.0;
        return Some(day + (hh * 3600 + mm * 60) as f64);
    }
    wire::ts(&Value::String(value.into())).map(|t| t.secs())
}

fn declared_thread(
    d: &Declared,
    by_id: &mut IndexMap<String, Agent>,
    claimed: &mut HashSet<String>,
    now: f64,
) -> Thread {
    let phases: Vec<Phase> = d
        .phases
        .iter()
        .map(|p| {
            let state = str_of(p, "state")
                .filter(|s| ["done", "now", "next", "failed", "skipped"].contains(s))
                .unwrap_or("next");
            let mut ph = Phase::new(
                str_of(p, "name").unwrap_or(""),
                state,
                phase_time(p.get("start"), d.ts, now),
                phase_time(p.get("end"), d.ts, now),
            );
            ph.note = p.get("note").filter(|n| truthy(n)).map(py_str);
            ph
        })
        .collect();
    let mut ags = Vec::new();
    for spec in &d.agents {
        let pattern = str_of(spec, "match").unwrap_or("");
        let ids: Vec<String> = by_id.keys().cloned().collect();
        for id in ids {
            if claimed.contains(&id) || !matches(pattern, &id, &by_id[&id].description) {
                continue;
            }
            claimed.insert(id.clone());
            let a = &mut by_id[&id];
            if let Some(role) = str_of(spec, "role") {
                a.role = role.into();
            }
            if let Some(ph) = spec.get("phase").and_then(Value::as_i64)
                && !spec.get("phase").is_some_and(Value::is_boolean)
                && 0 <= ph
                && (ph as usize) < phases.len()
            {
                a.phases = vec![ph];
            }
            ags.push(id);
        }
    }
    let status = d
        .status
        .clone()
        .filter(|s| ["active", "done", "failed", "cancelled"].contains(&s.as_str()))
        .unwrap_or_default();
    let mut t = Thread::new(
        format!("x:{}", d.thread),
        d.name.clone().unwrap_or_else(|| d.thread.clone()),
        "explicit",
    );
    t.phases = phases;
    t.agents = ags;
    t.status = status;
    t.worktree = d.worktree.clone();
    t.pct = d.pct;
    t.flavour = d.flavour.clone();
    t
}

// -- to_json ---------------------------------------------------------------------------------

fn bg_json(bg: &[BgTask]) -> J {
    J::Arr(
        bg.iter()
            .map(|b| {
                J::obj([
                    ("id", J::str(&b.id)),
                    ("kind", J::str(b.kind)),
                    ("description", J::str(&b.description)),
                    ("started", J::opt_float(b.started)),
                ])
            })
            .collect(),
    )
}

fn agent_json(a: &Agent) -> J {
    J::obj([
        ("id", J::str(&a.id)),
        ("description", J::str(&a.description)),
        ("role", J::str(&a.role)),
        ("agent_type", a.agent_type.clone()),
        ("model", J::opt_str(a.model.clone())),
        ("status", J::str(&a.status)),
        ("tool_use_id", a.tool_use_id.clone()),
        ("started", J::opt_float(a.started)),
        ("ended", J::opt_float(a.ended)),
        ("last_at", J::opt_float(a.last_at)),
        ("tool_calls", J::Int(a.tool_calls)),
        ("activity", J::str(&a.activity)),
        ("last_message", J::str(&a.last_message)),
        (
            "questions",
            J::Arr(a.questions.iter().map(J::str).collect()),
        ),
        ("pending", J::Arr(a.pending.iter().map(J::str).collect())),
        ("question_ids", J::ints(a.question_ids.iter().copied())),
        ("error", J::opt_str(a.error.clone())),
        ("phases", J::ints(a.phases.iter().copied())),
        ("parent", J::opt_str(a.parent.clone())),
        ("depth", J::Int(a.depth)),
        ("resumes", J::Int(a.resumes)),
        ("inferred", J::Bool(a.inferred)),
        ("transcript", J::opt_str(a.transcript.clone())),
        ("seq", J::Int(a.seq)),
        ("bg", bg_json(&a.bg)),
    ])
}

fn thread_json(t: &Thread, agents: &IndexMap<String, Agent>) -> J {
    J::obj([
        ("id", J::str(&t.id)),
        ("name", J::str(&t.name)),
        ("source", J::str(t.source)),
        (
            "phases",
            J::Arr(
                t.phases
                    .iter()
                    .map(|p| {
                        J::obj([
                            ("name", J::str(&p.name)),
                            ("state", J::str(&p.state)),
                            ("start", J::opt_float(p.start)),
                            ("end", J::opt_float(p.end)),
                            ("note", J::opt_str(p.note.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "agents",
            J::Arr(t.agents.iter().map(|id| agent_json(&agents[id])).collect()),
        ),
        ("status", J::str(&t.status)),
        ("started", J::opt_float(t.started)),
        ("ended", J::opt_float(t.ended)),
        ("worktree", J::opt_str(t.worktree.clone())),
        ("pct", t.pct.map_or(J::Null, J::Int)),
        ("flavour", J::opt_str(t.flavour.clone())),
    ])
}

fn orchestrator_json(o: &Orchestrator) -> J {
    J::obj([
        ("pane_id", J::str(&o.pane_id)),
        ("tab_id", o.tab_id.clone()),
        ("tab_label", J::str(&o.tab_label)),
        ("session_id", o.session_id.clone()),
        ("herdr_status", o.herdr_status.clone()),
        ("workspace_id", o.workspace_id.clone()),
        ("focused", J::Bool(o.focused)),
        ("cwd", J::opt_str(o.cwd.clone())),
        ("flavour", J::str(&o.flavour)),
        ("transcript", J::opt_str(o.transcript.clone())),
        ("pending", J::Arr(o.pending.iter().map(J::str).collect())),
        ("inbox", J::ints(o.inbox.iter().copied())),
        (
            "inbox_texts",
            J::Arr(o.inbox_texts.iter().map(J::str).collect()),
        ),
        (
            "threads",
            J::Arr(
                o.threads
                    .iter()
                    .map(|t| thread_json(t, &o.agents))
                    .collect(),
            ),
        ),
        ("hidden_done", J::Int(o.hidden_done)),
        ("tab_no", o.tab_no.map_or(J::Null, J::Int)),
        ("machine", J::Null),
        ("machine_id", J::Null),
        ("bg", bg_json(&o.bg)),
        ("last_activity", J::opt_float(o.last_activity)),
    ])
}

/// `Swarm.to_json`.
fn swarm_json(now: f64, herdr_ok: bool, inbox_ok: bool, orchestrators: &[Orchestrator]) -> J {
    let agents: Vec<&Agent> = orchestrators
        .iter()
        .flat_map(|o| {
            o.threads
                .iter()
                .flat_map(|t| t.agents.iter().map(|id| &o.agents[id]))
        })
        .collect();
    let count = |f: &dyn Fn(&&Agent) -> bool| agents.iter().filter(|a| f(a)).count() as i64;
    let counts = J::obj([
        ("orchestrators", J::Int(orchestrators.len() as i64)),
        (
            "threads",
            J::Int(orchestrators.iter().map(|o| o.threads.len() as i64).sum()),
        ),
        (
            "active_threads",
            J::Int(
                orchestrators
                    .iter()
                    .flat_map(|o| &o.threads)
                    .filter(|t| !t.done())
                    .count() as i64,
            ),
        ),
        ("agents", J::Int(agents.len() as i64)),
        (
            "running",
            J::Int(count(&|a| a.status == RUN || a.status == REV)),
        ),
        (
            "waiting",
            J::Int(
                orchestrators.iter().filter(|o| !o.bg.is_empty()).count() as i64
                    + count(&|a| a.status == BG),
            ),
        ),
        (
            "questions",
            J::Int(orchestrators.iter().map(|o| o.inbox.len() as i64).sum()),
        ),
        (
            "blocked",
            J::Int(
                orchestrators
                    .iter()
                    .filter(|o| o.herdr_status == J::str("blocked"))
                    .count() as i64,
            ),
        ),
        ("failed", J::Int(count(&|a| a.status == FAIL))),
    ]);
    J::obj([
        ("generated_at", J::Float(now)),
        ("herdr_ok", J::Bool(herdr_ok)),
        ("inbox_ok", J::Bool(inbox_ok)),
        ("errors", J::Arr(Vec::new())),
        ("counts", counts),
        (
            "orchestrators",
            J::Arr(orchestrators.iter().map(orchestrator_json).collect()),
        ),
    ])
}
