//! Python's character classes.

/// Python's `str.isspace()` for one character, which is also what `\s`
/// matches in a `str` pattern: Unicode `White_Space` plus the four
/// information separators U+001C to U+001F (Rust's `char::is_whitespace`
/// lacks those four).
pub(crate) fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// A line boundary of Python's `str.splitlines()` (`\r\n` is two of them
/// back to back).
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

/// Python's `\w` in a `str` pattern, as far as `\b` needs it: letters, digits
/// and `_`.
///
/// Approximation: Rust's alphanumeric set also counts the combining marks that
/// have the Alphabetic property (some vowel signs), which Python does not. No
/// inbox format puts one next to a `Q` id.
pub(crate) fn py_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

#[cfg(test)]
mod tests;
