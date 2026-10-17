//! [`clean`] and [`Sanitized`]: text that is safe to hand to herdr.
//!
//! herdr may type what it is given into a terminal (`agent.prompt`), so no
//! escape or key sequence may survive. This is the Python `herdr.clean` of
//! both plugins (they are identical); `fixtures/herdr/clean/clean.jsonl` holds
//! its captured outputs and the tests check this port against every line.

use std::fmt;

/// The default cap, in characters (Python's `MAX_ARG`).
pub const MAX_ARG: usize = 4000;

/// Line and tab breaks: each becomes one space (Python's `_SPACE`).
fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{2028}' | '\u{2029}' | '\u{85}'
    )
}

/// Every other C0 and C1 control, and DEL: dropped (Python's `_CONTROL`).
fn is_control(c: char) -> bool {
    matches!(c, '\u{00}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
}

/// Make text safe to show in a terminal or pass to herdr.
///
/// Line and tab breaks (`\t \n \r \v \f`, U+2028, U+2029, U+0085) become
/// spaces; every other C0/C1 control (ESC, BEL, CSI, ...) and DEL is dropped,
/// so no escape or key sequence survives. The result is capped at `limit`
/// characters (Unicode scalar values, as Python counts them): longer text
/// keeps its first `limit - 1` characters plus `…`.
///
/// `limit` 0 behaves as Python's slice `text[:-1]` does: all but the last
/// character, plus `…`. Nothing uses it; it is kept for exact parity.
pub fn clean(text: &str, limit: usize) -> String {
    let cleaned: String = text
        .chars()
        .filter_map(|c| {
            if is_space(c) {
                Some(' ')
            } else if is_control(c) {
                None
            } else {
                Some(c)
            }
        })
        .collect();
    let len = cleaned.chars().count();
    if len <= limit {
        return cleaned;
    }
    let keep = if limit == 0 { len - 1 } else { limit - 1 };
    let mut out: String = cleaned.chars().take(keep).collect();
    out.push('…');
    out
}

/// Text that went through [`clean`]: the only way to build a prompt or a
/// label for herdr. A type rule, not a convention.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Sanitized(String);

impl Sanitized {
    /// `clean(text, MAX_ARG)`.
    pub fn new(text: &str) -> Self {
        Self(clean(text, MAX_ARG))
    }

    /// `clean(text, limit)`, for the places that cap shorter or longer
    /// (a tab label at 200, the inbox hook's text at 10^9).
    pub fn with_limit(text: &str, limit: usize) -> Self {
        Self(clean(text, limit))
    }

    /// The cleaned text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The cleaned text, owned.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Sanitized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Sanitized {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Sanitized {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Sanitized {
    fn from(text: String) -> Self {
        Self::new(&text)
    }
}

#[cfg(test)]
mod tests;
