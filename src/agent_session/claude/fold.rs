//! The session fold. **Pure:** takes records and the current time, returns
//! data; no IO, no threads, no environment.
//!
//! A port of herdr-panorama's `scan/session.py` (after zoetrope's
//! `src/state/session.rs`, MIT). Records are folded into stores keyed by what
//! they are about (spawning calls, acknowledgements, notices, resumes,
//! workflow journals, background tasks), so the result does not depend on the
//! order files or lines are read; statuses are derived at query time from the
//! stores and the clock ([`Session::status_of`]). [`super::scan::Scanner`]
//! reads a session's files and feeds this fold.
//!
//! # Status rules
//!
//! panorama's `scan/session.py` docstring, pinned by the table tests in
//! `tests/agent_session_claude_fold.rs`:
//!
//! 1. Terminal evidence is, newest first: the latest `<task-notification>`
//!    for the agent from any carrier in any file (`completed` → done,
//!    `failed` → fail, `killed`/`stopped` → cxl), a foreground spawn's
//!    `completed` acknowledgement, a workflow journal `result` (done), the end
//!    of the agent's killed workflow run (cxl).
//! 2. Evidence is overturned by a later `SendMessage` resume
//!    (`resumedAgentId`) or by the agent's own activity more than
//!    [`REOPEN_MARGIN`] after it: the agent is running again.
//! 3. Without standing terminal evidence an agent is running while active:
//!    written within [`ACTIVE`] seconds, or a tool call in flight for at most
//!    [`PENDING_CAP`] (an abandoned call no longer keeps it running forever),
//!    or a background agent that has not been notified yet, within
//!    [`ASYNC_QUIET`]. Otherwise, if background work it started is still in
//!    flight ([`Session::waiting_bg`]), it is waiting ([`Status::Bg`]); else it
//!    ended silently (done, inferred).
//!
//! **Background work:** a Bash result with `backgroundTaskId` (run in the
//! background, or moved there by its timeout) or a Monitor result (`taskId`
//! with `timeoutMs`) starts a task owned by the file's owner. It ends with any
//! terminal notice naming it (every `<task-id>` of a notice counts), a
//! Monitor's `[Monitor expired` event or its timeout, or a successful
//! `TaskStop` / `KillShell`. Claude Code can also end one without a record (a
//! stop from its UI, the process exiting), so an open task only counts while
//! [`Liveness::task_alive`] says its process lives, or, when that cannot be
//! checked, for at most [`BG_CAP`] after it started.
//!
//! **Parentage:** `meta.toolUseId` is the `tool_use.id` of the spawning call;
//! the owner of that call (main or another agent's file) is the parent, so
//! depth-2 agents nest under the agent that spawned them.
//!
//! **Hooks:** panorama's hook records (`hooks/<session>.jsonl`, v1) add a
//! recorded stop as evidence and a recorded start as a reopening; an agent
//! that started, never stopped and whose Claude process is gone is
//! [`Status::Lost`].
//!
//! # Deviations from Python
//!
//! - A field of the wrong JSON type is treated as absent where panorama would
//!   use it anyway (a numeric `toolUseId`, a `resolvedModel` that is not a
//!   string), as in [`super::wire`].
//! - Two notices about one task at the same time and with the same ending,
//!   one with a `<tool_uses>` count and one without, compare without error
//!   (Python's tuple comparison raises there): the counted one is the later.
//! - `apply` cannot fail, so there is no "unreadable records" count.
//! - A hook record's `pid` outside `u32` is no pid.
//! - [`Session::status_of`] looks for a running descendant through at most
//!   [`NESTING_CAP`] levels; Python recurses until `RecursionError` (about
//!   1000 levels), which fails the whole collect.
//! - A workflow journal's `agentId` is compared as its `str()`: a numeric
//!   `agentId: 5` names agent `"5"` (Python keeps the int, which matches no
//!   agent id).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::wire::{
    self, BG_STOP_TOOLS, Block, Ending, Line, Record, SPAWN_TOOLS, TailInfo, py_str, py_strip,
    truthy,
};
use super::{Continuation, SessionId, Ts};
use crate::agent_procs::{Liveness, Pid, ProcessIdentity};
use crate::agent_session::Status;

/// An agent written within this many seconds is running (zoetrope's
/// `INTERACTIVE_IDLE_SECS`).
pub const ACTIVE: f64 = 120.0;
/// A tool call in flight keeps its agent running for at most this long.
pub const PENDING_CAP: f64 = 3600.0;
/// A background agent not notified yet is running for at most this long
/// after its last record.
pub const ASYNC_QUIET: f64 = 600.0;
/// An agent's final line can land a moment after its own notice: activity
/// reopens an agent only when it is this much later.
pub const REOPEN_MARGIN: f64 = 5.0;
/// An open background task nobody can check (another machine, no process
/// table) counts this long after it started.
pub const BG_CAP: f64 = 24.0 * 3600.0;

/// How many levels of descendants [`Session::status_of`] looks through for a
/// running one (Claude Code nests a few; a deeper `parentAgentId` chain
/// written into metas must not exhaust the stack).
pub const NESTING_CAP: usize = 64;
/// A Monitor past its timeout by this much has expired even without its last
/// event.
pub const MONITOR_SLACK: f64 = 60.0;
/// The version of panorama's hook records (`hooks.VERSION`).
pub const HOOK_VERSION: f64 = 1.0;
/// A recorded hook stop and a notice at most this long after it are the same
/// ending.
const STOP_NOTICE_WINDOW: f64 = 60.0;
/// A prompt is kept to this many characters.
const PROMPT_MAX: usize = 200;

/// Whose file a record came from: the session's main transcript or an
/// agent's own transcript (panorama's `MAIN` or the agent id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
    /// The session's main transcript.
    Main,
    /// The transcript of the agent with this id.
    Agent(String),
}

/// A spawning call (`Agent`, `Task`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Spawn {
    /// The call's `tool_use.id`.
    pub call: String,
    /// Whose file holds it (main wins, then the earliest record: a fork's file
    /// repeats its parent's history).
    pub owner: Owner,
    /// Its time.
    pub ts: Option<Ts>,
    /// The `description` input.
    pub description: String,
    /// The `model` input.
    pub model: Option<String>,
    /// The `subagent_type` input.
    pub subagent_type: Option<String>,
    /// `run_in_background` was set.
    pub background: bool,
}

/// A spawning call's result, naming the agent (`toolUseResult.agentId`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Ack {
    /// The spawning call.
    pub call: String,
    /// The result's time.
    pub ts: Option<Ts>,
    /// The agent it started.
    pub agent_id: String,
    /// `async_launched`, `completed`, …
    pub status: String,
    /// The result was an error.
    pub is_error: bool,
    /// `resolvedModel`.
    pub model: Option<String>,
    /// `totalToolUseCount`.
    pub tool_count: Option<i64>,
}

/// A `Workflow` call's result (`toolUseResult.runId`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct WorkflowLaunch {
    /// The call.
    pub call: String,
    /// The result's time.
    pub ts: Option<Ts>,
    /// The run.
    pub run_id: String,
    /// The run's task id: notices about it end the run.
    pub task_id: Option<String>,
    /// `workflowName`.
    pub name: Option<String>,
}

/// What kind of background task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BgKind {
    /// A Bash run in the background (or moved there by its timeout).
    Bash,
    /// A Monitor watch.
    Monitor,
}

impl BgKind {
    /// The tool's name.
    pub fn name(self) -> &'static str {
        match self {
            BgKind::Bash => "Bash",
            BgKind::Monitor => "Monitor",
        }
    }
}

/// A background task's start.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BgLaunch {
    /// The task id.
    pub task: String,
    /// Whose file launched it.
    pub owner: Owner,
    /// The result's time.
    pub ts: Option<Ts>,
    /// Bash or Monitor.
    pub kind: BgKind,
    /// The call that started it.
    pub call: String,
    /// A Monitor's timeout in seconds (unless persistent).
    pub timeout: Option<f64>,
    /// A persistent Monitor never times out.
    pub persistent: bool,
}

/// A hook `start` record of an agent.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HookStart {
    /// When.
    pub ts: Ts,
    /// The Claude Code process that ran it.
    pub pid: Option<Pid>,
    /// That process's start token.
    pub start: Option<String>,
    /// Its pid namespace.
    pub pidns: Option<String>,
}

/// One agent of the session, as its files tell.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct AgentRec {
    /// Its transcript (`agent-<id>.jsonl`).
    pub transcript: PathBuf,
    /// The workflow run it belongs to.
    pub workflow: Option<String>,
    /// Its `agent-<id>.meta.json` object (empty until read).
    pub meta: Map<String, Value>,
}

/// What the reader knows of one transcript beyond its records.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FileFacts {
    /// The stream time: the last record time of the lines read (an undated
    /// record inherits it).
    pub last_ts: Option<Ts>,
    /// The agent's own tool calls (`"type":"tool_use"` in its assistant
    /// records; main's are not counted).
    pub tool_calls: u64,
    /// The summary of its tail ([`super::io::summarize_tail`]).
    pub info: TailInfo,
}

/// The session's titles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Titles {
    /// `custom-title`.
    pub custom: Option<String>,
    /// `agent-name`.
    pub agent: Option<String>,
    /// `ai-title`.
    pub ai: Option<String>,
}

impl From<Ending> for Status {
    fn from(e: Ending) -> Status {
        match e {
            Ending::Done => Status::Done,
            Ending::Failed => Status::Failed,
            Ending::Cancelled => Status::Cancelled,
        }
    }
}

/// [`Session::status_of`]'s answer: panorama's `(status, ended_at, inferred)`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatusOf {
    /// The status.
    pub status: Status,
    /// When it ended (`None` while running).
    pub ended: Option<Ts>,
    /// Guessed from silence, not recorded.
    pub inferred: bool,
}

impl StatusOf {
    fn new(status: Status, ended: Option<f64>, inferred: bool) -> StatusOf {
        let ended = ended.map(Ts);
        StatusOf {
            status,
            ended,
            inferred,
        }
    }
}

/// A time ordered totally, for sets of times.
#[derive(Clone, Copy, Debug, PartialEq)]
struct T(f64);

impl Eq for T {}

impl PartialOrd for T {
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for T {
    fn cmp(&self, other: &T) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// A notice about a task: `(time, ending, tool uses)`, ordered as panorama
/// compares its tuples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NoticeAt(T, Ending, Option<u64>);

/// One Claude Code session folded into keyed stores: panorama's
/// `scan.session.Session` without its file reading ([`super::scan`]).
#[derive(Clone, Debug)]
pub struct Session {
    /// In discovery order, kept across updates (Python's dict).
    agents: IndexMap<String, AgentRec>,
    files: HashMap<Owner, FileFacts>,
    spawns: HashMap<String, Spawn>,
    acks: HashMap<String, Ack>,
    ack_of: HashMap<String, String>,
    notices: HashMap<String, BTreeSet<NoticeAt>>,
    resumes: HashMap<String, BTreeSet<T>>,
    wf_launches: IndexMap<String, WorkflowLaunch>,
    journal_started: HashMap<String, HashMap<String, (String, Option<String>)>>,
    journal_phases: HashMap<String, Vec<String>>,
    journal_done: HashMap<String, HashSet<String>>,
    journal_failed: HashMap<String, HashSet<String>>,
    spawn_results: HashSet<String>,
    spawn_owners: HashMap<String, IndexMap<Owner, f64>>,
    wf_summary: HashMap<String, (Map<String, Value>, f64)>,
    prompts: BTreeSet<(T, String)>,
    titles: Titles,
    continuation: Continuation,
    qids: HashMap<Owner, BTreeSet<u64>>,
    pending_ask: HashMap<String, Owner>,
    hook_starts: HashMap<String, Vec<HookStart>>,
    hook_stops: HashMap<String, Vec<f64>>,
    hook_procs: Vec<(f64, Pid, String)>,
    bg: IndexMap<String, BgLaunch>,
    bg_calls: HashMap<String, (BgKind, String)>,
    bg_stop_calls: HashMap<String, String>,
    bg_stops: HashMap<String, f64>,
    /// [`Session::children`] of every parent, built on first use and dropped
    /// by every input that can change parentage (`apply`, `set_agent`,
    /// `set_meta`, `reset`): panorama's `_children`, cached per update.
    children: OnceLock<HashMap<Owner, Vec<String>>>,
}

/// panorama's `(when or inf)`: no time, or a time of `0`, sorts last.
fn or_inf(t: Option<f64>) -> f64 {
    match t {
        Some(x) if x != 0.0 => x,
        _ => f64::INFINITY,
    }
}

/// Python's `a or b` over optional times (`0.0` is falsy).
fn or_time(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match a {
        Some(x) if x != 0.0 => Some(x),
        _ => b,
    }
}

fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

fn non_empty<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Python's `str(m.get(key) or "")`.
fn str_or_empty(m: &Map<String, Value>, key: &str) -> String {
    match m.get(key) {
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    }
}

/// Python's `isinstance(v, int)` (a bool is an int), as an integer.
fn py_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

/// A JSON number (never a bool) as a float.
fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// Order-independent choice when one spawning call appears in several files:
/// main wins, then the earliest record (the original, not a fork's copy).
fn better_owner(owner: &Owner, when: Option<f64>, old: &Spawn) -> bool {
    if old.owner == Owner::Main {
        return false;
    }
    if *owner == Owner::Main {
        return true;
    }
    or_inf(when) < or_inf(old.ts.map(Ts::secs))
}

impl Session {
    /// An empty fold of session `own`.
    pub fn new(own: SessionId) -> Session {
        Session {
            agents: IndexMap::new(),
            files: HashMap::new(),
            spawns: HashMap::new(),
            acks: HashMap::new(),
            ack_of: HashMap::new(),
            notices: HashMap::new(),
            resumes: HashMap::new(),
            wf_launches: IndexMap::new(),
            journal_started: HashMap::new(),
            journal_phases: HashMap::new(),
            journal_done: HashMap::new(),
            journal_failed: HashMap::new(),
            spawn_results: HashSet::new(),
            spawn_owners: HashMap::new(),
            wf_summary: HashMap::new(),
            prompts: BTreeSet::new(),
            titles: Titles::default(),
            continuation: Continuation::new(own),
            qids: HashMap::new(),
            pending_ask: HashMap::new(),
            hook_starts: HashMap::new(),
            hook_stops: HashMap::new(),
            hook_procs: Vec::new(),
            bg: IndexMap::new(),
            bg_calls: HashMap::new(),
            bg_stop_calls: HashMap::new(),
            bg_stops: HashMap::new(),
            children: OnceLock::new(),
        }
    }

    /// The session's id.
    pub fn id(&self) -> &SessionId {
        self.continuation.own()
    }

    /// Forgets everything folded from records (a file was rewritten): every
    /// store, the hook records, the workflow summaries, the continuation, and
    /// each file's stream time and tool-call count. The agents, their metas
    /// and the tail summaries stay (panorama's `_reset_stores`).
    pub fn reset(&mut self) {
        let agents = std::mem::take(&mut self.agents);
        let mut files = std::mem::take(&mut self.files);
        for f in files.values_mut() {
            f.last_ts = None;
            f.tool_calls = 0;
        }
        let own = self.continuation.own().clone();
        *self = Session::new(own);
        self.agents = agents;
        self.files = files;
    }

    // -- inputs -------------------------------------------------------------------------------

    /// Records an agent file of the session (its transcript, and the workflow
    /// run it is under), keeping a meta already read.
    pub fn set_agent(&mut self, id: &str, transcript: PathBuf, workflow: Option<String>) {
        self.children.take();
        let rec = self.agents.entry(id.to_string()).or_default();
        rec.transcript = transcript;
        rec.workflow = workflow;
    }

    /// Records an agent's `meta.json` object.
    pub fn set_meta(&mut self, id: &str, meta: Map<String, Value>) {
        self.children.take();
        self.agents.entry(id.to_string()).or_default().meta = meta;
    }

    /// The facts of `owner`'s transcript, for the reader to update.
    pub fn file_mut(&mut self, owner: Owner) -> &mut FileFacts {
        self.files.entry(owner).or_default()
    }

    /// Folds lines of the **main** transcript into the continuation rule
    /// ([`Continuation::feed`]): every line, in order.
    pub fn feed_main<L: AsRef<[u8]>>(&mut self, lines: &[L]) {
        self.continuation.feed(lines);
    }

    /// Folds one transcript record from `owner`'s file, at `when` (its own
    /// time, or the stream time for an undated record): panorama's
    /// `Session.apply`. `continued-in` records belong to the continuation
    /// rule ([`Session::feed_main`]) and are ignored here.
    pub fn apply(&mut self, owner: Owner, rec: &Line, when: Option<Ts>) {
        self.children.take();
        let when = when.map(Ts::secs);
        let notes = wire::parse_notices(rec.notice_text().unwrap_or(""));
        if !notes.is_empty() {
            for note in notes {
                if let (Some(status), Some(t)) = (note.status, when) {
                    self.notices.entry(note.task).or_default().insert(NoticeAt(
                        T(t),
                        status,
                        note.tool_uses,
                    ));
                }
            }
            return;
        }
        match rec.record() {
            Record::Title(kind, text) => {
                let slot = match kind {
                    wire::TitleKind::Custom => &mut self.titles.custom,
                    wire::TitleKind::Agent => &mut self.titles.agent,
                    wire::TitleKind::Ai => &mut self.titles.ai,
                };
                *slot = Some(text.to_string());
            }
            Record::Assistant { blocks, .. } => {
                for b in blocks {
                    if let Block::ToolUse {
                        id: Some(id),
                        name,
                        input,
                    } = b
                    {
                        self.tool_use(&owner, id, name.unwrap_or(""), input, when);
                    }
                }
            }
            Record::User {
                tool_use_result, ..
            } => {
                if owner == Owner::Main
                    && rec.is_human_prompt()
                    && let Some(t) = when
                {
                    let text = rec.string_content().unwrap_or("");
                    let first = wire::py_splitlines(text)
                        .into_iter()
                        .map(py_strip)
                        .find(|l| !l.is_empty())
                        .unwrap_or("");
                    self.prompts
                        .insert((T(t), first.chars().take(PROMPT_MAX).collect()));
                }
                let empty = Map::new();
                let tur = tool_use_result.and_then(Value::as_object).unwrap_or(&empty);
                for b in rec.blocks() {
                    if let Block::ToolResult {
                        tool_use_id: Some(call),
                        is_error,
                        content,
                    } = b
                    {
                        self.tool_result(&owner, call, is_error == Some(true), content, tur, when);
                    }
                }
            }
            _ => {}
        }
    }

    fn tool_use(
        &mut self,
        owner: &Owner,
        id: &str,
        name: &str,
        input: Option<&Map<String, Value>>,
        when: Option<f64>,
    ) {
        let empty = Map::new();
        let inp = input.unwrap_or(&empty);
        if SPAWN_TOOLS.contains(&name) {
            let owners = self.spawn_owners.entry(id.to_string()).or_default();
            let at = owners.entry(owner.clone()).or_insert(f64::INFINITY);
            *at = at.min(when.unwrap_or(f64::INFINITY));
            if let Some(old) = self.spawns.get(id)
                && !better_owner(owner, when, old)
            {
                return; // a fork's file repeats its parent's history, spawning calls included
            }
            let background = match inp.get("run_in_background") {
                Some(Value::Bool(true)) => true,
                Some(v) => py_str(v).to_lowercase() == "true",
                None => false,
            };
            let text = |key: &str| inp.get(key).and_then(Value::as_str).map(str::to_string);
            self.spawns.insert(
                id.to_string(),
                Spawn {
                    call: id.to_string(),
                    owner: owner.clone(),
                    ts: when.map(Ts),
                    description: str_or_empty(inp, "description"),
                    model: text("model"),
                    subagent_type: text("subagent_type"),
                    background,
                },
            );
        } else if name.ends_with("ask_operator") {
            self.pending_ask.insert(id.to_string(), owner.clone());
        } else if name == "Bash" || name == "Monitor" {
            let kind = if name == "Bash" {
                BgKind::Bash
            } else {
                BgKind::Monitor
            };
            self.bg_calls
                .insert(id.to_string(), (kind, str_or_empty(inp, "description")));
        } else if BG_STOP_TOOLS.contains(&name) {
            // `task_id or shell_id`: a truthy task_id of another type wins, and is no task
            let task = match inp.get("task_id") {
                Some(v) if truthy(v) => Some(v),
                _ => inp.get("shell_id"),
            };
            if let Some(task) = task.and_then(Value::as_str).filter(|t| !t.is_empty()) {
                self.bg_stop_calls.insert(id.to_string(), task.to_string());
            }
        }
    }

    fn tool_result(
        &mut self,
        owner: &Owner,
        call: &str,
        is_error: bool,
        content: Option<&Value>,
        tur: &Map<String, Value>,
        when: Option<f64>,
    ) {
        let agent_id = tur.get("agentId").filter(|v| truthy(v));
        if self.spawns.contains_key(call) || agent_id.is_some() {
            self.spawn_results.insert(call.to_string());
        }
        if let (Some(agent), Some(status)) = (agent_id, tur.get("status").filter(|v| truthy(v))) {
            let agent = py_str(agent);
            self.acks.insert(
                call.to_string(),
                Ack {
                    call: call.to_string(),
                    ts: when.map(Ts),
                    agent_id: agent.clone(),
                    status: py_str(status),
                    is_error,
                    model: tur
                        .get("resolvedModel")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    tool_count: py_int(tur.get("totalToolUseCount")),
                },
            );
            self.ack_of.insert(agent, call.to_string());
        }
        if let Some(run) = tur.get("runId").filter(|v| truthy(v)) {
            self.wf_launches.insert(
                call.to_string(),
                WorkflowLaunch {
                    call: call.to_string(),
                    ts: when.map(Ts),
                    run_id: py_str(run),
                    task_id: tur
                        .get("taskId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    name: tur.get("workflowName").filter(|v| truthy(v)).map(py_str),
                },
            );
        }
        if let (Some(agent), Some(t)) = (tur.get("resumedAgentId").and_then(Value::as_str), when)
            && tur.get("success") != Some(&Value::Bool(false))
        {
            self.resumes
                .entry(agent.to_string())
                .or_default()
                .insert(T(t));
        }
        self.background(owner, call, tur, is_error, when);
        if let Some(asker) = self.pending_ask.remove(call) {
            self.qids
                .entry(asker)
                .or_default()
                .extend(wire::question_ids(&wire::text_of(content)));
        }
    }

    fn background(
        &mut self,
        owner: &Owner,
        call: &str,
        tur: &Map<String, Value>,
        is_error: bool,
        when: Option<f64>,
    ) {
        let mut launch = None;
        if let Some(task) = non_empty(tur, "backgroundTaskId") {
            launch = Some((task, BgKind::Bash, None, false));
        } else if let Some(task) = non_empty(tur, "taskId")
            && tur.contains_key("timeoutMs")
            && !tur.contains_key("runId")
        {
            let timeout = number(tur.get("timeoutMs"))
                .filter(|ms| *ms > 0.0)
                .map(|ms| ms / 1000.0);
            let persistent = tur.get("persistent") == Some(&Value::Bool(true));
            launch = Some((task, BgKind::Monitor, timeout, persistent));
        }
        if let Some((task, kind, timeout, persistent)) = launch {
            // a fork's file repeats its parent's history, background launches included: main
            // wins, then the earliest record
            let replace = match self.bg.get(task) {
                None => true,
                Some(old) => {
                    old.owner != Owner::Main
                        && (*owner == Owner::Main || or_inf(when) < or_inf(old.ts.map(Ts::secs)))
                }
            };
            if replace {
                self.bg.insert(
                    task.to_string(),
                    BgLaunch {
                        task: task.to_string(),
                        owner: owner.clone(),
                        ts: when.map(Ts),
                        kind,
                        call: call.to_string(),
                        timeout,
                        persistent,
                    },
                );
            }
        }
        let mut stop = self.bg_stop_calls.get(call).cloned();
        if stop.is_none()
            && let Some(task) = tur.get("task_id").and_then(Value::as_str)
            && tur.contains_key("task_type")
        {
            stop = Some(task.to_string()); // TaskStop's own result names the task it stopped
        }
        if let (Some(stop), Some(t)) = (stop.filter(|s| !s.is_empty()), when)
            && !is_error
        {
            let at = self.bg_stops.entry(stop).or_insert(t);
            *at = at.min(t);
        }
    }

    /// Folds one line of workflow run `run`'s `journal.jsonl` (`None`: a line
    /// that did not parse).
    pub fn apply_journal(&mut self, run: &str, rec: Option<&Line>) {
        let Some(rec) = rec else {
            return;
        };
        let Some(aid) = rec.get("agentId").filter(|v| truthy(v)).map(py_str) else {
            return;
        };
        match rec.kind() {
            Some("started") => {
                let phase = rec.get("phase").and_then(Value::as_str).map(str::to_string);
                let label = match rec.get("label") {
                    Some(v) if truthy(v) => py_str(v),
                    _ => aid.clone(),
                };
                self.journal_started
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid, (label, phase.clone()));
                let order = self.journal_phases.entry(run.to_string()).or_default();
                if let Some(p) = phase.filter(|p| !p.is_empty())
                    && !order.contains(&p)
                {
                    order.push(p);
                }
            }
            Some("result") => {
                self.journal_done
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid);
            }
            Some("failed") => {
                self.journal_failed
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid);
            }
            _ => {}
        }
    }

    /// Records workflow run `run`'s summary (`workflows/<run>.json`) and the
    /// file's modification time.
    pub fn set_summary(&mut self, run: &str, summary: Map<String, Value>, mtime: Ts) {
        self.wf_summary.insert(run.to_string(), (summary, mtime.0));
    }

    /// The modification time of run `run`'s summary when it was read.
    pub fn summary_mtime(&self, run: &str) -> Option<Ts> {
        self.wf_summary.get(run).map(|(_, m)| Ts(*m))
    }

    /// Forgets the hook records (the hook file was rewritten).
    pub fn reset_hooks(&mut self) {
        self.hook_starts.clear();
        self.hook_stops.clear();
        self.hook_procs.clear();
    }

    /// Folds one of panorama's hook records (`hooks/<session>.jsonl`, v1). A
    /// record of another version, without an `agent` string or without a
    /// numeric `ts` is skipped.
    pub fn apply_hook(&mut self, rec: &Line) {
        let version_ok = match rec.get("v") {
            Some(Value::Bool(b)) => *b, // Python: True == 1
            Some(v) => v.as_f64() == Some(HOOK_VERSION),
            None => false,
        };
        let (Some(agent), Some(ts)) = (
            rec.get("agent").and_then(Value::as_str),
            number(rec.get("ts")),
        ) else {
            return;
        };
        if !version_ok {
            return;
        }
        let pid = py_int(rec.get("pid")).and_then(|p| Pid::try_from(p).ok());
        let pstart = rec
            .get("pstart")
            .and_then(Value::as_str)
            .map(str::to_string);
        let pidns = rec.get("pidns").and_then(Value::as_str).map(str::to_string);
        if let (Some(p), Some(s)) = (pid, &pstart) {
            self.hook_procs.push((ts, p, s.clone()));
        }
        if agent.is_empty() {
            return;
        }
        match rec.get("event").and_then(Value::as_str) {
            Some("start") => {
                self.hook_starts
                    .entry(agent.to_string())
                    .or_default()
                    .push(HookStart {
                        ts: Ts(ts),
                        pid,
                        start: pstart,
                        pidns,
                    })
            }
            Some("stop") => self
                .hook_stops
                .entry(agent.to_string())
                .or_default()
                .push(ts),
            _ => {}
        }
    }

    // -- queries ------------------------------------------------------------------------------

    /// The agents by id, in the order they were first recorded
    /// ([`Session::set_agent`] or [`Session::set_meta`]), as panorama's
    /// `Session.agents`.
    pub fn agents(&self) -> impl ExactSizeIterator<Item = (&str, &AgentRec)> + '_ {
        self.agents.iter().map(|(id, rec)| (id.as_str(), rec))
    }

    /// Agent `id`, once recorded.
    pub fn agent(&self, id: &str) -> Option<&AgentRec> {
        self.agents.get(id)
    }

    /// The facts of `owner`'s transcript, once it was read.
    pub fn file(&self, owner: &Owner) -> Option<&FileFacts> {
        self.files.get(owner)
    }

    /// The facts of agent `id`'s transcript.
    pub fn file_of(&self, id: &str) -> Option<&FileFacts> {
        self.agents.get(id)?;
        self.files.get(&Owner::Agent(id.to_string()))
    }

    /// The session's titles.
    pub fn titles(&self) -> &Titles {
        &self.titles
    }

    /// The session the conversation continued in (D6), if it did and was not
    /// resumed since. Forward only: nothing here names the sessions that
    /// continued **into** this one.
    pub fn continued_in(&self) -> Option<&SessionId> {
        self.continuation.next()
    }

    /// The human prompts of the main transcript, `(time, first line)`, in
    /// time order (equal ones once).
    pub fn prompts(&self) -> impl Iterator<Item = (Ts, &str)> {
        self.prompts.iter().map(|(t, s)| (Ts(t.0), s.as_str()))
    }

    /// The latest human prompt at or before `when`: its index among
    /// [`Session::prompts`] and its text (zoetrope's `prompt_for_ts`).
    pub fn prompt_before(&self, when: Option<Ts>) -> Option<(usize, &str)> {
        let when = when?.0;
        self.prompts
            .iter()
            .enumerate()
            .take_while(|(_, (t, _))| t.0 <= when)
            .last()
            .map(|(i, (_, s))| (i, s.as_str()))
    }

    /// The notices about `task`: `(time, ending, tool uses)`, oldest first.
    pub fn notices(&self, task: &str) -> Vec<(Ts, Ending, Option<u64>)> {
        self.notices
            .get(task)
            .map(|n| n.iter().map(|x| (Ts(x.0.0), x.1, x.2)).collect())
            .unwrap_or_default()
    }

    /// The times agent `id` was resumed by `SendMessage`, oldest first.
    pub fn resumes(&self, id: &str) -> Vec<Ts> {
        self.resumes
            .get(id)
            .map(|r| r.iter().map(|t| Ts(t.0)).collect())
            .unwrap_or_default()
    }

    /// The inbox question ids `owner`'s `ask_operator` calls recorded.
    pub fn qids(&self, owner: &Owner) -> Option<&BTreeSet<u64>> {
        self.qids.get(owner)
    }

    /// The workflow launches, in the order they were first read.
    pub fn wf_launches(&self) -> impl Iterator<Item = &WorkflowLaunch> {
        self.wf_launches.values()
    }

    /// Workflow run `run`'s summary object, if read.
    pub fn wf_summary(&self, run: &str) -> Option<&Map<String, Value>> {
        self.wf_summary.get(run).map(|(s, _)| s)
    }

    /// The journal's `started` record of agent `id` in run `run`: its label
    /// and phase.
    pub fn journal_started(&self, run: &str, id: &str) -> Option<(&str, Option<&str>)> {
        let (label, phase) = self.journal_started.get(run)?.get(id)?;
        Some((label, phase.as_deref()))
    }

    /// The phases of run `run` in the order its journal first named them.
    pub fn journal_phases(&self, run: &str) -> &[String] {
        self.journal_phases.get(run).map_or(&[], Vec::as_slice)
    }

    fn call_of(&self, id: &str) -> Option<&str> {
        self.agents
            .get(id)
            .and_then(|r| non_empty(&r.meta, "toolUseId"))
            .or_else(|| self.ack_of.get(id).map(String::as_str))
    }

    /// The call that spawned agent `id`.
    pub fn spawn_of(&self, id: &str) -> Option<&Spawn> {
        self.spawns.get(self.call_of(id)?)
    }

    /// The acknowledgement of agent `id`'s spawning call.
    pub fn ack(&self, id: &str) -> Option<&Ack> {
        let call = match self.ack_of.get(id) {
            Some(c) => Some(c.as_str()),
            None => self
                .agents
                .get(id)
                .and_then(|r| non_empty(&r.meta, "toolUseId")),
        };
        self.acks.get(call?)
    }

    /// The agent that spawned agent `id` ([`Owner::Main`] for the session),
    /// via `meta.toolUseId`.
    ///
    /// A fork's own file repeats its history, including the call that
    /// spawned it, so the agent's own copy never counts. Of the other files
    /// holding the call, main wins, then the earliest. If only the agent's own
    /// copy was read (the parent's file can be skipped as old),
    /// `meta.parentAgentId` names the parent.
    pub fn parent_of(&self, id: &str) -> Option<Owner> {
        let call = self.call_of(id);
        let me = Owner::Agent(id.to_string());
        let mut best: Option<(&Owner, f64)> = None;
        if let Some(owners) = call.and_then(|c| self.spawn_owners.get(c)) {
            for (o, t) in owners.iter().filter(|(o, _)| **o != me) {
                if *o == Owner::Main {
                    return Some(Owner::Main);
                }
                if best.is_none_or(|(_, bt)| *t < bt) {
                    best = Some((o, *t));
                }
            }
        }
        if let Some((o, _)) = best {
            return Some(o.clone());
        }
        if let Some(parent) = self
            .agents
            .get(id)
            .and_then(|r| non_empty(&r.meta, "parentAgentId"))
        {
            return Some(Owner::Agent(parent.to_string()));
        }
        call.filter(|c| self.spawns.contains_key(*c))
            .map(|_| Owner::Main)
    }

    /// The agents whose parent is `parent`, by id.
    pub fn children(&self, parent: &Owner) -> Vec<&str> {
        let index = self.children.get_or_init(|| {
            let mut index: HashMap<Owner, Vec<String>> = HashMap::new();
            for id in self.agents.keys() {
                if let Some(p) = self.parent_of(id) {
                    index.entry(p).or_default().push(id.clone());
                }
            }
            index
        });
        index
            .get(parent)
            .map(|c| c.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// The newest time in agent `id`'s transcript (its tail's or its stream's).
    pub fn last_ts(&self, id: &str) -> Option<Ts> {
        let f = self.file_of(id)?;
        max_opt(f.info.last_ts.map(Ts::secs), f.last_ts.map(Ts::secs)).map(Ts)
    }

    /// The newest time in any file read: a prompt, a reply, a tool call, a
    /// notice.
    pub fn last_activity(&self) -> Option<Ts> {
        self.files
            .values()
            .flat_map(|f| [f.info.last_ts.map(Ts::secs), f.last_ts.map(Ts::secs)])
            .flatten()
            .reduce(f64::max)
            .map(Ts)
    }

    /// The `(time, ending)` of the latest notice about the run's latest
    /// launch, if any.
    pub fn run_end(&self, run: &str) -> Option<(Ts, Ending)> {
        let mut launches: Vec<&WorkflowLaunch> = self
            .wf_launches
            .values()
            .filter(|w| w.run_id == run)
            .collect();
        launches.sort_by(|a, b| {
            a.ts.map_or(0.0, Ts::secs)
                .total_cmp(&b.ts.map_or(0.0, Ts::secs))
        });
        let task = launches.last()?.task_id.as_deref().unwrap_or("");
        let last = self.notices.get(task)?.last()?;
        Some((Ts(last.0.0), last.1))
    }

    /// Whether run `run` is still running: `None` when no launch of it is
    /// known (compacted away).
    pub fn run_live(&self, run: &str) -> Option<bool> {
        self.wf_launches
            .values()
            .any(|w| w.run_id == run)
            .then(|| self.run_end(run).is_none())
    }

    /// The description of the call that started a background task.
    pub fn bg_description(&self, launch: &BgLaunch) -> &str {
        self.bg_calls
            .get(&launch.call)
            .map_or("", |(_, d)| d.as_str())
    }

    /// `owner`'s background tasks with no ending in the transcripts, oldest
    /// first.
    pub fn open_bg(&self, owner: &Owner, now: Ts) -> Vec<&BgLaunch> {
        self.open_bg_at(owner, now.0)
    }

    fn open_bg_at(&self, owner: &Owner, now: f64) -> Vec<&BgLaunch> {
        let expired = |l: &BgLaunch| match (l.kind, l.persistent, l.timeout, l.ts) {
            (BgKind::Monitor, false, Some(to), Some(ts)) => {
                to != 0.0 && now > ts.0 + to + MONITOR_SLACK
            }
            _ => false,
        };
        let mut out: Vec<&BgLaunch> = self
            .bg
            .values()
            .filter(|l| l.owner == *owner)
            .filter(|l| !self.notices.contains_key(&l.task) && !self.bg_stops.contains_key(&l.task))
            .filter(|l| !expired(l))
            .collect();
        out.sort_by(|a, b| {
            a.ts.map_or(0.0, Ts::secs)
                .total_cmp(&b.ts.map_or(0.0, Ts::secs))
        });
        out
    }

    /// The open tasks that are still running: their process lives, or, when
    /// that cannot be checked, they started at most [`BG_CAP`] ago. A
    /// Monitor's process is never checked (its timeout decides).
    pub fn waiting_bg(&self, owner: &Owner, now: Ts, live: &dyn Liveness) -> Vec<&BgLaunch> {
        self.waiting_bg_at(owner, now.0, live)
    }

    fn waiting_bg_at(&self, owner: &Owner, now: f64, live: &dyn Liveness) -> Vec<&BgLaunch> {
        self.open_bg_at(owner, now)
            .into_iter()
            .filter(|l| {
                let alive = match l.kind {
                    BgKind::Bash => live.task_alive(&l.task),
                    BgKind::Monitor => None,
                };
                alive.unwrap_or_else(|| l.ts.is_some_and(|ts| now - ts.0 <= BG_CAP))
            })
            .collect()
    }

    /// Only a recorded, recognised Claude process in our pid namespace that
    /// no longer exists counts as gone; anything unknown is not.
    fn process_gone(&self, start: &HookStart, live: &dyn Liveness) -> bool {
        let (Some(pid), Some(token)) = (start.pid, &start.start) else {
            return false;
        };
        let id = ProcessIdentity::new(pid, token.clone(), start.pidns.clone());
        live.process_alive(&id) == Some(false)
    }

    /// A later hook record from another Claude process: the session was
    /// resumed (or restarted) since; an agent's missing stop then says
    /// nothing about how it ended.
    fn resumed_after(&self, start: &HookStart) -> bool {
        self.hook_procs.iter().any(|(t, p, s)| {
            *t > start.ts.0 && (Some(*p), Some(s.as_str())) != (start.pid, start.start.as_deref())
        })
    }

    /// Agent `id`'s status at `now`: the status rules (module docs), with
    /// `live` answering the process checks. An agent that ended its turn
    /// silently but has a running descendant is waiting on it: running.
    pub fn status_of(&self, id: &str, now: Ts, live: &dyn Liveness) -> StatusOf {
        self.status_seen(id, now.0, live, &[])
    }

    fn status_seen(&self, id: &str, now: f64, live: &dyn Liveness, seen: &[&str]) -> StatusOf {
        let status = self.status(id, now, live);
        if status.status == Status::Done
            && status.inferred
            && !seen.contains(&id)
            && seen.len() < NESTING_CAP
        {
            let mut seen = seen.to_vec();
            seen.push(id);
            let running = self
                .children(&Owner::Agent(id.to_string()))
                .into_iter()
                .filter(|c| !seen.contains(c))
                .any(|c| {
                    matches!(
                        self.status_seen(c, now, live, &seen).status,
                        Status::Run | Status::Bg
                    )
                });
            if running {
                return StatusOf::new(Status::Run, None, true);
            }
        }
        status
    }

    fn status(&self, id: &str, now: f64, live: &dyn Liveness) -> StatusOf {
        let rec = self.agents.get(id);
        let f = self.file_of(id);
        let mut last = self.last_ts(id).map(Ts::secs);
        let run = rec.and_then(|r| r.workflow.as_deref());
        let notices: Vec<(f64, Ending)> = self
            .notices
            .get(id)
            .into_iter()
            .flatten()
            .map(|n| (n.0.0, n.1))
            .collect();
        let mut evidence = notices.clone();
        let ack = self.ack(id);
        if let Some(a) = ack
            && a.status == "completed"
            && let Some(t) = a.ts.map(Ts::secs)
        {
            evidence.push((
                t,
                if a.is_error {
                    Ending::Failed
                } else {
                    Ending::Done
                },
            ));
        }
        if let Some(run) = run {
            if self.journal_failed.get(run).is_some_and(|s| s.contains(id)) {
                return StatusOf::new(Status::Failed, last, false); // the run's own ledger: final
            }
            if self.journal_done.get(run).is_some_and(|s| s.contains(id)) {
                return StatusOf::new(Status::Done, last, false);
            }
            if let Some((Ts(t), end)) = self.run_end(run) {
                let end = match end {
                    Ending::Cancelled | Ending::Failed => Ending::Cancelled,
                    Ending::Done => Ending::Done,
                };
                evidence.push((t, end));
            } else if let Some((summary, mtime)) = self.wf_summary.get(run)
                && matches!(
                    summary.get("status").and_then(Value::as_str),
                    Some("killed" | "failed")
                )
                && self.run_live(run).is_none()
            {
                evidence.push((*mtime, Ending::Cancelled));
            }
        }
        for &stop in self.hook_stops.get(id).into_iter().flatten() {
            // a recorded stop confirms an ending; a notice about the same stop says how it ended
            if !notices
                .iter()
                .any(|(t, _)| stop - REOPEN_MARGIN <= *t && *t <= stop + STOP_NOTICE_WINDOW)
            {
                evidence.push((stop, Ending::Done));
            }
        }
        let starts: &[HookStart] = self.hook_starts.get(id).map_or(&[], Vec::as_slice);
        let reopen_times: Vec<f64> = self
            .resumes(id)
            .into_iter()
            .map(Ts::secs)
            .chain(starts.iter().map(|s| s.ts.0))
            .collect();
        let mut reopened = false;
        let newest = evidence
            .iter()
            .copied()
            .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((when, ending)) = newest {
            reopened = reopen_times.iter().any(|r| *r > when)
                || last.is_some_and(|l| l > when + REOPEN_MARGIN);
            if !reopened {
                return StatusOf::new(ending.into(), Some(when), false);
            }
        }
        // the first of the latest starts (Python's `max`)
        let latest_start = starts
            .iter()
            .fold(None::<&HookStart>, |best, s| match best {
                Some(b) if b.ts >= s.ts => Some(b),
                _ => Some(s),
            });
        if let Some(hs) = latest_start
            && !evidence.iter().any(|(t, _)| *t >= hs.ts.0)
            && !self.resumed_after(hs)
            && self.process_gone(hs, live)
        {
            // started, never stopped, its Claude process is gone
            return StatusOf::new(Status::Lost, or_time(last, Some(hs.ts.0)), false);
        }
        if let Some(r) = reopen_times.iter().copied().reduce(f64::max)
            && last.is_none_or(|l| r > l)
        {
            last = Some(r); // a resume restarts the clock before the agent writes anything
        }
        // no standing terminal evidence: liveness
        let Some(last) = last else {
            let started = or_time(
                ack.and_then(|a| a.ts).map(Ts::secs),
                self.spawn_of(id).and_then(|s| s.ts).map(Ts::secs),
            );
            if started.is_some_and(|s| now - s <= ASYNC_QUIET) {
                return StatusOf::new(Status::Run, None, true);
            }
            return StatusOf::new(Status::Done, started, true);
        };
        if let Some(run) = run
            && self.run_live(run) == Some(true)
        {
            let newest_launch = self
                .wf_launches
                .values()
                .filter(|w| w.run_id == run)
                .filter_map(|w| w.ts.map(Ts::secs))
                .reduce(f64::max);
            let first = f.and_then(|f| f.info.first_ts).map(Ts::secs);
            if let (Some(launch), Some(first)) = (newest_launch, first)
                && first < launch - REOPEN_MARGIN
            {
                // started by an earlier instance of a run that was relaunched (resumed): the
                // new instance re-runs unfinished steps with new agents; this one was abandoned
                return StatusOf::new(Status::Cancelled, Some(launch), false);
            }
            if now - last <= PENDING_CAP {
                // the live run still waits on it (the same ground truth as an open spawning call)
                return StatusOf::new(Status::Run, None, false);
            }
        }
        if let Some(sp) = self.spawn_of(id)
            && !sp.background
            && !self.spawn_results.contains(&sp.call)
            && now - sp.ts.map_or(last, |t| t.0.max(last)) <= PENDING_CAP
        {
            // foreground: the parent still waits on the spawning call; that is ground truth
            return StatusOf::new(Status::Run, None, false);
        }
        let pend_ok = f.is_some_and(|f| {
            f.info
                .pending
                .iter()
                .any(|p| p.ts.is_some_and(|t| now - t.secs() <= PENDING_CAP))
        });
        if now - last <= ACTIVE || pend_ok {
            return StatusOf::new(Status::Run, None, false);
        }
        if !self
            .waiting_bg_at(&Owner::Agent(id.to_string()), now, live)
            .is_empty()
        {
            return StatusOf::new(Status::Bg, None, false); // its turn ended, its background work runs
        }
        let meta = rec.map(|r| &r.meta);
        let meta_is = |key: &str, value: &str| {
            meta.and_then(|m| m.get(key)).and_then(Value::as_str) == Some(value)
        };
        if meta_is("agentType", "fork") {
            return StatusOf::new(Status::Done, Some(last), true); // interactive sidechain: idle
        }
        let background = ack.is_some_and(|a| a.status == "async_launched")
            || meta_is("requestShape", "background");
        // panorama: `(background or reopened) and (not evidence or reopened)`
        if (reopened || (background && evidence.is_empty())) && now - last <= ASYNC_QUIET {
            return StatusOf::new(Status::Run, None, true); // not notified yet: still working
        }
        if f.is_some_and(|f| f.info.handback) {
            return StatusOf::new(Status::Done, Some(last), false);
        }
        StatusOf::new(Status::Done, Some(last), true)
    }
}
