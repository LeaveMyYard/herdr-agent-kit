//! Folding transcript records and the main transcript's lines.

use serde_json::{Map, Value};

use super::times::{NoticeAt, T};
use super::{Owner, Session};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::{self, Block, Line, Record};

/// Inputs: the main transcript's lines and transcript records.
impl Session {
    /// Folds lines of the **main** transcript into the continuation rule
    /// ([`Continuation::feed`](crate::agent_session::claude::Continuation::feed)): every line, in order.
    pub fn feed_main<L: AsRef<[u8]>>(&mut self, lines: &[L]) {
        self.conversation.continuation.feed(lines);
    }

    /// Folds one transcript record from `owner`'s file, at `when` (its own
    /// time, or the stream time for an undated record): panorama's
    /// `Session.apply`. `continued-in` records belong to the continuation
    /// rule ([`Session::feed_main`]) and are ignored here.
    pub fn apply(&mut self, owner: Owner, rec: &Line, when: Option<Ts>) {
        self.children.take();
        let when = when.map(Ts::secs);
        let notes = wire::parse_notices(rec.notice_text().unwrap_or(""));
        if !notes.is_empty() {
            for note in notes {
                if let (Some(status), Some(t)) = (note.status, when) {
                    self.notices.entry(note.task).or_default().insert(NoticeAt(
                        T(t),
                        status,
                        note.tool_uses,
                    ));
                }
            }
            return;
        }
        match rec.record() {
            Record::Title(kind, text) => self.conversation.set_title(kind, text),
            Record::Assistant { blocks, .. } => {
                for b in blocks {
                    if let Block::ToolUse {
                        id: Some(id),
                        name,
                        input,
                    } = b
                    {
                        self.tool_use(&owner, id, name.unwrap_or(""), input, when);
                    }
                }
            }
            Record::User {
                tool_use_result, ..
            } => {
                if owner == Owner::Main
                    && rec.is_human_prompt()
                    && let Some(t) = when
                {
                    self.conversation
                        .add_prompt(t, rec.string_content().unwrap_or(""));
                }
                let empty = Map::new();
                let tur = tool_use_result.and_then(Value::as_object).unwrap_or(&empty);
                for b in rec.blocks() {
                    if let Block::ToolResult {
                        tool_use_id: Some(call),
                        is_error,
                        content,
                    } = b
                    {
                        self.tool_result(&owner, call, is_error == Some(true), content, tur, when);
                    }
                }
            }
            _ => {}
        }
    }
}
