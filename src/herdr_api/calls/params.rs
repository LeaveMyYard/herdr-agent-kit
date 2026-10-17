//! The params choke point: every string param goes through [`clean`] on
//! the way out.

use std::path::Path;

use serde_json::Value;

use crate::herdr_api::sanitize::{MAX_ARG, clean};

/// A string param through the choke point.
pub(super) fn s(text: &str) -> Value {
    Value::String(clean(text, MAX_ARG))
}

pub(super) fn path(p: &Path) -> Value {
    s(&p.to_string_lossy())
}
