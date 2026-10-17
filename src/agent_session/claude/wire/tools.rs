//! Claude Code's tool names: the only place the kit spells them. The fold,
//! the tail summary and the one-line tool summaries dispatch on these.

/// The spawning tools (`Agent`, and `Task` from older Claude Code versions).
pub const SPAWN_TOOLS: [&str; 2] = [AGENT, TASK];

/// The tools that stop a background task (`TaskStop`, and
/// `KillShell` from older Claude Code versions).
pub const BG_STOP_TOOLS: [&str; 2] = [TASK_STOP, KILL_SHELL];

/// Spawns a subagent.
pub(crate) const AGENT: &str = "Agent";
/// Spawns a subagent (older Claude Code versions).
pub(crate) const TASK: &str = "Task";
/// Sends a message to (and may resume) another agent.
pub(crate) const SEND_MESSAGE: &str = "SendMessage";
/// Runs a shell command, possibly in the background.
pub(crate) const BASH: &str = "Bash";
/// Starts a background watch.
pub(crate) const MONITOR: &str = "Monitor";
/// Stops a background task.
pub(crate) const TASK_STOP: &str = "TaskStop";
/// Stops a background shell (older Claude Code versions).
pub(crate) const KILL_SHELL: &str = "KillShell";
/// A subagent hands its final report back to its caller.
pub(crate) const SUBAGENT_HANDBACK: &str = "SubagentHandback";

/// The file tools a summary shows as a path.
pub(crate) const FILE_TOOLS: [&str; 4] = ["Read", "Write", "Edit", "NotebookEdit"];
/// Fetches a URL.
pub(crate) const WEB_FETCH: &str = "WebFetch";
/// The search tools a summary shows as their query or pattern.
pub(crate) const SEARCH_TOOLS: [&str; 4] = ["ToolSearch", "WebSearch", "Grep", "Glob"];

#[cfg(test)]
mod tests;
