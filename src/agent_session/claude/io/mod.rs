//! Transcript IO. **IO.**
//!
//! - [`ProjectsRoot`]: where Claude Code keeps transcripts
//!   (`${CLAUDE_CONFIG_DIR:-~/.claude}/projects`), and the lookup of a
//!   session's main transcript ([`ProjectsRoot::find`]).
//! - [`related`]: a session's subagent transcripts and workflow journals.
//! - [`summarize_tail`]: activity, calls in flight, last text and model from
//!   the end of a transcript.
//! - [`Follower`] and [`Transcripts`]: the continuation rule
//!   ([`super::Continuation`]) applied to files on disk, incrementally, and
//!   forward chains over them ([`super::chain`]).
//!
//! Files are tailed with the harness-neutral [`Tailer`](crate::agent_session::tail::Tailer).
//! Parsing and rules live in the pure modules ([`super::wire`],
//! [`super::continuation`]); this module only reads.

mod discovery;
mod follower;
mod projects;
mod tail_summary;
mod transcripts;

pub use discovery::{
    AgentFile, Related, related, scan_agent_files, scan_workflow_ids, subagents_dir,
    workflow_summary_path,
};
pub use follower::{FOLLOW_CHUNK, Follower};
pub use projects::{ProjectsRoot, sanitize_cwd};
pub use tail_summary::{HEAD_MAX, TAIL_BYTES, TAIL_MAX, summarize_tail, summarize_tail_from};
pub use transcripts::{FOLLOWERS_MAX, Transcripts};
