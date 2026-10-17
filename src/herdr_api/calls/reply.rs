//! Reading replies: the shared parsers the calls are built from.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::herdr_api::error::HerdrError;
use crate::herdr_api::types::{AgentInfo, Extra, LayoutInfo, PaneInfo, TabInfo};

pub(super) fn field<T: DeserializeOwned>(mut result: Value, key: &str) -> Result<T, HerdrError> {
    match result.get_mut(key).map(Value::take) {
        Some(v) if !v.is_null() => {
            serde_json::from_value(v).map_err(|e| HerdrError::Garbled(format!("`{key}`: {e}")))
        }
        _ => Err(HerdrError::Garbled(format!("no `{key}` in the reply"))),
    }
}

/// A list field; missing reads as empty (Python's `res.get(key, [])`), items
/// that do not fit are skipped.
pub(super) fn list<T: DeserializeOwned>(mut result: Value, key: &str) -> Vec<T> {
    match result.get_mut(key).map(Value::take) {
        Some(Value::Array(items)) => items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn unit(r: Result<Value, HerdrError>) -> Result<(), HerdrError> {
    r.map(drop)
}

pub(super) fn raw(r: Result<Value, HerdrError>) -> Result<Value, HerdrError> {
    r
}

pub(super) fn extra(r: Result<Value, HerdrError>) -> Result<Extra, HerdrError> {
    match r? {
        Value::Object(m) => Ok(m),
        _ => Ok(Map::new()),
    }
}

/// What herdr echoes back for an action it did: `None` when the echo is
/// missing or does not fit. herdr confirmed the action either way (no
/// `error`), so this is never [`HerdrError::Garbled`] (which reads as "maybe
/// done"); the Python callers only checked for an error here.
pub(super) fn echo<T: DeserializeOwned>(
    r: Result<Value, HerdrError>,
    key: &str,
) -> Result<Option<T>, HerdrError> {
    Ok(field(r?, key).ok())
}

pub(super) fn tab_info(r: Result<Value, HerdrError>) -> Result<TabInfo, HerdrError> {
    field(r?, "tab")
}

pub(super) fn pane_info(r: Result<Value, HerdrError>) -> Result<PaneInfo, HerdrError> {
    field(r?, "pane")
}

pub(super) fn agent_info(r: Result<Value, HerdrError>) -> Result<AgentInfo, HerdrError> {
    field(r?, "agent")
}

pub(super) fn layout(r: Result<Value, HerdrError>) -> Result<LayoutInfo, HerdrError> {
    field(r?, "layout")
}
