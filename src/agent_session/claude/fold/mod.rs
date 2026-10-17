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
//!    flight ([`Session::waiting_bg`]), it is waiting ([`Status::Bg`](crate::agent_session::Status::Bg)); else it
//!    ended silently (done, inferred).
//!
//! **Background work:** a Bash result with `backgroundTaskId` (run in the
//! background, or moved there by its timeout) or a Monitor result (`taskId`
//! with `timeoutMs`) starts a task owned by the file's owner. It ends with any
//! terminal notice naming it (every `<task-id>` of a notice counts), a
//! Monitor's `[Monitor expired` event or its timeout, or a successful
//! `TaskStop` / `KillShell`. Claude Code can also end one without a record (a
//! stop from its UI, the process exiting), so an open task only counts while
//! [`Liveness::task_alive`](crate::agent_procs::Liveness::task_alive) says its process lives, or, when that cannot be
//! checked, for at most [`BG_CAP`] after it started.
//!
//! **Parentage:** `meta.toolUseId` is the `tool_use.id` of the spawning call;
//! the owner of that call (main or another agent's file) is the parent, so
//! depth-2 agents nest under the agent that spawned them.
//!
//! **Hooks:** panorama's hook records (`hooks/<session>.jsonl`, v1) add a
//! recorded stop as evidence and a recorded start as a reopening; an agent
//! that started, never stopped and whose Claude process is gone is
//! [`Status::Lost`](crate::agent_session::Status::Lost).
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
use std::sync::OnceLock;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::Continuation;
use crate::agent_procs::Pid;
use times::{NoticeAt, T};

// The session itself: construction, identity, reset.
mod lifecycle;

// Inputs: what the reader folds in.
mod apply;
mod background;
mod hooks;
mod tool_calls;
mod workflow;

// Derived views.
mod parentage;
mod queries;
mod status;

// Supports: the stores' values, the rules' limits, times and JSON fields.
mod fields;
mod limits;
mod stores;
mod times;

pub use limits::{
    ACTIVE, ASYNC_QUIET, BG_CAP, HOOK_VERSION, MONITOR_SLACK, NESTING_CAP, PENDING_CAP,
    REOPEN_MARGIN,
};
pub use status::StatusOf;
pub use stores::{
    Ack, AgentRec, BgKind, BgLaunch, FileFacts, HookStart, Owner, Spawn, Titles, WorkflowLaunch,
};

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
