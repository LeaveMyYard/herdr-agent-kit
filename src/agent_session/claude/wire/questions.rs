//! The inbox question ids an `ask_operator` result names.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

struct QuestionRes {
    qid: Regex,
    qid_json: Regex,
}

fn question_res() -> &'static QuestionRes {
    static R: OnceLock<QuestionRes> = OnceLock::new();
    R.get_or_init(|| QuestionRes {
        qid: Regex::new(r"(?:Recorded as Q|\bQ)([0-9]+)\b").expect("qid"),
        qid_json: Regex::new(r#""id"\s*:\s*([0-9]+)\s*,\s*"status"\s*:\s*"(?:open|already_open)""#)
            .expect("qid json"),
    })
}

/// The inbox question ids an `ask_operator` result names (`Recorded as Q12`,
/// `Q12`, or a JSON receipt's `"id": 12, "status": "open"`): panorama's
/// `wire.question_ids`. Only ASCII digits are read (Python's `\d` also takes
/// other Unicode digits), and an id beyond `u64` is skipped.
pub fn question_ids(text: &str) -> BTreeSet<u64> {
    let r = question_res();
    r.qid
        .captures_iter(text)
        .chain(r.qid_json.captures_iter(text))
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

#[cfg(test)]
mod tests;
