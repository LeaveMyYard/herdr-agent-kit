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
//! # API
//!
//! - **Inputs** (what a reader folds in): [`Session::apply`],
//!   [`Session::feed_main`], [`Session::set_agent`], [`Session::set_meta`],
//!   [`Session::file_mut`], [`Session::apply_journal`],
//!   [`Session::set_summary`], [`Session::apply_hook`],
//!   [`Session::reset_hooks`], [`Session::reset`].
//! - **Status:** [`Session::status_of`] (and the neutral
//!   [`Session::view`]).
//! - **Agents:** [`Session::agents`], [`Session::agent`],
//!   [`Session::file`], [`Session::file_of`], [`Session::last_ts`],
//!   [`Session::last_activity`].
//! - **Views**, one per concern, each borrowing only what it reads:
//!   [`Session::conversation`] (titles, prompts, continuation),
//!   [`Session::evidence`] (notices, resumes), [`Session::parentage`]
//!   (spawning calls, parents, children), [`Session::workflows`] (runs,
//!   journals, summaries) and [`Session::background`] (background tasks).
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
//!    flight ([`Background::waiting`]), it is waiting ([`Status::Bg`](crate::agent_session::Status::Bg)); else it
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
//! **Hooks:** hook records ([below](#hook-records-format-v1)) add a recorded
//! stop as evidence and a recorded start as a reopening; an agent that
//! started, never stopped and whose Claude process is gone is
//! [`Status::Lost`](crate::agent_session::Status::Lost).
//!
//! # Hook records (format v1)
//!
//! Claude Code's transcripts do not say which process ran an agent, so a
//! plugin can record its `SubagentStart` / `SubagentStop` hooks and
//! `SessionStart` in a JSONL file per session, given to
//! [`Scanner::new`](super::scan::Scanner::new) (`hooks`) and folded with
//! [`Session::apply_hook`]. This is the kit's own versioned format (it began
//! as herdr-panorama's `hooks.py` records, which are version 1), not a
//! Claude Code format. One JSON object per line:
//!
//! | key | type | meaning |
//! |---|---|---|
//! | `v` | number | the format version, [`HOOK_VERSION`]; a record of another version is skipped |
//! | `event` | string | `"start"` (an agent started), `"stop"` (it stopped); any other value (`"session"`) only records its process |
//! | `agent` | string | the agent id without the `agent-` prefix; `""` for a session record. Required |
//! | `ts` | number | epoch seconds. Required |
//! | `pid` | integer | the Claude Code process that ran the hook, when recognised |
//! | `pstart` | string | that process's start token ([`crate::agent_procs::ProcessIdentity`]) |
//! | `pidns` | string | its pid namespace |
//!
//! Other keys (`agent_type`, `source`, …) are ignored. A record with both
//! `pid` and `pstart` names a process: a later record from another process
//! means the session was resumed since, so a missing stop no longer counts
//! as lost. A writer appends; a rewritten file is read again from the start
//! ([`Session::reset_hooks`]). A future incompatible change gets a new
//! version number, and readers skip versions they do not know.
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

use std::collections::HashMap;
use std::sync::OnceLock;

use indexmap::IndexMap;

use stores::{AskStore, BgStore, ConversationStore, HookStore, Notices, SpawnStore, WorkflowStore};

// The session itself: construction, identity, reset.
mod lifecycle;

// Inputs: what the reader folds in, and where each record goes.
mod apply;
mod inputs;
mod tool_calls;

// The keyed stores, one per concern.
mod stores;

// The read side: the agents, the views and the status rules.
mod agents;
mod status;
mod views;

// Supports: owners, the records the stores hold, the rules' limits and
// times.
mod limits;
mod owner;
mod times;
mod values;

pub use limits::{
    ACTIVE, ASYNC_QUIET, BG_CAP, HOOK_VERSION, MONITOR_SLACK, NESTING_CAP, PENDING_CAP,
    REOPEN_MARGIN,
};
pub use owner::Owner;
pub use status::StatusOf;
pub use values::{
    Ack, AgentRec, BgKind, BgLaunch, FileFacts, HookStart, Spawn, Titles, WorkflowLaunch,
};
pub use views::{Background, Conversation, Evidence, Parentage, Workflows};

/// One Claude Code session folded into keyed stores: panorama's
/// `scan.session.Session` without its file reading ([`super::scan`]).
#[derive(Clone, Debug)]
pub struct Session {
    /// In discovery order, kept across updates (Python's dict).
    agents: IndexMap<String, AgentRec>,
    files: HashMap<Owner, FileFacts>,
    notices: Notices,
    conversation: ConversationStore,
    spawns: SpawnStore,
    workflows: WorkflowStore,
    background: BgStore,
    hooks: HookStore,
    asks: AskStore,
    /// [`Parentage::children`] of every parent, built on first use and
    /// dropped by every input that can change parentage (`apply`,
    /// `set_agent`, `set_meta`, `reset`): panorama's `_children`, cached per
    /// update.
    children: OnceLock<HashMap<Owner, Vec<String>>>,
}
