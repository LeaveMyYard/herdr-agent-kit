//! The tool names the fold and the summaries dispatch on.

/// The spawning tools (`Agent`, and `Task` from older Claude Code versions).
pub const SPAWN_TOOLS: [&str; 2] = ["Agent", "Task"];

/// The tools that stop a background task (`TaskStop`, and
/// `KillShell` from older Claude Code versions).
pub const BG_STOP_TOOLS: [&str; 2] = ["TaskStop", "KillShell"];
