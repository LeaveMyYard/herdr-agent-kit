//! The `ask_operator` receipt: what the MCP tool returns to the asking agent. **Pure.**
//!
//! The tool returns a JSON object, which the MCP SDK hands to Claude Code as text (and
//! as structured content). The receipt for one question is
//! `{"id", "status", "message"}` (plus `"answer"` when it was already answered); for
//! several, `{"group", "questions": [...], "message"}` with one such entry per question.
//! When the inbox cannot take the question it is `{"status": "unavailable", "message"}`.
//!
//! - Writing: the status names and the message texts, from herdr-inbox's `service.py`
//!   (the JSON around them is the MCP layer's).
//! - Reading: [`question_ids`], herdr-panorama's `scan/wire.py`, which attributes the
//!   questions to the agent that asked them.
//!
//! Goldens: `fixtures/inbox/receipts.json`, results of the inbox's own MCP server.

use std::collections::BTreeSet;

use super::text::{ascii_digits, parse_id, py_space, py_word, trim_start};

/// Status of a newly filed question.
pub const OPEN: &str = "open";
/// Status of a question that is the same as one still open: it is not filed again.
pub const ALREADY_OPEN: &str = "already_open";
/// Status of a question that is the same as one answered recently: the answer is returned.
pub const ANSWERED: &str = "answered";
/// Status when the inbox does not serve the caller, or is being upgraded.
pub const UNAVAILABLE: &str = "unavailable";

/// The message of an [`UNAVAILABLE`] receipt for a caller that is not the top-level
/// agent of a herdr pane (`service.UNAVAILABLE`).
pub const NOT_SERVED_MESSAGE: &str = "The operator inbox only serves the top-level agent of a herdr pane, and this \
     session is not one (not in herdr, or nested inside another agent). Ask in chat, or ask your caller, instead.";

/// The message of an [`UNAVAILABLE`] receipt while the database is not migrated to the
/// tool's schema yet (`service.NOT_MIGRATED`).
pub const NOT_MIGRATED_MESSAGE: &str = "The operator inbox is being upgraded (its delivery worker migrates the \
     database); call this tool again in a few seconds, or ask in chat if it stays unavailable.";

/// The message of an [`OPEN`] entry.
pub fn open_message(id: u64) -> String {
    format!(
        "Recorded as Q{id}. The operator will answer asynchronously; the answer will arrive in this conversation \
         as a message starting with `[inbox]` that names Q{id}. Continue with work that does not depend on it. Do \
         not repeat the question in chat or call ask_operator for it again. If an answer seems overdue, call \
         check_answers."
    )
}

/// The message of an [`ALREADY_OPEN`] entry.
pub fn already_open_message(id: u64) -> String {
    format!("Q{id} is already with the operator. Do not ask again; keep working.")
}

/// The message of an [`ANSWERED`] entry, which also carries `answer` in its own field.
pub fn answered_message(id: u64, answer: &str) -> String {
    format!("Q{id} was already answered by the operator: {answer}")
}

/// The top-level message of a several-question receipt: every entry's id and status.
pub fn filed_message(entries: &[(u64, &str)]) -> String {
    let ids: Vec<String> = entries
        .iter()
        .map(|(id, status)| format!("Q{id} ({status})"))
        .collect();
    format!(
        "Filed {}. Answers arrive later as `[inbox]` messages naming the Q ids; keep working and do not ask these \
         again. If an answer seems overdue, call check_answers.",
        ids.join(", ")
    )
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
    let s = trim_start(s, py_space).strip_prefix(':')?;
    let s = trim_start(s, py_space);
    let digits = ascii_digits(s);
    let s = trim_start(&s[digits.len()..], py_space).strip_prefix(',')?;
    let s = trim_start(s, py_space).strip_prefix("\"status\"")?;
    let s = trim_start(s, py_space).strip_prefix(':')?;
    let s = trim_start(s, py_space).strip_prefix('"')?;
    if digits.is_empty() || !(s.starts_with("open\"") || s.starts_with("already_open\"")) {
        return None;
    }
    parse_id(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_stand_as_words() {
        assert_eq!(
            question_ids("Recorded as Q5. names Q5."),
            BTreeSet::from([5])
        );
        assert!(question_ids("xQ5 Q5x _Q5 Q").is_empty());
        assert_eq!(question_ids("(Q7) Q8,Q9"), BTreeSet::from([7, 8, 9]));
    }

    #[test]
    fn json_entries_count_only_when_open() {
        assert_eq!(
            question_ids(r#"{"id": 3, "status": "open"}"#),
            BTreeSet::from([3])
        );
        assert_eq!(
            question_ids(r#"{"id":4 ,"status" :"already_open"}"#),
            BTreeSet::from([4])
        );
        assert!(question_ids(r#"{"id": 5, "status": "answered"}"#).is_empty());
        assert!(question_ids(r#"{"id": 6, "status": "opened"}"#).is_empty());
    }
}
