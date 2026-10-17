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

use std::collections::BTreeSet;

use super::text::{
    ascii_digits, js_space, parse_id, py_len, py_line_break, py_space, trim, trim_end, trim_start,
};

/// herdr's cap on one argument, in code points (`herdr.MAX_ARG` in herdr-inbox): a
/// delivery message longer than this is split, or sent as a reference.
pub const MAX_ARG: usize = 4000;

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

/// The first line of `text` once it is stripped, cut to `width` code points with a
/// trailing `…` (Python's `service.first_line`: `str.strip`, `str.splitlines`).
pub fn first_line(text: &str, width: usize) -> String {
    let stripped = trim(text, py_space);
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
/// never truncated (Python's `service.delivery_messages`; pass [`MAX_ARG`]).
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
    if let Some(t) = text.map(|t| trim(t, py_space)).filter(|t| !t.is_empty()) {
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

/// Python's `transcript._PASTE_WRAPPER` (`^\s*(?:<pasted_content[^>]*>\s*)?`), or the
/// TypeScript one with `space = js_space`: leading space, then one opening paste tag.
fn skip_paste_open(text: &str, space: fn(char) -> bool) -> &str {
    let rest = trim_start(text, space);
    if let Some(tag) = rest.strip_prefix("<pasted_content")
        && let Some(end) = tag.find('>')
    {
        return trim_start(&tag[end + 1..], space);
    }
    rest
}

/// The ids a message names in the inbox's own header, read only where the inbox writes
/// them (Python's `transcript.header_ids`, which decides that a delivery landed).
///
/// `[inbox] Q7 answered — …` names 7. `[inbox] answered — Q7 … · Q8 …` names the first
/// segment's id and every id after ` · ` (and followed by a space). An id merely
/// mentioned elsewhere does not count, nor does a message that only quotes a header. A
/// leading opening paste tag (Claude Code records big pastes wrapped) is skipped.
pub fn header_ids(text: &str) -> BTreeSet<u64> {
    let text = skip_paste_open(text, py_space);
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
    if let Some(id) = rest.strip_prefix('Q').and_then(id_then_space) {
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

/// `(\d+) ` at the start of `s`: the id, when a space follows the digits.
fn id_then_space(s: &str) -> Option<u64> {
    let digits = ascii_digits(s);
    if digits.is_empty() || !s[digits.len()..].starts_with(' ') {
        return None;
    }
    parse_id(digits)
}

/// Each `[inbox] ` inside an asyncRewake task notification, up to the end of its
/// paragraph, the reminder or the notification, right-stripped (Python's
/// `transcript._rewake_texts`).
///
/// Every occurrence starts a text, so one that sits inside an earlier one's paragraph is
/// returned again on its own.
pub fn rewake_texts(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = content[from..].find("[inbox] ") {
        let start = from + at;
        let end = rewake_end(content, start).unwrap_or(content.len());
        out.push(trim_end(&content[start..end], py_space));
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
                .take_while(|&c| py_space(c))
                .any(|c| c == '\n')
        });
    match (tag, blank) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// One answer as the UI mod reads it from a delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAnswer {
    /// The id as written, e.g. `Q12`.
    pub id: String,
    /// The question's first line (empty when the segment has no `: `).
    pub question: String,
    /// The answer.
    pub answer: String,
}

/// The answers a message carries, read only from its start (the herdr-inbox-ui mod's
/// `parseAnswers`, which draws them).
///
/// One opening paste tag before and one closing paste tag after the message are dropped,
/// then control characters (all but tab and newline). A batch is split at each ` · `
/// followed by `Q<digits> `; each segment splits at its first `: ` into the question and
/// the answer, both trimmed. A message that merely quotes a header gives nothing.
pub fn parse_answers(raw: &str) -> Vec<ParsedAnswer> {
    if !raw.contains("[inbox]") {
        return Vec::new();
    }
    let text = js_clean(strip_paste_close(skip_paste_open(raw, js_space)));
    if let Some(rest) = text.strip_prefix("[inbox] Q") {
        let digits = ascii_digits(rest);
        if let Some(seg) = rest[digits.len()..].strip_prefix(" answered — ")
            && !digits.is_empty()
        {
            return vec![split_segment(seg, format!("Q{digits}"))];
        }
    }
    let Some(rest) = text.strip_prefix("[inbox] answered — ") else {
        return Vec::new();
    };
    batch_segments(rest)
        .into_iter()
        .filter_map(|seg| {
            let s = trim_start(seg, js_space).strip_prefix('Q')?;
            let digits = ascii_digits(s);
            let tail = s[digits.len()..].strip_prefix(' ')?;
            (!digits.is_empty()).then(|| split_segment(tail, format!("Q{digits}")))
        })
        .collect()
}

/// The answers of an asyncRewake wake-up: the text after `Stop hook blocking error from
/// command "…": ` up to `</system-reminder>`, read with [`parse_answers`] (the mod's
/// `rewakeAnswers`).
pub fn rewake_answers(text: &str) -> Vec<ParsedAnswer> {
    const LEAD: &str = "Stop hook blocking error from command \"";
    for (at, _) in text.match_indices(LEAD) {
        let after = &text[at + LEAD.len()..];
        let Some(quote) = after.find('"') else {
            continue;
        };
        let Some(body) = after[quote + 1..].strip_prefix(": ") else {
            continue;
        };
        let Some(end) = body.find("</system-reminder>") else {
            continue;
        };
        return parse_answers(trim_end(&body[..end], js_space));
    }
    Vec::new()
}

/// `\s*<\/pasted_content[^>]*>\s*$` removed: a closing paste tag that ends the text.
fn strip_paste_close(text: &str) -> &str {
    let body = trim_end(text, js_space);
    let Some(close) = body
        .len()
        .checked_sub(1)
        .filter(|&i| body.as_bytes()[i] == b'>')
    else {
        return text;
    };
    // the leftmost tag whose attributes run, without a `>`, to that final `>`
    let tag = body
        .match_indices("</pasted_content")
        .map(|(i, _)| i)
        .find(|&i| {
            i + "</pasted_content".len() <= close
                && !body[i + "</pasted_content".len()..close].contains('>')
        });
    match tag {
        Some(i) => trim_end(&text[..i], js_space),
        None => text,
    }
}

/// The mod's `clean`: control characters other than tab and newline are dropped.
fn js_clean(text: &str) -> String {
    text.chars()
        .filter(|&c| !matches!(c, '\u{0}'..='\u{8}' | '\u{0b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
        .collect()
}

/// `rest.split(/ · (?=Q\d+ )/)`.
fn batch_segments(rest: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut seg_start = 0;
    let mut i = 0;
    while let Some(at) = rest[i..].find(" · ") {
        let sep = i + at;
        let after = &rest[sep + " · ".len()..];
        if after
            .strip_prefix('Q')
            .and_then(id_then_space_str)
            .is_some()
        {
            out.push(&rest[seg_start..sep]);
            seg_start = sep + " · ".len();
            i = seg_start;
        } else {
            i = sep + 1;
        }
    }
    out.push(&rest[seg_start..]);
    out
}

/// `\d+ ` at the start of `s` (any length of digits, unlike [`id_then_space`]).
fn id_then_space_str(s: &str) -> Option<()> {
    let digits = ascii_digits(s);
    (!digits.is_empty() && s[digits.len()..].starts_with(' ')).then_some(())
}

fn split_segment(seg: &str, id: String) -> ParsedAnswer {
    match seg.find(": ") {
        Some(at) => ParsedAnswer {
            id,
            question: trim(&seg[..at], js_space).to_owned(),
            answer: trim(&seg[at + 2..], js_space).to_owned(),
        },
        None => ParsedAnswer {
            id,
            question: String::new(),
            answer: trim(seg, js_space).to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a<'a>(id: u64, question: &'a str, answer: &'a str) -> Answered<'a> {
        Answered {
            id,
            question,
            answer,
        }
    }

    #[test]
    fn single_and_batch_round_trip() {
        let one = delivery_text(&[a(12, "Which DB?", "Postgres")]);
        assert_eq!(one, "[inbox] Q12 answered — Which DB?: Postgres");
        assert_eq!(header_ids(&one), BTreeSet::from([12]));
        let two = delivery_text(&[a(1, "a?", "b"), a(2, "c?", "d")]);
        assert_eq!(two, "[inbox] answered — Q1 a?: b · Q2 c?: d");
        assert_eq!(header_ids(&two), BTreeSet::from([1, 2]));
        let parsed = parse_answers(&two);
        assert_eq!(
            parsed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["Q1", "Q2"]
        );
        assert_eq!(
            (parsed[1].question.as_str(), parsed[1].answer.as_str()),
            ("c?", "d")
        );
    }

    #[test]
    fn a_quoted_header_is_not_a_delivery() {
        let quoted = "why did you get \"[inbox] Q12 answered — Which DB?: Postgres\" twice?";
        assert!(header_ids(quoted).is_empty());
        assert!(parse_answers(quoted).is_empty());
    }

    #[test]
    fn first_line_counts_code_points() {
        assert_eq!(first_line(&"\u{1F680}".repeat(5), 3), "\u{1F680}\u{1F680}…");
        assert_eq!(first_line("hello", 0), "hell…");
        assert_eq!(first_line(" \u{1c}a\u{1d}b", 80), "a");
    }

    #[test]
    fn an_over_long_answer_goes_alone_as_a_reference() {
        let long = "x".repeat(MAX_ARG);
        let qs = [a(1, "a?", "b"), a(2, "Long?", &long), a(3, "c?", "d")];
        let msgs = delivery_messages(&qs, MAX_ARG);
        assert_eq!(
            msgs.iter()
                .map(|m| (m.ids.clone(), m.acknowledged))
                .collect::<Vec<_>>(),
            [(vec![2], false), (vec![1, 3], true)]
        );
        assert!(msgs[0].text.contains("(long answer, 4000 chars;"));
    }
}
