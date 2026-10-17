//! Routes a tool call and its result to the stores that fold them: the one
//! place that knows which tool feeds which store.

use serde_json::{Map, Value};

use super::{BgKind, Owner, Session};
use crate::agent_session::claude::wire::{BASH, BG_STOP_TOOLS, MONITOR, SPAWN_TOOLS};
use crate::ask_operator::is_ask_operator;

/// Tool calls and their results, from records ([`Session::apply`]).
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
            self.spawns.on_call(owner, id, inp, when);
        } else if is_ask_operator(name) {
            self.asks.on_call(id, owner);
        } else if name == BASH {
            self.background.on_call(id, BgKind::Bash, inp);
        } else if name == MONITOR {
            self.background.on_call(id, BgKind::Monitor, inp);
        } else if BG_STOP_TOOLS.contains(&name) {
            self.background.on_stop_call(id, inp);
        }
    }

    /// A result goes to every store: each keeps what concerns it (`tur`:
    /// the record's `toolUseResult`).
    pub(super) fn tool_result(
        &mut self,
        owner: &Owner,
        call: &str,
        is_error: bool,
        content: Option<&Value>,
        tur: &Map<String, Value>,
        when: Option<f64>,
    ) {
        self.spawns.on_result(call, is_error, tur, when);
        self.workflows.on_result(call, tur, when);
        self.background.on_result(owner, call, tur, is_error, when);
        self.asks.on_result(call, content);
    }
}
