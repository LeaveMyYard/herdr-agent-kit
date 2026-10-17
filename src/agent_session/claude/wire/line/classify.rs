//! [`Line::record`]: which [`Record`] a line holds (the lenient record match).

use serde_json::{Map, Value};

use super::Line;
use crate::agent_session::SessionId;
use crate::agent_session::claude::wire::{Attachment, BOOKKEEPING, Content, Record, TitleKind};

impl Line {
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
            "attachment" => Record::Attachment(attachment(
                self.get("attachment").and_then(Value::as_object),
            )),
            "continued-in" => Record::ContinuedIn {
                from: self.get("sessionId").and_then(SessionId::from_value),
                to: self
                    .get("continuedInSessionId")
                    .and_then(SessionId::from_value),
                sidechain: self.is_sidechain(),
            },
            "custom-title" | "agent-name" | "ai-title" => self.title(kind),
            k if BOOKKEEPING.contains(&k) => Record::Bookkeeping(k),
            _ => Record::Other,
        }
    }

    /// A title record (`custom-title`, `agent-name`, `ai-title`) of type
    /// `kind`; [`Record::Other`] when its title field is not a string.
    fn title(&self, kind: &str) -> Record<'_> {
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
}

/// An `attachment` record's `attachment` object, when it is one.
fn attachment(att: Option<&Map<String, Value>>) -> Attachment<'_> {
    let Some(att) = att else {
        return Attachment::Other(None);
    };
    match att.get("type").and_then(Value::as_str) {
        Some("queued_command") => Attachment::QueuedCommand {
            prompt: att.get("prompt").and_then(Value::as_str),
        },
        Some("hook_additional_context") => Attachment::HookAdditionalContext {
            content: match att.get("content") {
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
                Some(Value::String(s)) => vec![s.as_str()],
                _ => Vec::new(),
            },
        },
        other => Attachment::Other(other),
    }
}
