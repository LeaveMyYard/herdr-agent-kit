//! Question references (herdr-inbox's `deeplink.question_ref`, `question_id`
//! and `describe`, herdr-inbox#57).

use crate::pytext::py_int_str;

/// A question reference without its `Q` (herdr-inbox's `deeplink.question_ref`).
///
/// The optional `machine:` is split off at the **last** `:`; then one `Q` or `q` right
/// before the number is removed, and nothing else: `qnap:Q12` → `qnap:12`, `box:Q12` →
/// `box:12`, `Q12` and `q12` → `12`, `12` stays, `QQ12` → `Q12` (which is not a number, so
/// [`question_id`] and [`valid`](super::valid) refuse it). Any text is taken; nothing is
/// validated.
pub fn question_ref(value: &str) -> String {
    let (machine, reference) = match value.rfind(':') {
        Some(at) => value.split_at(at + 1),
        None => ("", value),
    };
    let reference = reference.strip_prefix(['Q', 'q']).unwrap_or(reference);
    format!("{machine}{reference}")
}

/// A local question reference (`Q12`, `q12`, `12`) as its id (herdr-inbox's
/// `deeplink.question_id`, the type of `inbox answer`/`dismiss`/`redeliver`/`deliver`'s
/// id arguments); `None` for anything else (`QQ12`, `qnap:Q12`, ``).
///
/// The rest after [`question_ref`] is read as Python's `int()` reads it: surrounding
/// whitespace (Python's `isspace()` set without U+001C to U+001F), one sign and `_`
/// between digits are allowed (`Q 12`, `Q+12` and `1_2` are 12). Unlike Python, the digits are ASCII only, and a negative number (no question
/// has one) or one over `u64::MAX` is `None`.
pub fn question_id(value: &str) -> Option<u64> {
    py_int_str(&question_ref(value))
}

/// How the inbox names a question reference to the operator (herdr-inbox's
/// `deeplink.describe` of a question target): [`question_ref`], then `Q` after the last
/// `:`. `qnap:12` and `qnap:Q12` → `qnap:Q12`, `12` → `Q12`; text that is not a reference
/// is named as it comes (`nap` → `Qnap`, `QQ12` → `QQ12`).
pub fn describe_question(value: &str) -> String {
    let reference = question_ref(value);
    let (machine, number) = match reference.rfind(':') {
        Some(at) => reference.split_at(at + 1),
        None => ("", reference.as_str()),
    };
    format!("{machine}Q{number}")
}
