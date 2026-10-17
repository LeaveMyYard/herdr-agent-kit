//! Reading JSON fields as panorama's Python reads them.

use serde_json::{Map, Value};

use crate::agent_session::claude::wire::{py_str, truthy};

pub(super) fn non_empty<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Python's `str(m.get(key) or "")`.
pub(super) fn str_or_empty(m: &Map<String, Value>, key: &str) -> String {
    match m.get(key) {
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    }
}

/// Python's `isinstance(v, int)` (a bool is an int), as an integer.
pub(super) fn py_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

/// A JSON number (never a bool) as a float.
pub(super) fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}
