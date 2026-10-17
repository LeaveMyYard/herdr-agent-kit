//! Python's string operations.

use super::chars::{py_isspace, py_line_break};

/// Python's `str.strip()`.
pub(crate) fn py_strip(s: &str) -> &str {
    s.trim_matches(py_isspace)
}

/// Python's `bytes.strip()` is empty: the line is blank (`b" \t\n\r\x0b\x0c"`).
pub(crate) fn blank(raw: &[u8]) -> bool {
    raw.iter()
        .all(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
}

/// Python's `str.splitlines()` (without the line ends).
pub(crate) fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if !py_line_break(c) {
            continue;
        }
        let end = if c == '\r' && matches!(iter.peek(), Some(&(_, '\n'))) {
            iter.next();
            i + 2
        } else {
            i + c.len_utf8()
        };
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python's `len()` of a `str`: code points, not bytes.
#[cfg(feature = "internal")]
pub(crate) fn py_len(s: &str) -> usize {
    s.chars().count()
}

/// The run of ASCII digits at the start of `s` (possibly empty).
///
/// Python's `\d` also matches other scripts' decimal digits and TypeScript's
/// does not. The inbox writes ids as `Q{int}`, always ASCII, so the ports
/// follow TypeScript here.
pub(crate) fn ascii_digits(s: &str) -> &str {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    &s[..end]
}

/// Python's `int()` of a run of ASCII digits; `None` when it does not fit a
/// `u64` (Python's ints have no limit: such an id is dropped, never wrapped).
pub(crate) fn parse_id(digits: &str) -> Option<u64> {
    digits.parse().ok()
}

/// Python's `int(s)` for an ASCII decimal that fits a `u64`.
#[cfg(feature = "internal")]
pub(crate) fn py_int_str(s: &str) -> Option<u64> {
    let s = s.trim_matches(py_int_space);
    let (negative, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    // digits, with single underscores only between two digits
    let ok = !s.is_empty()
        && !s.starts_with('_')
        && !s.ends_with('_')
        && !s.contains("__")
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'_');
    if !ok {
        return None;
    }
    let n: u64 = s.replace('_', "").parse().ok()?;
    (!negative || n == 0).then_some(n)
}

/// What Python's `int()` strips around a number: `str.isspace()` except the information
/// separators U+001C to U+001F, which are ASCII and so not mapped to a space before the
/// ASCII-only strip (CPython `_PyUnicode_TransformDecimalAndSpaceToASCII`).
#[cfg(feature = "internal")]
fn py_int_space(c: char) -> bool {
    py_isspace(c) && !matches!(c, '\u{1c}'..='\u{1f}')
}

#[cfg(test)]
mod tests;
