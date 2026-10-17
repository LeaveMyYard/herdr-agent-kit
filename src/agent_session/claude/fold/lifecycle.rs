//! Creating a [`Session`] and forgetting what was folded into it.

use std::collections::HashMap;
use std::sync::OnceLock;

use indexmap::IndexMap;

use super::Session;
use super::stores::{AskStore, BgStore, ConversationStore, HookStore, SpawnStore, WorkflowStore};
use crate::agent_session::SessionId;

/// Construction, identity and reset.
impl Session {
    /// An empty fold of session `own`.
    pub fn new(own: SessionId) -> Session {
        Session {
            agents: IndexMap::new(),
            files: HashMap::new(),
            notices: HashMap::new(),
            conversation: ConversationStore::new(own),
            spawns: SpawnStore::default(),
            workflows: WorkflowStore::default(),
            background: BgStore::default(),
            hooks: HookStore::default(),
            asks: AskStore::default(),
            children: OnceLock::new(),
        }
    }

    /// The session's id.
    pub fn id(&self) -> &SessionId {
        self.conversation.continuation.own()
    }

    /// Forgets everything folded from records (a file was rewritten): every
    /// store, the hook records, the workflow summaries, the continuation, and
    /// each file's stream time and tool-call count. The agents, their metas
    /// and the tail summaries stay (panorama's `_reset_stores`).
    pub fn reset(&mut self) {
        let agents = std::mem::take(&mut self.agents);
        let mut files = std::mem::take(&mut self.files);
        for f in files.values_mut() {
            f.last_ts = None;
            f.tool_calls = 0;
        }
        let own = self.id().clone();
        *self = Session::new(own);
        self.agents = agents;
        self.files = files;
    }
}
