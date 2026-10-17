//! Folding transcript records and the main transcript's lines.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::limits::PROMPT_MAX;
use super::times::{NoticeAt, T};
use super::{FileFacts, Owner, Session};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::{self, Block, Line, Record};
use crate::pytext::{py_splitlines, py_strip};

/// Inputs: agent files, metas, the main transcript's lines and transcript records.
impl Session {
    /// Records an agent file of the session (its transcript, and the workflow
    /// run it is under), keeping a meta already read.
    pub fn set_agent(&mut self, id: &str, transcript: PathBuf, workflow: Option<String>) {
        self.children.take();
        let rec = self.agents.entry(id.to_string()).or_default();
        rec.transcript = transcript;
        rec.workflow = workflow;
    }

    /// Records an agent's `meta.json` object.
    pub fn set_meta(&mut self, id: &str, meta: Map<String, Value>) {
        self.children.take();
        self.agents.entry(id.to_string()).or_default().meta = meta;
    }

    /// The facts of `owner`'s transcript, for the reader to update.
    pub fn file_mut(&mut self, owner: Owner) -> &mut FileFacts {
        self.files.entry(owner).or_default()
    }

    /// Folds lines of the **main** transcript into the continuation rule
    /// ([`Continuation::feed`](crate::agent_session::claude::Continuation::feed)): every line, in order.
    pub fn feed_main<L: AsRef<[u8]>>(&mut self, lines: &[L]) {
        self.continuation.feed(lines);
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
            Record::Title(kind, text) => {
                let slot = match kind {
                    wire::TitleKind::Custom => &mut self.titles.custom,
                    wire::TitleKind::Agent => &mut self.titles.agent,
                    wire::TitleKind::Ai => &mut self.titles.ai,
                };
                *slot = Some(text.to_string());
            }
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
                    let text = rec.string_content().unwrap_or("");
                    let first = py_splitlines(text)
                        .into_iter()
                        .map(py_strip)
                        .find(|l| !l.is_empty())
                        .unwrap_or("");
                    self.prompts
                        .insert((T(t), first.chars().take(PROMPT_MAX).collect()));
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
