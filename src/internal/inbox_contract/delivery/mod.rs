//! The `[inbox] …` delivery text: how an answer reaches the asking conversation. **Pure.**
//!
//! One answer is delivered as `[inbox] Q7 answered — <question's first line>: <answer>`,
//! several to the same pane as one message, `[inbox] answered — Q7 <q>: <a> · Q8 <q>: <a>`.
//! An answer too long for herdr's argument cap is delivered as a reference that tells the
//! agent to call `check_answers`.
//!
//! - Writing: [`delivery_text`], [`answer_line`], [`long_answer_reference`],
//!   [`delivery_messages`] and [`format_answer`] (the answer's own one-line form), from
//!   herdr-inbox's `service.py`.
//! - Reading: [`header_ids`] and [`rewake_texts`] (herdr-inbox's `transcript.py`, which
//!   decides whether an answer landed), and [`parse_answers`] and [`rewake_answers`] (the
//!   herdr-inbox-ui mod's `register.tsx`, which draws them).
//!
//! The Python and TypeScript readers differ on purpose in places (whitespace sets, what
//! counts as a segment); each function says which one it follows. Lengths are counted in
//! code points, as Python's `len` does. Ids are ASCII digits (see the crate's goldens in
//! `fixtures/inbox/delivery.json`, captured from both).

use crate::pytext::ascii_digits;

mod transcript;
mod ui;
mod write;

pub use transcript::{header_ids, rewake_texts};
pub use ui::{ParsedAnswer, parse_answers, rewake_answers};
pub use write::{
    Message, answer_line, delivery_messages, delivery_text, first_line, format_answer,
    long_answer_reference,
};

/// herdr's cap on one argument, in code points (`herdr.MAX_ARG` in herdr-inbox): a
/// delivery message longer than this is split, or sent as a reference.
pub use crate::herdr_api::MAX_ARG;

/// How much of a question's first line a delivery quotes, in code points.
pub const QUESTION_WIDTH: usize = 80;

/// An answered question, as much of it as its delivery shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answered<'a> {
    /// The question's id (`Q{id}` in the text).
    pub id: u64,
    /// The question as asked (only its first line is shown).
    pub question: &'a str,
    /// The operator's answer, as [`format_answer`] wrote it.
    pub answer: &'a str,
}

/// Python's `transcript._PASTE_WRAPPER` (`^\s*(?:<pasted_content[^>]*>\s*)?`), or the
/// TypeScript one with `space = js_space`: leading space, then one opening paste tag.
fn skip_paste_open(text: &str, space: fn(char) -> bool) -> &str {
    let rest = text.trim_start_matches(space);
    if let Some(tag) = rest.strip_prefix("<pasted_content")
        && let Some(end) = tag.find('>')
    {
        return tag[end + 1..].trim_start_matches(space);
    }
    rest
}

/// `(\d+) ` at the start of `s`: the digits, when a space follows them (any number of
/// digits; [`parse_id`](crate::pytext::parse_id) decides whether they make an id).
fn digits_then_space(s: &str) -> Option<&str> {
    let digits = ascii_digits(s);
    (!digits.is_empty() && s[digits.len()..].starts_with(' ')).then_some(digits)
}

#[cfg(test)]
mod tests;
