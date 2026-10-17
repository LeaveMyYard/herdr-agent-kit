//! File modification times and JSON object files.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::{Map, Value};

use crate::agent_session::Ts;
use crate::agent_session::claude::wire::Line;

/// A file's modification time in epoch seconds, `0.0` when it cannot be read
/// (panorama's `_mtime`; computed as CPython's `st_mtime`).
pub(super) fn mtime(path: &Path) -> Ts {
    Ts(fs::metadata(path).map_or(0.0, |m| mtime_of(&m)))
}

pub(super) fn mtime_of(meta: &fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |d| {
            d.as_secs() as f64 + f64::from(d.subsec_nanos()) * 1e-9
        })
}

/// A JSON object file (`meta.json`, a run summary): panorama's
/// `json.load(open(path, encoding="utf-8"))`. It goes through [`Line::parse`],
/// so a lone UTF-16 surrogate escape reads as Python reads it; a leading UTF-8
/// BOM does not (Python's `utf-8` codec keeps it and `json` rejects it).
pub(super) fn read_object(path: &Path) -> Option<Map<String, Value>> {
    let data = fs::read(path).ok()?;
    if data.starts_with(b"\xef\xbb\xbf") {
        return None;
    }
    Line::parse(&data).map(Line::into_map)
}
