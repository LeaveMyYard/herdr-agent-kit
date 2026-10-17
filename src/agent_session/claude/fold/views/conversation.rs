//! [`Conversation`]: the main transcript's titles, prompts and
//! continuation.

use super::super::Titles;
use super::super::stores::ConversationStore;
use crate::agent_session::{SessionId, Ts};

/// The main transcript's conversation
/// ([`Session::conversation`](super::super::Session::conversation)).
#[derive(Clone, Copy, Debug)]
pub struct Conversation<'a> {
    pub(super) store: &'a ConversationStore,
}

impl<'a> Conversation<'a> {
    /// The session's titles.
    pub fn titles(&self) -> &'a Titles {
        &self.store.titles
    }

    /// The session the conversation continued in (D6), if it did and was not
    /// resumed since. Forward only: nothing here names the sessions that
    /// continued **into** this one.
    pub fn continued_in(&self) -> Option<&'a SessionId> {
        self.store.continuation.next()
    }

    /// The human prompts of the main transcript, `(time, first line)`, in
    /// time order (equal ones once).
    pub fn prompts(&self) -> impl Iterator<Item = (Ts, &'a str)> + 'a {
        self.store
            .prompts
            .iter()
            .map(|(t, s)| (Ts(t.0), s.as_str()))
    }

    /// The latest human prompt at or before `when`: its index among
    /// [`Conversation::prompts`] and its text (zoetrope's `prompt_for_ts`).
    pub fn prompt_before(&self, when: Option<Ts>) -> Option<(usize, &'a str)> {
        let when = when?.0;
        self.store
            .prompts
            .iter()
            .enumerate()
            .take_while(|(_, (t, _))| t.0 <= when)
            .last()
            .map(|(i, (_, s))| (i, s.as_str()))
    }
}
