//! [`Conversation`]: the main transcript's titles, prompts and
//! continuation.

use super::super::stores::ConversationStore;
use super::super::{Prompt, Titles};
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

    /// The human prompts of the main transcript, in time order (equal ones
    /// once).
    pub fn prompts(&self) -> impl ExactSizeIterator<Item = Prompt<'a>> + use<'a> {
        self.store
            .prompts
            .iter()
            .enumerate()
            .map(|(index, (t, s))| Prompt {
                index,
                at: Ts(t.0),
                text: s.as_str(),
            })
    }

    /// The latest human prompt at or before `when` (zoetrope's
    /// `prompt_for_ts`).
    pub fn prompt_before(&self, when: Option<Ts>) -> Option<Prompt<'a>> {
        let when = when?;
        self.prompts().take_while(|p| p.at.0 <= when.0).last()
    }
}
