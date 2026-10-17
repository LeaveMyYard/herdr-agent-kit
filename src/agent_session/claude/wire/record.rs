//! [`Record`]: what one transcript line says, and its content blocks.

use serde_json::{Map, Value};

use super::{py_str, truthy};
use crate::agent_session::SessionId;

/// Record types Claude Code writes as bookkeeping, captured from real
/// transcripts. They never move a conversation: the continuation rule does not
/// depend on this list (only `user` and `assistant` records clear a
/// continuation), so a type Claude Code adds tomorrow is just as harmless.
pub const BOOKKEEPING: [&str; 4] = ["last-prompt", "cost-state", "mode", "file-history-snapshot"];

/// What one transcript line says, borrowed from its [`Line`](super::Line). The time is
/// [`Line::ts`](super::Line::ts).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Record<'a> {
    /// A model turn: its model and content blocks.
    Assistant {
        /// `message.model`, when a string.
        model: Option<&'a str>,
        /// The object blocks of `message.content`.
        blocks: Vec<Block<'a>>,
    },
    /// A user record: a typed prompt, a notification, or tool results.
    User {
        /// `message.content`.
        content: Content<'a>,
        /// `toolUseResult`: an object, or a plain string for some errors.
        tool_use_result: Option<&'a Value>,
    },
    /// A `queue-operation` `enqueue`: a prompt queued mid-turn.
    QueueEnqueue {
        /// `content`, when a string.
        content: Option<&'a str>,
    },
    /// An `attachment` record.
    Attachment(Attachment<'a>),
    /// A `continued-in` record: the conversation moved on to another session
    /// id. **Raw**: whether it counts is the continuation rule's decision
    /// ([`super::Continuation`](crate::agent_session::claude::Continuation)).
    ContinuedIn {
        /// `sessionId`, when it is a session id.
        from: Option<SessionId>,
        /// `continuedInSessionId`, when it is a session id.
        to: Option<SessionId>,
        /// `isSidechain: true`.
        sidechain: bool,
    },
    /// A `custom-title`, `agent-name` or `ai-title` record with a string value.
    Title(TitleKind, &'a str),
    /// A known bookkeeping record ([`BOOKKEEPING`]), by type.
    Bookkeeping(&'a str),
    /// Anything else, including types Claude Code adds later.
    Other,
}

/// The three title records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TitleKind {
    /// `custom-title` (`customTitle`): set by the user.
    Custom,
    /// `agent-name` (`agentName`).
    Agent,
    /// `ai-title` (`aiTitle`): generated.
    Ai,
}

/// `message.content` of a user record.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Content<'a> {
    /// A string (a typed prompt, a notification, a local command).
    Text(&'a str),
    /// A list: its object blocks.
    Blocks(Vec<Block<'a>>),
    /// Missing or another shape.
    None,
}

/// The `attachment` of an attachment record.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Attachment<'a> {
    /// `queued_command`: a prompt typed while the model was busy.
    QueuedCommand {
        /// `prompt`, when a string.
        prompt: Option<&'a str>,
    },
    /// `hook_additional_context`: text a hook added (`content`, its strings).
    HookAdditionalContext {
        /// The strings of `content` (a list, or one string).
        content: Vec<&'a str>,
    },
    /// Another attachment type (or none).
    Other(Option<&'a str>),
}

/// One object block of `message.content`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Block<'a> {
    /// `text`.
    Text(Option<&'a str>),
    /// `tool_use`: a tool call.
    ToolUse {
        /// `id`, when a non-empty string.
        id: Option<&'a str>,
        /// `name`, when a non-empty string.
        name: Option<&'a str>,
        /// `input`, when an object.
        input: Option<&'a Map<String, Value>>,
    },
    /// `tool_result`: the result of a call.
    ToolResult {
        /// `tool_use_id`, when a non-empty string.
        tool_use_id: Option<&'a str>,
        /// `is_error`, when a boolean.
        is_error: Option<bool>,
        /// `content`: a string or a list of blocks.
        content: Option<&'a Value>,
    },
    /// Another block type, by type.
    Other(Option<&'a str>),
}

fn non_empty_str<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

impl<'a> Block<'a> {
    pub(super) fn of(b: &'a Map<String, Value>) -> Block<'a> {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => Block::Text(b.get("text").and_then(Value::as_str)),
            Some("tool_use") => Block::ToolUse {
                id: non_empty_str(b, "id"),
                name: non_empty_str(b, "name"),
                input: b.get("input").and_then(Value::as_object),
            },
            Some("tool_result") => Block::ToolResult {
                tool_use_id: non_empty_str(b, "tool_use_id"),
                is_error: b.get("is_error").and_then(Value::as_bool),
                content: b.get("content"),
            },
            other => Block::Other(other),
        }
    }
}

/// The text of a `tool_result`'s `content`: a string as it is, a list of
/// blocks as their `text`s joined by spaces (panorama's `wire.text_of`). A
/// `text` that is not a string reads as Python's `str()` of it; anything else is no
/// text.
pub fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_object)
            .map(|b| match b.get("text") {
                Some(t) if truthy(t) => py_str(t),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}
