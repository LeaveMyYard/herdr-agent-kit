//! `ask_operator` calls and the inbox question ids their results name.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use super::super::Owner;
use crate::agent_session::claude::wire;
use crate::ask_operator::question_ids;

/// The asker of each `ask_operator` call in flight, and the question ids
/// each owner's answered calls recorded.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct AskStore {
    pending: HashMap<String, Owner>,
    pub(in super::super) qids: HashMap<Owner, BTreeSet<u64>>,
}

impl AskStore {
    /// An `ask_operator` call `id` by `owner`.
    pub(in super::super) fn on_call(&mut self, id: &str, owner: &Owner) {
        self.pending.insert(id.to_string(), owner.clone());
    }

    /// A tool result for `call`: when it answers an `ask_operator` call, the
    /// question ids in its content are the asker's.
    pub(in super::super) fn on_result(&mut self, call: &str, content: Option<&Value>) {
        if let Some(asker) = self.pending.remove(call) {
            self.qids
                .entry(asker)
                .or_default()
                .extend(question_ids(&wire::text_of(content)));
        }
    }
}
