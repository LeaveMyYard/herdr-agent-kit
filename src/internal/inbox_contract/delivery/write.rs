//! Writing a delivery (herdr-inbox's `service.py`): the single and batch
//! messages, the long-answer reference and the answer's own one-line form.

use super::{Answered, QUESTION_WIDTH};
use crate::pytext::{py_isspace, py_len, py_line_break};

/// The first line of `text` once it is stripped, cut to `width` code points with a
/// trailing `…` (Python's `service.first_line`: `str.strip`, `str.splitlines`).
pub fn first_line(text: &str, width: usize) -> String {
    let stripped = text.trim_matches(py_isspace);
    let line = match stripped.find(py_line_break) {
        Some(end) => &stripped[..end],
        None => stripped,
    };
    if py_len(line) <= width {
        return line.to_owned();
    }
    // Python's `line[: width - 1]`: a width of 0 slices to -1, all but the last character.
    let keep = match width {
        0 => py_len(line) - 1,
        w => w - 1,
    };
    let mut out: String = line.chars().take(keep).collect();
    out.push('…');
    out
}

/// One question's segment of a batch message: `Q7 <question's first line>: <answer>`.
pub fn answer_line(q: &Answered<'_>) -> String {
    format!(
        "Q{} {}: {}",
        q.id,
        first_line(q.question, QUESTION_WIDTH),
        q.answer
    )
}

/// The message that delivers `qs` together: the single form for one answer, the batch
/// form otherwise (an empty slice gives the bare batch header, as in Python).
pub fn delivery_text(qs: &[Answered<'_>]) -> String {
    if let [q] = qs {
        return format!(
            "[inbox] Q{} answered — {}: {}",
            q.id,
            first_line(q.question, QUESTION_WIDTH),
            q.answer
        );
    }
    let lines: Vec<String> = qs.iter().map(answer_line).collect();
    format!("[inbox] answered — {}", lines.join(" · "))
}

/// What is delivered instead of an answer too long for one message: the single form,
/// with the answer replaced by its length and a pointer to `check_answers`.
pub fn long_answer_reference(q: &Answered<'_>) -> String {
    format!(
        "[inbox] Q{} answered — {}: (long answer, {} chars; call check_answers to read it)",
        q.id,
        first_line(q.question, QUESTION_WIDTH),
        py_len(q.answer)
    )
}

/// One message of a pane's delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The text herdr types or the hook prints.
    pub text: String,
    /// The questions it delivers, in order.
    pub ids: Vec<u64>,
    /// `false` for a [`long_answer_reference`]: the answer itself was not delivered, so
    /// it stays unacknowledged and `check_answers` still returns it.
    pub acknowledged: bool,
}

/// Split one pane's answers into messages of at most `max_arg` code points, in order and
/// never truncated (Python's `service.delivery_messages`; pass [`MAX_ARG`](super::MAX_ARG)).
///
/// Answers are batched greedily. An answer whose single message is over the cap is sent
/// alone as a [`long_answer_reference`] (which itself is not checked against the cap).
pub fn delivery_messages(qs: &[Answered<'_>], max_arg: usize) -> Vec<Message> {
    let mut out = Vec::new();
    let mut batch: Vec<Answered<'_>> = Vec::new();
    let flush = |batch: &[Answered<'_>]| Message {
        text: delivery_text(batch),
        ids: batch.iter().map(|q| q.id).collect(),
        acknowledged: true,
    };
    for q in qs {
        if py_len(&delivery_text(std::slice::from_ref(q))) > max_arg {
            out.push(Message {
                text: long_answer_reference(q),
                ids: vec![q.id],
                acknowledged: false,
            });
            continue;
        }
        if !batch.is_empty() {
            batch.push(*q);
            let fits = py_len(&delivery_text(&batch)) <= max_arg;
            batch.pop();
            if !fits {
                out.push(flush(&batch));
                batch.clear();
            }
        }
        batch.push(*q);
    }
    if !batch.is_empty() {
        out.push(flush(&batch));
    }
    out
}

/// The answer's one-line form: picked labels joined by `, `, each with ` (note: …)` when
/// it has a note, then the free text (Python's `service.format_answer`).
///
/// `notes` maps a label to its note; an empty note is no note. A label holding one of
/// `,"()·` is quoted (`"` escaped as `\"`) so the agent can split the picks; the free
/// text is stripped and dropped when blank.
pub fn format_answer(choices: &[&str], notes: &[(&str, &str)], text: Option<&str>) -> String {
    let note = |label: &str| {
        notes
            .iter()
            .find(|(l, _)| *l == label)
            .map(|(_, n)| *n)
            .filter(|n| !n.is_empty())
    };
    let mut parts: Vec<String> = choices
        .iter()
        .map(|c| match note(c) {
            Some(n) => format!("{} (note: {n})", quote(c)),
            None => quote(c),
        })
        .collect();
    if let Some(t) = text
        .map(|t| t.trim_matches(py_isspace))
        .filter(|t| !t.is_empty())
    {
        parts.push(t.to_owned());
    }
    parts.join(", ")
}

fn quote(label: &str) -> String {
    if label.contains([',', '"', '(', ')', '·']) {
        format!("\"{}\"", label.replace('"', "\\\""))
    } else {
        label.to_owned()
    }
}
