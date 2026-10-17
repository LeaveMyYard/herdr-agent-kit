//! Workflow runs: launches, journals and summaries.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::super::values::WorkflowLaunch;
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::{Ending, Line};
use crate::pytext::{py_str, truthy};

/// What the workflow runs' launches, journals and summaries say.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct WorkflowStore {
    /// By call id, in the order first read.
    pub(in super::super) launches: IndexMap<String, WorkflowLaunch>,
    /// Run → agent id → the journal's `started` label and phase.
    pub(in super::super) started: HashMap<String, HashMap<String, (String, Option<String>)>>,
    /// Run → its phases in the order its journal first named them.
    pub(in super::super) phases: HashMap<String, Vec<String>>,
    /// Run → the agents with a journal `result`.
    done: HashMap<String, HashSet<String>>,
    /// Run → the agents with a journal `failed`.
    failed: HashMap<String, HashSet<String>>,
    /// Run → its summary object and the file's modification time.
    pub(in super::super) summaries: HashMap<String, (Map<String, Value>, f64)>,
}

impl WorkflowStore {
    /// A tool result for `call` (`tur`: its `toolUseResult`): a `Workflow`
    /// call's launch, when it names a run.
    pub(in super::super) fn on_result(
        &mut self,
        call: &str,
        tur: &Map<String, Value>,
        when: Option<f64>,
    ) {
        if let Some(run) = tur.get("runId").filter(|v| truthy(v)) {
            self.launches.insert(
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
    }

    /// One line of run `run`'s journal (`None`: a line that did not parse).
    pub(in super::super) fn apply_journal(&mut self, run: &str, rec: Option<&Line>) {
        let Some(rec) = rec else {
            return;
        };
        let Some(aid) = rec.get("agentId").filter(|v| truthy(v)).map(py_str) else {
            return;
        };
        match rec.kind() {
            Some("started") => {
                let phase = rec.get("phase").and_then(Value::as_str).map(str::to_string);
                let label = match rec.get("label") {
                    Some(v) if truthy(v) => py_str(v),
                    _ => aid.clone(),
                };
                self.started
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid, (label, phase.clone()));
                let order = self.phases.entry(run.to_string()).or_default();
                if let Some(p) = phase.filter(|p| !p.is_empty())
                    && !order.contains(&p)
                {
                    order.push(p);
                }
            }
            Some("result") => {
                self.done.entry(run.to_string()).or_default().insert(aid);
            }
            Some("failed") => {
                self.failed.entry(run.to_string()).or_default().insert(aid);
            }
            _ => {}
        }
    }

    /// Run `run`'s summary and its file's modification time.
    pub(in super::super) fn set_summary(
        &mut self,
        run: &str,
        summary: Map<String, Value>,
        mtime: Ts,
    ) {
        self.summaries.insert(run.to_string(), (summary, mtime.0));
    }

    /// How run `run`'s own journal says agent `id` ended: `failed` before
    /// `result`.
    pub(in super::super) fn ledger(&self, run: &str, id: &str) -> Option<Ending> {
        let has = |m: &HashMap<String, HashSet<String>>| m.get(run).is_some_and(|s| s.contains(id));
        if has(&self.failed) {
            Some(Ending::Failed)
        } else if has(&self.done) {
            Some(Ending::Done)
        } else {
            None
        }
    }
}
