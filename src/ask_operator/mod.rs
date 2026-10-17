//! herdr-inbox's `ask_operator` tool as a Claude Code transcript shows it:
//! which tool calls are asks, and which question ids their results name.
//! **Pure; crate-private.**
//!
//! Inbox-specific, so outside the public Claude API: the fold records the ids
//! per asker, read through `Session::qids` (feature `internal`), and the
//! inbox's receipt format re-exports [`question_ids`]
//! (`internal::inbox_contract::receipt::question_ids`).

use std::collections::BTreeSet;

use crate::pytext::{ascii_digits, parse_id, py_isspace, py_word};

/// A tool call is an `ask_operator` call: the MCP tool name ends with
/// `ask_operator` (`mcp__plugin_herdr-inbox_herdr-inbox__ask_operator`, or
/// the server's name under another install), as panorama's
/// `name.endswith("ask_operator")`.
pub(crate) fn is_ask_operator(tool_name: &str) -> bool {
    tool_name.ends_with("ask_operator")
}

/// The question ids a receipt names (herdr-panorama's `wire.question_ids`).
///
/// Two readings, joined: every `Q<digits>` that stands as a word (not preceded or followed
/// by a letter, digit or `_`), and every JSON `"id": <n>, "status": "open"` or
/// `"already_open"`. A receipt that names no question (an error, unavailable) gives none.
pub fn question_ids(text: &str) -> BTreeSet<u64> {
    let mut ids = BTreeSet::new();
    // (?:Recorded as Q|\bQ)(\d+)\b: the first form is always also the second.
    for (at, _) in text.match_indices('Q') {
        if text[..at].chars().next_back().is_some_and(py_word) {
            continue;
        }
        let rest = &text[at + 1..];
        let digits = ascii_digits(rest);
        if !digits.is_empty() && !rest[digits.len()..].chars().next().is_some_and(py_word) {
            ids.extend(parse_id(digits));
        }
    }
    // "id"\s*:\s*(\d+)\s*,\s*"status"\s*:\s*"(?:open|already_open)"
    for (at, _) in text.match_indices("\"id\"") {
        ids.extend(json_open_id(&text[at + 4..]));
    }
    ids
}

fn json_open_id(s: &str) -> Option<u64> {
    let s = s.trim_start_matches(py_isspace).strip_prefix(':')?;
    let s = s.trim_start_matches(py_isspace);
    let digits = ascii_digits(s);
    let s = s[digits.len()..]
        .trim_start_matches(py_isspace)
        .strip_prefix(',')?;
    let s = s
        .trim_start_matches(py_isspace)
        .strip_prefix("\"status\"")?;
    let s = s.trim_start_matches(py_isspace).strip_prefix(':')?;
    let s = s.trim_start_matches(py_isspace).strip_prefix('"')?;
    if digits.is_empty() || !(s.starts_with("open\"") || s.starts_with("already_open\"")) {
        return None;
    }
    parse_id(digits)
}

#[cfg(test)]
mod tests;
