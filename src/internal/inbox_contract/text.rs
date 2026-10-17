//! String rules of the two languages the formats were written in. **Pure.**
//!
//! herdr-inbox writes and reads its formats in Python and its UI mod reads them in
//! TypeScript; the two disagree on what whitespace is. Each port names the rule it
//! follows instead of using Rust's own (`char::is_whitespace` differs from both).

/// Python's `str.isspace()`, which is also what `\s` matches in a `str` pattern.
///
/// Unlike Rust's `char::is_whitespace` it includes the information separators
/// U+001C to U+001F.
pub(crate) fn py_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// JavaScript's `\s` and `String.prototype.trim` set (WhiteSpace plus LineTerminator).
///
/// Unlike Python's it has U+FEFF and lacks U+0085 and U+001C to U+001F.
pub(crate) fn js_space(c: char) -> bool {
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

/// A line boundary of Python's `str.splitlines()` (`\r\n` is two of them back to back).
pub(crate) fn py_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r'
            | '\u{0b}'
            | '\u{0c}'
            | '\u{1c}'
            | '\u{1d}'
            | '\u{1e}'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// Python's `\w` in a `str` pattern, as far as `\b` needs it: letters, digits and `_`.
///
/// Approximation: Rust's alphanumeric set also counts the combining marks that have the
/// Alphabetic property (some vowel signs), which Python does not. No inbox format puts
/// one next to a `Q` id.
pub(crate) fn py_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// `s` without leading characters for which `space` holds.
pub(crate) fn trim_start(s: &str, space: fn(char) -> bool) -> &str {
    s.trim_start_matches(space)
}

/// `s` without trailing characters for which `space` holds.
pub(crate) fn trim_end(s: &str, space: fn(char) -> bool) -> &str {
    s.trim_end_matches(space)
}

/// `s` without leading and trailing characters for which `space` holds.
pub(crate) fn trim(s: &str, space: fn(char) -> bool) -> &str {
    s.trim_matches(space)
}

/// The run of ASCII digits at the start of `s` (possibly empty).
///
/// Python's `\d` also matches other scripts' decimal digits and TypeScript's does not. The
/// inbox writes ids as `Q{int}`, always ASCII, so the ports follow TypeScript here.
pub(crate) fn ascii_digits(s: &str) -> &str {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    &s[..end]
}

/// `(\d+) ` at the start of `s`: the digits, when a space follows them (any number of
/// digits; [`parse_id`] decides whether they make an id).
pub(crate) fn digits_then_space(s: &str) -> Option<&str> {
    let digits = ascii_digits(s);
    (!digits.is_empty() && s[digits.len()..].starts_with(' ')).then_some(digits)
}

/// Python's `int()` of a run of ASCII digits; `None` when it does not fit a `u64`
/// (Python's ints have no limit: such an id is dropped, never wrapped).
pub(crate) fn parse_id(digits: &str) -> Option<u64> {
    digits.parse().ok()
}

/// Python's `len()` of a `str`: code points, not bytes.
pub(crate) fn py_len(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests;
