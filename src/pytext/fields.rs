//! Reading one field of a JSON object as panorama's Python reads it.

use serde_json::{Map, Value};

use super::values::{py_str, truthy};

/// The field as a string, when it is a non-empty string (`m.get(key)` that
/// is a `str` and truthy).
pub(crate) fn non_empty<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Python's `str(m.get(key) or "")`.
pub(crate) fn str_or_empty(m: &Map<String, Value>, key: &str) -> String {
    match m.get(key) {
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    }
}

/// Python's `isinstance(v, int)` (a bool is an int), as an integer.
pub(crate) fn py_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

/// A JSON number (never a bool) as a float.
pub(crate) fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}
