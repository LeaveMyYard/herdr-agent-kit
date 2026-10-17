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
mod tests {
    use super::*;

    /// panorama tests/test_herdr.py::test_clean_strips_controls_and_caps
    #[test]
    fn clean_strips_controls_and_caps() {
        assert_eq!(clean("a\x1b[31mb\tc\nd\x07", MAX_ARG), "a[31mb c d");
        assert_eq!(clean(&"x".repeat(5000), MAX_ARG).chars().count(), 4000);
    }

    /// Every input in `fixtures/herdr/clean/clean.jsonl`, captured from both
    /// Python plugins, gives the same output here.
    #[test]
    fn clean_matches_the_captured_python_outputs() {
        let goldens = include_str!("../../fixtures/herdr/clean/clean.jsonl");
        let mut n = 0;
        for line in goldens.lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let text = row["text"].as_str().unwrap();
            let limit = row["limit"].as_u64().map_or(MAX_ARG, |l| l as usize);
            assert_eq!(
                clean(text, limit),
                row["out"].as_str().unwrap(),
                "input {text:?} limit {limit}"
            );
            n += 1;
        }
        assert!(n > 200, "{n} goldens");
    }

    #[test]
    fn clean_is_idempotent_on_its_own_output() {
        for text in ["a\x1b[1mb", &"y".repeat(4001), "a\u{2028}b"] {
            let once = clean(text, MAX_ARG);
            assert_eq!(clean(&once, MAX_ARG), once);
        }
    }

    #[test]
    fn sanitized_is_built_only_through_clean() {
        assert_eq!(Sanitized::new("a\nb\x1b").as_str(), "a b");
        assert_eq!(Sanitized::with_limit("abcdef", 3).as_str(), "ab…");
        assert_eq!(Sanitized::from("x\ty".to_string()).to_string(), "x y");
    }
}
