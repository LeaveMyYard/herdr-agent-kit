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
//!   questions to the agent that asked them. It is the crate's one implementation, the
//!   same the session fold records per asker.
//!
//! Goldens: `fixtures/inbox/receipts.json`, results of the inbox's own MCP server.

pub use crate::ask_operator::question_ids;

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
