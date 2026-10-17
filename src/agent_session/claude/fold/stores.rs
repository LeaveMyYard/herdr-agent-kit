//! The values the session's keyed stores hold, and what the reader knows
//! of each file.

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::agent_procs::Pid;
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::TailInfo;

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
    /// The summary of its tail ([`super::io::summarize_tail`](crate::agent_session::claude::io::summarize_tail)).
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
