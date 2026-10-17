//! Reading a delivery as the herdr-inbox-ui mod draws it (`register.tsx`).

use super::skip_paste_open;
use crate::internal::inbox_contract::text::{ascii_digits, js_space, trim, trim_end, trim_start};

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

/// `\d+ ` at the start of `s` (any length of digits, unlike `transcript`'s
/// `id_then_space`).
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
