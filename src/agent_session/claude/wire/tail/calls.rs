//! The tool calls a tail window makes and answers, in first-call order.

use std::collections::{HashMap, HashSet};

use super::ToolCall;

/// Calls seen in a tail window: made (by id, first-insertion order kept as a
/// Python dict keeps it when a key is set again), answered, and the last one.
#[derive(Default)]
pub(super) struct Calls {
    order: Vec<String>,
    by_id: HashMap<String, ToolCall>,
    answered: HashSet<String>,
    last: Option<String>,
}

impl Calls {
    /// A `tool_use` block with this id.
    pub(super) fn made(&mut self, id: &str, call: ToolCall) {
        if self.by_id.insert(id.to_string(), call).is_none() {
            self.order.push(id.to_string());
        }
        self.last = Some(id.to_string());
    }

    /// A `tool_result` block for this id.
    pub(super) fn answered(&mut self, id: &str) {
        self.answered.insert(id.to_string());
    }

    /// The calls without a result, oldest first, and the current one: the
    /// newest of those, else the last call made.
    pub(super) fn finish(mut self) -> (Vec<ToolCall>, Option<ToolCall>) {
        let pending: Vec<ToolCall> = self
            .order
            .iter()
            .filter(|id| !self.answered.contains(*id))
            .filter_map(|id| self.by_id.get(id).cloned())
            .collect();
        let current = pending
            .last()
            .cloned()
            .or_else(|| self.last.and_then(|id| self.by_id.remove(&id)));
        (pending, current)
    }
}
