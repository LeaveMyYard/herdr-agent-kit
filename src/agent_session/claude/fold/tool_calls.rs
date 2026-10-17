//! Tool calls and their results: spawns, acknowledgements, workflow
//! launches, resumes and `ask_operator` question ids.

use serde_json::{Map, Value};

use super::fields::{py_int, str_or_empty};
use super::times::{T, or_inf};
use super::{Ack, BgKind, Owner, Session, Spawn, WorkflowLaunch};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::{self, BG_STOP_TOOLS, SPAWN_TOOLS, py_str, truthy};

/// Order-independent choice when one spawning call appears in several files:
/// main wins, then the earliest record (the original, not a fork's copy).
fn better_owner(owner: &Owner, when: Option<f64>, old: &Spawn) -> bool {
    if old.owner == Owner::Main {
        return false;
    }
    if *owner == Owner::Main {
        return true;
    }
    or_inf(when) < or_inf(old.ts.map(Ts::secs))
}

/// Tool calls and their results, folded from records ([`Session::apply`]).
impl Session {
    pub(super) fn tool_use(
        &mut self,
        owner: &Owner,
        id: &str,
        name: &str,
        input: Option<&Map<String, Value>>,
        when: Option<f64>,
    ) {
        let empty = Map::new();
        let inp = input.unwrap_or(&empty);
        if SPAWN_TOOLS.contains(&name) {
            let owners = self.spawn_owners.entry(id.to_string()).or_default();
            let at = owners.entry(owner.clone()).or_insert(f64::INFINITY);
            *at = at.min(when.unwrap_or(f64::INFINITY));
            if let Some(old) = self.spawns.get(id)
                && !better_owner(owner, when, old)
            {
                return; // a fork's file repeats its parent's history, spawning calls included
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
        } else if name.ends_with("ask_operator") {
            self.pending_ask.insert(id.to_string(), owner.clone());
        } else if name == "Bash" || name == "Monitor" {
            // `bg_calls` and `bg_stop_calls` are read by `background.rs`
            let kind = if name == "Bash" {
                BgKind::Bash
            } else {
                BgKind::Monitor
            };
            self.bg_calls
                .insert(id.to_string(), (kind, str_or_empty(inp, "description")));
        } else if BG_STOP_TOOLS.contains(&name) {
            // `task_id or shell_id`: a truthy task_id of another type wins, and is no task
            let task = match inp.get("task_id") {
                Some(v) if truthy(v) => Some(v),
                _ => inp.get("shell_id"),
            };
            if let Some(task) = task.and_then(Value::as_str).filter(|t| !t.is_empty()) {
                self.bg_stop_calls.insert(id.to_string(), task.to_string());
            }
        }
    }

    pub(super) fn tool_result(
        &mut self,
        owner: &Owner,
        call: &str,
        is_error: bool,
        content: Option<&Value>,
        tur: &Map<String, Value>,
        when: Option<f64>,
    ) {
        let agent_id = tur.get("agentId").filter(|v| truthy(v));
        if self.spawns.contains_key(call) || agent_id.is_some() {
            self.spawn_results.insert(call.to_string());
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
        if let Some(run) = tur.get("runId").filter(|v| truthy(v)) {
            self.wf_launches.insert(
                call.to_string(),
                WorkflowLaunch {
                    call: call.to_string(),
                    ts: when.map(Ts),
                    run_id: py_str(run),
                    task_id: tur
                        .get("taskId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    name: tur.get("workflowName").filter(|v| truthy(v)).map(py_str),
                },
            );
        }
        if let (Some(agent), Some(t)) = (tur.get("resumedAgentId").and_then(Value::as_str), when)
            && tur.get("success") != Some(&Value::Bool(false))
        {
            self.resumes
                .entry(agent.to_string())
                .or_default()
                .insert(T(t));
        }
        self.background(owner, call, tur, is_error, when);
        if let Some(asker) = self.pending_ask.remove(call) {
            self.qids
                .entry(asker)
                .or_default()
                .extend(wire::question_ids(&wire::text_of(content)));
        }
    }
}
