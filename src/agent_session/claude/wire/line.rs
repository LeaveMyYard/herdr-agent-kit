//! [`Line`]: one parsed transcript line, and its view as a [`Record`].

use serde_json::{Map, Value};

use super::{
    Attachment, BOOKKEEPING, Block, Content, Record, TitleKind, contains, py_isspace, py_strip,
    truthy, ts,
};
use crate::agent_session::{SessionId, Ts};

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
                match serde_json::from_slice::<Value>(&replace_lone_surrogates(raw)) {
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

    /// The record this line holds. Lenient: an unknown type, or a known one
    /// whose fields do not have the expected shape, is [`Record::Other`].
    pub fn record(&self) -> Record<'_> {
        let Some(kind) = self.kind() else {
            return Record::Other;
        };
        match kind {
            "assistant" => Record::Assistant {
                model: self
                    .message()
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str),
                blocks: self.blocks(),
            },
            "user" => Record::User {
                content: match self.message().and_then(|m| m.get("content")) {
                    Some(Value::String(s)) => Content::Text(s),
                    Some(Value::Array(_)) => Content::Blocks(self.blocks()),
                    _ => Content::None,
                },
                tool_use_result: self.get("toolUseResult"),
            },
            "queue-operation" if self.str_field("operation") == Some("enqueue") => {
                Record::QueueEnqueue {
                    content: self.str_field("content"),
                }
            }
            "attachment" => {
                Record::Attachment(match self.get("attachment").and_then(Value::as_object) {
                    Some(att) => match att.get("type").and_then(Value::as_str) {
                        Some("queued_command") => Attachment::QueuedCommand {
                            prompt: att.get("prompt").and_then(Value::as_str),
                        },
                        Some("hook_additional_context") => Attachment::HookAdditionalContext {
                            content: match att.get("content") {
                                Some(Value::Array(items)) => {
                                    items.iter().filter_map(Value::as_str).collect()
                                }
                                Some(Value::String(s)) => vec![s.as_str()],
                                _ => Vec::new(),
                            },
                        },
                        other => Attachment::Other(other),
                    },
                    None => Attachment::Other(None),
                })
            }
            "continued-in" => Record::ContinuedIn {
                from: self.get("sessionId").and_then(SessionId::from_value),
                to: self
                    .get("continuedInSessionId")
                    .and_then(SessionId::from_value),
                sidechain: self.is_sidechain(),
            },
            "custom-title" | "agent-name" | "ai-title" => {
                let (title, key) = match kind {
                    "custom-title" => (TitleKind::Custom, "customTitle"),
                    "agent-name" => (TitleKind::Agent, "agentName"),
                    _ => (TitleKind::Ai, "aiTitle"),
                };
                match self.str_field(key) {
                    Some(v) => Record::Title(title, v),
                    None => Record::Other,
                }
            }
            k if BOOKKEEPING.contains(&k) => Record::Bookkeeping(k),
            _ => Record::Other,
        }
    }
}

/// The code unit of a `\uXXXX` escape starting at `raw[i]` (the backslash).
fn escape_unit(raw: &[u8], i: usize) -> Option<u16> {
    let hex = raw.get(i + 2..i + 6)?;
    if raw.get(i + 1) != Some(&b'u') || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
}

/// `raw` with every unpaired UTF-16 surrogate escape (`\ud800`..`\udfff`
/// not part of a high-low pair) replaced by `\ufffd`. Escaped backslashes are
/// skipped, so `\\ud83d` (a backslash, then text) is left alone.
fn replace_lone_surrogates(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        match escape_unit(raw, i) {
            Some(0xD800..=0xDBFF) if matches!(escape_unit(raw, i + 6), Some(0xDC00..=0xDFFF)) => {
                out.extend_from_slice(&raw[i..i + 12]);
                i += 12;
            }
            Some(0xD800..=0xDFFF) => {
                out.extend_from_slice(b"\\ufffd");
                i += 6;
            }
            _ => {
                // any other escape (or a stray backslash): copy it and the byte it escapes
                out.extend_from_slice(&raw[i..(i + 2).min(raw.len())]);
                i += 2;
            }
        }
    }
    out
}
