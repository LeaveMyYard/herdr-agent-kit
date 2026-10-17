//! Reading what an invocation printed: each command's stdout (in text
//! mode), and a non-zero exit as a [`HerdrError`].

use std::path::PathBuf;

use serde_json::Value;

use crate::herdr_api::error::{HerdrError, SERVER_NOT_RUNNING};
use crate::herdr_api::types::{Machine, Snapshot};
use crate::herdr_api::wire;

/// `machine list --json`: a list of machines, or (tolerated, as herdr-inbox
/// does) a `{"result": {"machines": [...]}}` envelope. Empty output is no
/// machines; output that is not JSON is [`HerdrError::Garbled`] (both Python
/// callers read that as no machines). Items that are not objects are
/// skipped; nothing is filtered (`enabled`, a safe `target`: the callers'
/// rules).
pub(crate) fn parse_machines(stdout: &str) -> Result<Vec<Machine>, HerdrError> {
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    let data: Value =
        serde_json::from_str(stdout).map_err(|e| HerdrError::Garbled(e.to_string()))?;
    let items = match data {
        Value::Array(items) => items,
        Value::Object(mut o) => match o
            .remove("result")
            .as_mut()
            .and_then(|r| r.get_mut("machines"))
            .map(Value::take)
        {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter(Value::is_object)
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// `--machine <id> api snapshot`: `result.snapshot`, else Garbled.
pub(crate) fn parse_snapshot(stdout: &str) -> Result<Snapshot, HerdrError> {
    let data: Value = serde_json::from_str(stdout)
        .map_err(|_| HerdrError::Garbled("unreadable snapshot".into()))?;
    match data.pointer("/result/snapshot") {
        Some(snap @ Value::Object(_)) => serde_json::from_value(snap.clone())
            .map_err(|_| HerdrError::Garbled("unreadable snapshot".into())),
        _ => Err(HerdrError::Garbled("no snapshot".into())),
    }
}

/// `plugin config-dir`: the trimmed path, or `None` when herdr printed none.
pub(crate) fn parse_config_dir(stdout: &str) -> Option<PathBuf> {
    let dir = stdout.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// A non-zero exit as an error (Python: `(stderr or stdout).strip() or
/// f"exit {code}"`).
pub(super) fn exit_error(status: Option<i32>, stdout: &[u8], stderr: &[u8]) -> HerdrError {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let text = if stderr.is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let text = if text.is_empty() {
        status.map_or_else(|| "killed by a signal".to_owned(), |c| format!("exit {c}"))
    } else {
        text.to_owned()
    };
    if let Ok(Value::Object(mut reply)) = serde_json::from_str::<Value>(&text)
        && let Some(error) = reply.remove("error")
    {
        let err = wire::refusal(error);
        return if err.is_code(SERVER_NOT_RUNNING) {
            HerdrError::NotRunning
        } else {
            err
        };
    }
    HerdrError::Refused {
        code: String::new(),
        message: text,
    }
}

/// stdout as Python's `text=True` reads it (universal newlines).
pub(super) fn text_mode(stdout: &[u8]) -> String {
    crate::herdr_api::calls::universal_newlines(&String::from_utf8_lossy(stdout))
}
