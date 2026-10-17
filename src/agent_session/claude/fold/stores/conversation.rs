//! The main transcript's conversation: the session's titles, its human
//! prompts and the session it continued in.

use std::collections::BTreeSet;

use super::super::limits::PROMPT_MAX;
use super::super::times::T;
use super::super::values::Titles;
use crate::agent_session::SessionId;
use crate::agent_session::claude::Continuation;
use crate::agent_session::claude::wire::TitleKind;
use crate::pytext::{py_splitlines, py_strip};

/// Titles, prompts and the continuation rule of the main transcript.
#[derive(Clone, Debug)]
pub(in super::super) struct ConversationStore {
    pub(in super::super) titles: Titles,
    /// `(time, first line)` of each human prompt.
    pub(in super::super) prompts: BTreeSet<(T, String)>,
    pub(in super::super) continuation: Continuation,
}

impl ConversationStore {
    pub(in super::super) fn new(own: SessionId) -> ConversationStore {
        ConversationStore {
            titles: Titles::default(),
            prompts: BTreeSet::new(),
            continuation: Continuation::new(own),
        }
    }

    /// A title record: the latest of each kind wins.
    pub(in super::super) fn set_title(&mut self, kind: TitleKind, text: &str) {
        let slot = match kind {
            TitleKind::Custom => &mut self.titles.custom,
            TitleKind::Agent => &mut self.titles.agent,
            TitleKind::Ai => &mut self.titles.ai,
        };
        *slot = Some(text.to_string());
    }

    /// A human prompt at `t`: its first non-blank line, cut to
    /// [`PROMPT_MAX`] characters.
    pub(in super::super) fn add_prompt(&mut self, t: f64, text: &str) {
        let first = py_splitlines(text)
            .into_iter()
            .map(py_strip)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        self.prompts
            .insert((T(t), first.chars().take(PROMPT_MAX).collect()));
    }
}
