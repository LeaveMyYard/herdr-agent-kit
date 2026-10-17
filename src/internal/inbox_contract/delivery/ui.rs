//! Reading a delivery as the herdr-inbox-ui mod draws it (`register.tsx`).

use super::{digits_then_space, skip_paste_open};
use crate::pytext::ascii_digits;

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
            let s = seg.trim_start_matches(js_space).strip_prefix('Q')?;
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
        return parse_answers(body[..end].trim_end_matches(js_space));
    }
    Vec::new()
}

/// `\s*<\/pasted_content[^>]*>\s*$` removed: a closing paste tag that ends the text.
fn strip_paste_close(text: &str) -> &str {
    let body = text.trim_end_matches(js_space);
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
        Some(i) => text[..i].trim_end_matches(js_space),
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
            .and_then(digits_then_space)
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

fn split_segment(seg: &str, id: String) -> ParsedAnswer {
    match seg.find(": ") {
        Some(at) => ParsedAnswer {
            id,
            question: seg[..at].trim_matches(js_space).to_owned(),
            answer: seg[at + 2..].trim_matches(js_space).to_owned(),
        },
        None => ParsedAnswer {
            id,
            question: String::new(),
            answer: seg.trim_matches(js_space).to_owned(),
        },
    }
}

/// JavaScript's `\s` and `String.prototype.trim` set (WhiteSpace plus LineTerminator).
///
/// Unlike Python's it has U+FEFF and lacks U+0085 and U+001C to U+001F.
pub(super) fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
