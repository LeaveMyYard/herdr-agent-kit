//! Reading a delivery back from a transcript (herdr-inbox's `transcript.py`,
//! which decides whether an answer landed).

use std::collections::BTreeSet;

use super::{digits_then_space, skip_paste_open};
use crate::pytext::{ascii_digits, parse_id, py_isspace};

/// The ids a message names in the inbox's own header, read only where the inbox writes
/// them (Python's `transcript.header_ids`, which decides that a delivery landed).
///
/// `[inbox] Q7 answered — …` names 7. `[inbox] answered — Q7 … · Q8 …` names the first
/// segment's id and every id after ` · ` (and followed by a space). An id merely
/// mentioned elsewhere does not count, nor does a message that only quotes a header. A
/// leading opening paste tag (Claude Code records big pastes wrapped) is skipped.
pub fn header_ids(text: &str) -> BTreeSet<u64> {
    let text = skip_paste_open(text, py_isspace);
    let mut ids = BTreeSet::new();
    if let Some(rest) = text.strip_prefix("[inbox] Q") {
        let digits = ascii_digits(rest);
        if !digits.is_empty() && rest[digits.len()..].starts_with(" answered — ") {
            ids.extend(parse_id(digits));
            return ids;
        }
    }
    let Some(rest) = text.strip_prefix("[inbox] answered — ") else {
        return ids;
    };
    if let Some(id) = rest
        .strip_prefix('Q')
        .and_then(digits_then_space)
        .and_then(parse_id)
    {
        ids.insert(id);
    }
    // ` · Q(\d+) ` with finditer: non-overlapping, scanning on after each match.
    let mut i = 0;
    while let Some(at) = rest[i..].find(" · Q") {
        let start = i + at + " · Q".len();
        let digits = ascii_digits(&rest[start..]);
        if !digits.is_empty() && rest[start + digits.len()..].starts_with(' ') {
            ids.extend(parse_id(digits));
            i = start + digits.len() + 1;
        } else {
            i += at + 1;
        }
    }
    ids
}

/// Each `[inbox] ` inside an asyncRewake task notification, up to the end of its
/// paragraph, the reminder or the notification, right-stripped (Python's
/// `transcript._rewake_texts`).
///
/// Every occurrence starts a text, so one that sits inside an earlier one's paragraph is
/// returned again on its own.
///
/// Python applies it only to a `user` record's content that contains
/// `<task-notification>` (`transcript._record_texts`); elsewhere an `[inbox] ` inside a
/// text is not a delivery.
pub fn rewake_texts(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = content[from..].find("[inbox] ") {
        let start = from + at;
        let end = rewake_end(content, start).unwrap_or(content.len());
        out.push(content[start..end].trim_end_matches(py_isspace));
        from = start + 1;
    }
    out
}

/// The earliest match at or after `from` of `\n\s*\n|</system-reminder>|</task-notification>`.
fn rewake_end(content: &str, from: usize) -> Option<usize> {
    let tag = ["</system-reminder>", "</task-notification>"]
        .iter()
        .filter_map(|t| content[from..].find(t).map(|i| from + i))
        .min();
    let blank = content[from..]
        .match_indices('\n')
        .map(|(i, _)| from + i)
        .find(|&i| {
            // a newline, then whitespace that holds another newline
            content[i + 1..]
                .chars()
                .take_while(|&c| py_isspace(c))
                .any(|c| c == '\n')
        });
    match (tag, blank) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
