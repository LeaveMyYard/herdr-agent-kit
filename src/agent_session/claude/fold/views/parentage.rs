//! [`Parentage`]: which call spawned an agent, its acknowledgement, its
//! parent and children.

use std::collections::HashMap;
use std::sync::OnceLock;

use indexmap::IndexMap;

use super::super::stores::SpawnStore;
use super::super::{Ack, AgentRec, Owner, Spawn};
use crate::pytext::non_empty;

/// Who spawned whom ([`Session::parentage`](super::super::Session::parentage)).
///
/// `meta.toolUseId` is the `tool_use.id` of the spawning call; the owner of
/// that call (main or another agent's file) is the parent, so depth-2 agents
/// nest under the agent that spawned them.
#[derive(Clone, Copy, Debug)]
pub struct Parentage<'a> {
    pub(super) agents: &'a IndexMap<String, AgentRec>,
    pub(super) spawns: &'a SpawnStore,
    pub(super) children: &'a OnceLock<HashMap<Owner, Vec<String>>>,
}

impl<'a> Parentage<'a> {
    fn call_of(&self, id: &str) -> Option<&'a str> {
        self.agents
            .get(id)
            .and_then(|r| non_empty(&r.meta, "toolUseId"))
            .or_else(|| self.spawns.ack_of.get(id).map(String::as_str))
    }

    /// The call that spawned agent `id`.
    pub fn spawn_of(&self, id: &str) -> Option<&'a Spawn> {
        self.spawns.spawns.get(self.call_of(id)?)
    }

    /// The acknowledgement of agent `id`'s spawning call.
    pub fn ack(&self, id: &str) -> Option<&'a Ack> {
        let call = match self.spawns.ack_of.get(id) {
            Some(c) => Some(c.as_str()),
            None => self
                .agents
                .get(id)
                .and_then(|r| non_empty(&r.meta, "toolUseId")),
        };
        self.spawns.acks.get(call?)
    }

    /// The agent that spawned agent `id` ([`Owner::Main`] for the session),
    /// via `meta.toolUseId`.
    ///
    /// A fork's own file repeats its history, including the call that
    /// spawned it, so the agent's own copy never counts. Of the other files
    /// holding the call, main wins, then the earliest. If only the agent's own
    /// copy was read (the parent's file can be skipped as old),
    /// `meta.parentAgentId` names the parent.
    ///
    /// "The earliest" is panorama's `min` over each file's earliest time
    /// (no time sorts last, a time of `0` first), not the rule that picks
    /// [`Parentage::spawn_of`]'s copy, where `0` sorts last: both follow
    /// panorama.
    pub fn parent_of(&self, id: &str) -> Option<Owner> {
        let call = self.call_of(id);
        let me = Owner::Agent(id.to_string());
        let mut best: Option<(&Owner, f64)> = None;
        if let Some(owners) = call.and_then(|c| self.spawns.owners.get(c)) {
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
        call.filter(|c| self.spawns.spawns.contains_key(*c))
            .map(|_| Owner::Main)
    }

    /// The agents whose parent is `parent`, by id, in the order they were
    /// first recorded.
    pub fn children(&self, parent: &Owner) -> impl ExactSizeIterator<Item = &'a str> + use<'a> {
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
            .map(|c| c.iter())
            .unwrap_or_default()
            .map(String::as_str)
    }
}
