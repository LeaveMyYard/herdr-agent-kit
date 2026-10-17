//! Creating a [`Session`] and forgetting what was folded into it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use indexmap::IndexMap;

use super::{Session, Titles};
use crate::agent_session::SessionId;
use crate::agent_session::claude::Continuation;

/// Construction, identity and reset.
impl Session {
    /// An empty fold of session `own`.
    pub fn new(own: SessionId) -> Session {
        Session {
            agents: IndexMap::new(),
            files: HashMap::new(),
            spawns: HashMap::new(),
            acks: HashMap::new(),
            ack_of: HashMap::new(),
            notices: HashMap::new(),
            resumes: HashMap::new(),
            wf_launches: IndexMap::new(),
            journal_started: HashMap::new(),
            journal_phases: HashMap::new(),
            journal_done: HashMap::new(),
            journal_failed: HashMap::new(),
            spawn_results: HashSet::new(),
            spawn_owners: HashMap::new(),
            wf_summary: HashMap::new(),
            prompts: BTreeSet::new(),
            titles: Titles::default(),
            continuation: Continuation::new(own),
            qids: HashMap::new(),
            pending_ask: HashMap::new(),
            hook_starts: HashMap::new(),
            hook_stops: HashMap::new(),
            hook_procs: Vec::new(),
            bg: IndexMap::new(),
            bg_calls: HashMap::new(),
            bg_stop_calls: HashMap::new(),
            bg_stops: HashMap::new(),
            children: OnceLock::new(),
        }
    }

    /// The session's id.
    pub fn id(&self) -> &SessionId {
        self.continuation.own()
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
        let own = self.continuation.own().clone();
        *self = Session::new(own);
        self.agents = agents;
        self.files = files;
    }
}
