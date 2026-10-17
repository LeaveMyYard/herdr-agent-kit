//! [`Line`]: one parsed transcript line and its field accessors. Its view as a
//! [`Record`] is in `classify`, the repair of lone surrogate escapes in
//! `repair`.

mod classify;
mod repair;

use serde_json::{Map, Value};

use super::{Attachment, Block, Record, contains, ts};
use crate::agent_session::Ts;
use crate::pytext::{py_isspace, py_strip, truthy};

/// One parsed transcript line: a JSON object. Everything else (a broken line,
/// an array, a number) is not a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Line(Map<String, Value>);

impl Line {
    /// Parses one raw line (trailing `\r` and surrounding JSON whitespace
    /// allowed; a leading UTF-8 BOM skipped): panorama's `wire.parse_line`.
    pub fn parse(raw: &[u8]) -> Option<Line> {
        let raw = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(raw);
        match serde_json::from_slice::<Value>(raw) {
            Ok(Value::Object(map)) => Some(Line(map)),
            Ok(_) => None,
            // Python's json accepts a lone UTF-16 surrogate escape (Node writes one
            // when a string is cut inside an emoji); serde_json does not
            Err(_) if contains(raw, b"\\ud") || contains(raw, b"\\uD") => {
                match serde_json::from_slice::<Value>(&repair::replace_lone_surrogates(raw)) {
                    Ok(Value::Object(map)) => Some(Line(map)),
                    _ => None,
                }
            }
            Err(_) => None,
        }
    }

    /// A line built from an already parsed object.
    pub fn from_map(map: Map<String, Value>) -> Line {
        Line(map)
    }

    /// The whole object, owned.
    pub fn into_map(self) -> Map<String, Value> {
        self.0
    }

    /// The whole object.
    pub fn map(&self) -> &Map<String, Value> {
        &self.0
    }

    /// One top-level field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// The record type (`"type"`), when it is a string.
    pub fn kind(&self) -> Option<&str> {
        self.str_field("type")
    }

    /// The record's **own** (top-level) timestamp. A nested one, such as a
    /// `file-history-snapshot`'s `snapshot.timestamp`, never dates a record.
    pub fn ts(&self) -> Option<Ts> {
        self.get("timestamp").and_then(ts)
    }

    /// `isSidechain: true` (exactly the JSON `true`).
    pub fn is_sidechain(&self) -> bool {
        matches!(self.get("isSidechain"), Some(Value::Bool(true)))
    }

    fn str_field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// `message`, when it is an object.
    pub fn message(&self) -> Option<&Map<String, Value>> {
        self.get("message").and_then(Value::as_object)
    }

    /// The object blocks of `message.content` when it is a list (any record
    /// type): panorama's `wire.blocks`.
    pub fn blocks(&self) -> Vec<Block<'_>> {
        match self.message().and_then(|m| m.get("content")) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_object)
                .map(Block::of)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `message.content` when it is a string that is not blank: panorama's
    /// `wire.string_content`.
    pub fn string_content(&self) -> Option<&str> {
        match self.message().and_then(|m| m.get("content")) {
            Some(Value::String(s)) if !py_strip(s).is_empty() => Some(s),
            _ => None,
        }
    }

    /// The text a typed prompt or a `<task-notification>` arrives in, from any
    /// of its three carriers (a `queue-operation` `enqueue`, an `attachment`
    /// `queued_command`, a `user` string): panorama's `wire.notice_text`.
    pub fn notice_text(&self) -> Option<&str> {
        match self.record() {
            Record::QueueEnqueue { content } => content,
            Record::Attachment(Attachment::QueuedCommand { prompt }) => prompt,
            Record::User { .. } => self.string_content(),
            _ => None,
        }
    }

    /// An operator prompt: panorama's `wire.is_human_prompt` (zoetrope's).
    /// `origin.kind == "human"`; a legacy record without an origin counts when
    /// its string content is not a notification or a local command. Meta
    /// records are never prompts.
    pub fn is_human_prompt(&self) -> bool {
        if self.kind() != Some("user") || self.get("isMeta").is_some_and(truthy) {
            return false;
        }
        let Some(text) = self.string_content() else {
            return false;
        };
        match self
            .get("origin")
            .and_then(Value::as_object)
            .and_then(|o| o.get("kind"))
        {
            Some(kind) if !kind.is_null() => kind.as_str() == Some("human"),
            _ => {
                let t = text.trim_start_matches(py_isspace);
                !["<task-notification>", "<local-command", "<command-"]
                    .iter()
                    .any(|p| t.starts_with(p))
            }
        }
    }
}

#[cfg(test)]
mod tests;
