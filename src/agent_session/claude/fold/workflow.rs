//! Workflow runs: journals, summaries, launches and how a run ended.

use serde_json::{Map, Value};

use super::{Session, WorkflowLaunch};
use crate::agent_session::Ts;
use crate::agent_session::claude::wire::{Ending, Line, py_str, truthy};

/// Workflow runs: journals, summaries, launches and how a run ended.
impl Session {
    /// Folds one line of workflow run `run`'s `journal.jsonl` (`None`: a line
    /// that did not parse).
    pub fn apply_journal(&mut self, run: &str, rec: Option<&Line>) {
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
                self.journal_started
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid, (label, phase.clone()));
                let order = self.journal_phases.entry(run.to_string()).or_default();
                if let Some(p) = phase.filter(|p| !p.is_empty())
                    && !order.contains(&p)
                {
                    order.push(p);
                }
            }
            Some("result") => {
                self.journal_done
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid);
            }
            Some("failed") => {
                self.journal_failed
                    .entry(run.to_string())
                    .or_default()
                    .insert(aid);
            }
            _ => {}
        }
    }

    /// Records workflow run `run`'s summary (`workflows/<run>.json`) and the
    /// file's modification time.
    pub fn set_summary(&mut self, run: &str, summary: Map<String, Value>, mtime: Ts) {
        self.wf_summary.insert(run.to_string(), (summary, mtime.0));
    }

    /// The modification time of run `run`'s summary when it was read.
    pub fn summary_mtime(&self, run: &str) -> Option<Ts> {
        self.wf_summary.get(run).map(|(_, m)| Ts(*m))
    }

    /// The workflow launches, in the order they were first read.
    pub fn wf_launches(&self) -> impl Iterator<Item = &WorkflowLaunch> {
        self.wf_launches.values()
    }

    /// Workflow run `run`'s summary object, if read.
    pub fn wf_summary(&self, run: &str) -> Option<&Map<String, Value>> {
        self.wf_summary.get(run).map(|(s, _)| s)
    }

    /// The journal's `started` record of agent `id` in run `run`: its label
    /// and phase.
    pub fn journal_started(&self, run: &str, id: &str) -> Option<(&str, Option<&str>)> {
        let (label, phase) = self.journal_started.get(run)?.get(id)?;
        Some((label, phase.as_deref()))
    }

    /// The phases of run `run` in the order its journal first named them.
    pub fn journal_phases(&self, run: &str) -> &[String] {
        self.journal_phases.get(run).map_or(&[], Vec::as_slice)
    }

    /// The `(time, ending)` of the latest notice about the run's latest
    /// launch, if any.
    pub fn run_end(&self, run: &str) -> Option<(Ts, Ending)> {
        let mut launches: Vec<&WorkflowLaunch> = self
            .wf_launches
            .values()
            .filter(|w| w.run_id == run)
            .collect();
        launches.sort_by(|a, b| {
            a.ts.map_or(0.0, Ts::secs)
                .total_cmp(&b.ts.map_or(0.0, Ts::secs))
        });
        let task = launches.last()?.task_id.as_deref().unwrap_or("");
        let last = self.notices.get(task)?.last()?;
        Some((Ts(last.0.0), last.1))
    }

    /// Whether run `run` is still running: `None` when no launch of it is
    /// known (compacted away).
    pub fn run_live(&self, run: &str) -> Option<bool> {
        self.wf_launches
            .values()
            .any(|w| w.run_id == run)
            .then(|| self.run_end(run).is_none())
    }
}
