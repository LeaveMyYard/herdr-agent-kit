//! [`TailInfo`]: what the end of a transcript says.

use serde_json::{Map, Value};

use super::{Block, Line, blank, py_strip, summarize_tool};
use crate::agent_session::Ts;

/// One tool call still waiting for its result (or the last call made).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ToolCall {
    /// The tool name (`"?"` when the record has none).
    pub name: String,
    /// Its `input` object (empty when the record has none).
    pub input: Map<String, Value>,
    /// The record's own time.
    pub ts: Option<Ts>,
}

/// What the end of a transcript says: activity, calls in flight, last text,
/// model and times. Panorama's `TailInfo`, read by
/// [`super::io::summarize_tail`](crate::agent_session::claude::io::summarize_tail).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TailInfo {
    /// The first line's own time.
    pub first_ts: Option<Ts>,
    /// The time of the last dated `user`, `assistant` or `system` record.
    pub last_ts: Option<Ts>,
    /// The last assistant model (placeholders such as `<synthetic>` skipped).
    pub model: Option<String>,
    /// The last non-blank assistant text, trimmed.
    pub last_text: String,
    /// [`summarize_tool`] of the newest call in flight, else of the last call.
    pub activity: String,
    /// Calls without a result yet, oldest first.
    pub pending: Vec<ToolCall>,
    /// The agent called `SubagentHandback`.
    pub handback: bool,
}

impl TailInfo {
    /// Folds the tail window's lines (each one raw line, partial first line
    /// already dropped), with the file's first line `head` when it was read
    /// whole. Pure; [`super::io::summarize_tail`](crate::agent_session::claude::io::summarize_tail) does the reading.
    pub fn from_lines<L: AsRef<[u8]>>(
        head: Option<&[u8]>,
        lines: &[L],
        cwd: Option<&str>,
    ) -> TailInfo {
        let mut info = TailInfo {
            first_ts: head.and_then(Line::parse).and_then(|l| l.ts()),
            ..TailInfo::default()
        };
        // first-insertion order, as a Python dict keeps it when a key is set again
        let mut order: Vec<String> = Vec::new();
        let mut uses: std::collections::HashMap<String, ToolCall> =
            std::collections::HashMap::new();
        let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_use: Option<String> = None;
        for raw in lines {
            let raw = raw.as_ref();
            if blank(raw) {
                continue;
            }
            let Some(rec) = Line::parse(raw) else {
                continue;
            };
            let when = rec.ts();
            let kind = rec.kind();
            if when.is_some() && matches!(kind, Some("user" | "assistant" | "system")) {
                info.last_ts = when;
            }
            let assistant = kind == Some("assistant");
            if assistant
                && let Some(model) = rec
                    .message()
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                && !model.starts_with('<')
            {
                info.model = Some(model.to_string());
            }
            for b in rec.blocks() {
                match b {
                    Block::ToolUse {
                        id: Some(id),
                        name,
                        input,
                    } if assistant => {
                        let call = ToolCall {
                            name: name.unwrap_or("?").to_string(),
                            input: input.cloned().unwrap_or_default(),
                            ts: when,
                        };
                        if uses.insert(id.to_string(), call).is_none() {
                            order.push(id.to_string());
                        }
                        last_use = Some(id.to_string());
                        if name == Some("SubagentHandback") {
                            info.handback = true;
                        }
                    }
                    Block::Text(Some(text)) if assistant && !py_strip(text).is_empty() => {
                        info.last_text = py_strip(text).to_string();
                    }
                    Block::ToolResult {
                        tool_use_id: Some(id),
                        ..
                    } => {
                        done.insert(id.to_string());
                    }
                    _ => {}
                }
            }
        }
        info.pending = order
            .iter()
            .filter(|id| !done.contains(*id))
            .filter_map(|id| uses.get(id).cloned())
            .collect();
        let current = info
            .pending
            .last()
            .cloned()
            .or_else(|| last_use.and_then(|id| uses.remove(&id)));
        if let Some(call) = current {
            info.activity = summarize_tool(&call.name, Some(&call.input), cwd);
        }
        info
    }
}
