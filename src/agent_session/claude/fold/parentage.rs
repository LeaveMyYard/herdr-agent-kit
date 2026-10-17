//! Parentage: which call spawned an agent, its acknowledgement, its parent
//! and children.

use std::collections::HashMap;

use super::fields::non_empty;
use super::{Ack, Owner, Session, Spawn};

impl Session {
    fn call_of(&self, id: &str) -> Option<&str> {
        self.agents
            .get(id)
            .and_then(|r| non_empty(&r.meta, "toolUseId"))
            .or_else(|| self.ack_of.get(id).map(String::as_str))
    }

    /// The call that spawned agent `id`.
    pub fn spawn_of(&self, id: &str) -> Option<&Spawn> {
        self.spawns.get(self.call_of(id)?)
    }

    /// The acknowledgement of agent `id`'s spawning call.
    pub fn ack(&self, id: &str) -> Option<&Ack> {
        let call = match self.ack_of.get(id) {
            Some(c) => Some(c.as_str()),
            None => self
                .agents
                .get(id)
                .and_then(|r| non_empty(&r.meta, "toolUseId")),
        };
        self.acks.get(call?)
    }

    /// The agent that spawned agent `id` ([`Owner::Main`] for the session),
    /// via `meta.toolUseId`.
    ///
    /// A fork's own file repeats its history, including the call that
    /// spawned it, so the agent's own copy never counts. Of the other files
    /// holding the call, main wins, then the earliest. If only the agent's own
    /// copy was read (the parent's file can be skipped as old),
    /// `meta.parentAgentId` names the parent.
    pub fn parent_of(&self, id: &str) -> Option<Owner> {
        let call = self.call_of(id);
        let me = Owner::Agent(id.to_string());
        let mut best: Option<(&Owner, f64)> = None;
        if let Some(owners) = call.and_then(|c| self.spawn_owners.get(c)) {
            for (o, t) in owners.iter().filter(|(o, _)| **o != me) {
                if *o == Owner::Main {
                    return Some(Owner::Main);
                }
                if best.is_none_or(|(_, bt)| *t < bt) {
                    best = Some((o, *t));
                }
            }
        }
        if let Some((o, _)) = best {
            return Some(o.clone());
        }
        if let Some(parent) = self
            .agents
            .get(id)
            .and_then(|r| non_empty(&r.meta, "parentAgentId"))
        {
            return Some(Owner::Agent(parent.to_string()));
        }
        call.filter(|c| self.spawns.contains_key(*c))
            .map(|_| Owner::Main)
    }

    /// The agents whose parent is `parent`, by id.
    pub fn children(&self, parent: &Owner) -> Vec<&str> {
        let index = self.children.get_or_init(|| {
            let mut index: HashMap<Owner, Vec<String>> = HashMap::new();
            for id in self.agents.keys() {
                if let Some(p) = self.parent_of(id) {
                    index.entry(p).or_default().push(id.clone());
                }
            }
            index
        });
        index
            .get(parent)
            .map(|c| c.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}
