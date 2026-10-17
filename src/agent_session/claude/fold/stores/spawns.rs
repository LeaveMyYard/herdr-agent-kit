//! Calls that start or resume an agent: spawning calls (`Agent`, `Task`),
//! their acknowledgements, and `SendMessage` resumes.

use std::collections::{BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::super::owner::{Owner, replaces};
use super::super::times::T;
use super::super::values::{Ack, Spawn};
use crate::agent_session::Ts;
use crate::pytext::{py_int, py_str, str_or_empty, truthy};

/// Spawning calls, their results and resumes.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct SpawnStore {
    /// By call id: the copy that counts ([`replaces`]).
    pub(in super::super) spawns: HashMap<String, Spawn>,
    /// By call id.
    pub(in super::super) acks: HashMap<String, Ack>,
    /// Agent id → the call its acknowledgement answers.
    pub(in super::super) ack_of: HashMap<String, String>,
    /// The calls that have a result (`agentId`, or a known spawning call).
    pub(in super::super) results: HashSet<String>,
    /// Call id → every file holding the call, with its earliest time there.
    pub(in super::super) owners: HashMap<String, IndexMap<Owner, f64>>,
    /// Agent id → the times `SendMessage` resumed it.
    pub(in super::super) resumes: HashMap<String, BTreeSet<T>>,
}

impl SpawnStore {
    /// A spawning call `id` in `owner`'s file.
    pub(in super::super) fn on_call(
        &mut self,
        owner: &Owner,
        id: &str,
        inp: &Map<String, Value>,
        when: Option<f64>,
    ) {
        let owners = self.owners.entry(id.to_string()).or_default();
        let at = owners.entry(owner.clone()).or_insert(f64::INFINITY);
        *at = at.min(when.unwrap_or(f64::INFINITY));
        if let Some(old) = self.spawns.get(id)
            && !replaces(owner, when, &old.owner, old.ts.map(Ts::secs))
        {
            return;
        }
        let background = match inp.get("run_in_background") {
            Some(Value::Bool(true)) => true,
            Some(v) => py_str(v).to_lowercase() == "true",
            None => false,
        };
        let text = |key: &str| inp.get(key).and_then(Value::as_str).map(str::to_string);
        self.spawns.insert(
            id.to_string(),
            Spawn {
                call: id.to_string(),
                owner: owner.clone(),
                ts: when.map(Ts),
                description: str_or_empty(inp, "description"),
                model: text("model"),
                subagent_type: text("subagent_type"),
                background,
            },
        );
    }

    /// A tool result for `call` (`tur`: its `toolUseResult`): a spawning
    /// call's acknowledgement, or a resume.
    pub(in super::super) fn on_result(
        &mut self,
        call: &str,
        is_error: bool,
        tur: &Map<String, Value>,
        when: Option<f64>,
    ) {
        let agent_id = tur.get("agentId").filter(|v| truthy(v));
        if self.spawns.contains_key(call) || agent_id.is_some() {
            self.results.insert(call.to_string());
        }
        if let (Some(agent), Some(status)) = (agent_id, tur.get("status").filter(|v| truthy(v))) {
            let agent = py_str(agent);
            self.acks.insert(
                call.to_string(),
                Ack {
                    call: call.to_string(),
                    ts: when.map(Ts),
                    agent_id: agent.clone(),
                    status: py_str(status),
                    is_error,
                    model: tur
                        .get("resolvedModel")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    tool_count: py_int(tur.get("totalToolUseCount")),
                },
            );
            self.ack_of.insert(agent, call.to_string());
        }
        if let (Some(agent), Some(t)) = (tur.get("resumedAgentId").and_then(Value::as_str), when)
            && tur.get("success") != Some(&Value::Bool(false))
        {
            self.resumes
                .entry(agent.to_string())
                .or_default()
                .insert(T(t));
        }
    }
}
