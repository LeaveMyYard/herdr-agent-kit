//! TOML 1.0, strictly: what Python's `tomllib` accepts. **Pure**.
//!
//! The `toml` crate parses TOML 1.1; `tomllib` (Python 3.11 to 3.13, which the
//! Python plugins run on) parses 1.0 only. While a Python and a Rust process
//! read the same `config.toml` (plan §5.4), the file must mean the same to
//! both, so [`parse`] first parses with the `toml` crate and then refuses the
//! TOML 1.1 additions the crate accepted:
//!
//! - a newline or a comment inside an inline table (`{` … `}`), arrays inside
//!   it excepted;
//! - a trailing comma in an inline table (`{ a = 1, }`);
//! - the `\e` and `\xHH` escapes in basic strings and quoted keys;
//! - a time without seconds (`07:32`, `1979-05-27T07:32Z`);
//!
//! and what `tomllib` refuses beyond the spec, as Python's `datetime` cannot
//! hold it: second 60 (a leap second) and year 0; and a leading byte-order
//! mark (`tomllib` decodes the file as plain UTF-8 and then fails on the
//! U+FEFF). Like `tomllib`, it reads every CRLF as LF first, so a multi-line
//! string holds `\n` where the file has CRLF.
//!
//! The goldens in `fixtures/toml10/cases.json` were captured from `tomllib`
//! (`fixtures/toml10/capture/capture_tomllib.py`) and pin that every input
//! `tomllib` rejects is rejected here, and that what it accepts parses to the
//! same values. Two inputs still differ, the other way round: integers outside
//! the 64-bit range (`tomllib` keeps them as Python integers, the `toml` crate
//! refuses them) and a file that is not UTF-8 (`tomllib` raises
//! `UnicodeDecodeError`, which the plugins do not catch).

use toml::Table;
use toml_parser::decoder::Encoding;
use toml_parser::parser::{EventReceiver, parse_document};
use toml_parser::{ErrorSink, Source, Span};

/// Parses `text` as `tomllib` does. The error is one line; when it is a
/// construct the `toml` crate accepts and `tomllib` refuses, it says which and
/// where, as `tomllib` counts lines and columns.
pub fn parse(text: &str) -> Result<Table, String> {
    // tomllib: `src = s.replace("\r\n", "\n")` before anything else.
    let text = text.replace("\r\n", "\n");
    let table = text
        .parse::<Table>()
        .map_err(|e| one_line(&e.to_string()))?;
    match first_refused(&text) {
        None => Ok(table),
        Some((what, at)) => {
            let (line, column) = line_column(&text, at);
            Err(format!(
                "{what}, which TOML 1.0 (Python's tomllib) refuses (at line {line}, column {column})"
            ))
        }
    }
}

/// The first construct in `text` (which the `toml` crate already parsed) that
/// `tomllib` refuses, and its byte offset.
fn first_refused(text: &str) -> Option<(&'static str, usize)> {
    if text.starts_with('\u{feff}') {
        return Some(("a byte-order mark", 0));
    }
    let tokens = Source::new(text).lex().into_vec();
    let mut finder = Finder {
        text,
        nesting: Vec::new(),
        after_comma: false,
        found: None,
    };
    parse_document(&tokens, &mut finder, &mut IgnoreErrors);
    finder.found
}

struct IgnoreErrors;

impl ErrorSink for IgnoreErrors {
    fn report_error(&mut self, _error: toml_parser::ParseError) {}
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    InlineTable,
    Array,
}

struct Finder<'t> {
    text: &'t str,
    /// The open inline tables and arrays, innermost last.
    nesting: Vec<Open>,
    /// The last event other than whitespace was a comma in an inline table.
    after_comma: bool,
    found: Option<(&'static str, usize)>,
}

impl Finder<'_> {
    fn flag(&mut self, what: &'static str, span: Span) {
        if self.found.is_none() {
            self.found = Some((what, span.start()));
        }
    }

    fn in_inline_table(&self) -> bool {
        self.nesting.last() == Some(&Open::InlineTable)
    }

    fn raw(&self, span: Span) -> &str {
        self.text.get(span.start()..span.end()).unwrap_or("")
    }

    /// A basic string or quoted key: `\e` and `\x` are TOML 1.1 escapes.
    fn check_escapes(&mut self, span: Span, kind: Option<Encoding>) {
        if !matches!(kind, Some(Encoding::BasicString | Encoding::MlBasicString)) {
            return;
        }
        let mut chars = self.raw(span).char_indices();
        while let Some((_, c)) = chars.next() {
            if c != '\\' {
                continue;
            }
            match chars.next() {
                Some((i, 'e')) => {
                    self.flag("the TOML 1.1 escape \\e", shifted(span, i - 1));
                    return;
                }
                Some((i, 'x')) => {
                    self.flag("the TOML 1.1 escape \\x", shifted(span, i - 1));
                    return;
                }
                _ => {}
            }
        }
    }

    /// An unquoted value that is a date, a time or both (the `toml` crate
    /// parsed it, so its fields are in place): `HH:MM:SS` in TOML 1.0, and
    /// neither second 60 nor year 0 in Python's `datetime`.
    fn check_datetime(&mut self, span: Span) {
        let raw = self.raw(span).as_bytes();
        let has_date = raw.get(4) == Some(&b'-');
        if has_date && raw.starts_with(b"0000") {
            self.flag("year 0", span);
            return;
        }
        // A time starts the value or follows the date's `T`, `t` or space.
        let start = match raw.get(10) {
            Some(b'T' | b't' | b' ') if has_date => 11,
            _ if has_date => return,
            _ => 0,
        };
        if raw.get(start + 2) != Some(&b':') {
            return; // a number
        }
        if raw.get(start + 5) != Some(&b':') {
            self.flag("a TOML 1.1 time without seconds", span);
        } else if raw.get(start + 6..start + 8) == Some(b"60") {
            self.flag("second 60", span);
        }
    }

    fn significant(&mut self) {
        self.after_comma = false;
    }
}

fn shifted(span: Span, by: usize) -> Span {
    Span::new_unchecked(span.start() + by, span.end())
}

impl EventReceiver for Finder<'_> {
    fn std_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.significant();
    }
    fn array_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.significant();
    }
    fn inline_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.significant();
        self.nesting.push(Open::InlineTable);
        true
    }
    fn inline_table_close(&mut self, span: Span, _error: &mut dyn ErrorSink) {
        if self.after_comma {
            self.flag("a TOML 1.1 trailing comma in an inline table", span);
        }
        self.significant();
        self.nesting.pop();
    }
    fn array_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.significant();
        self.nesting.push(Open::Array);
        true
    }
    fn array_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.significant();
        self.nesting.pop();
    }
    fn simple_key(&mut self, span: Span, kind: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.significant();
        self.check_escapes(span, kind);
    }
    fn key_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.significant();
    }
    fn key_val_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.significant();
    }
    fn scalar(&mut self, span: Span, kind: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.significant();
        match kind {
            Some(_) => self.check_escapes(span, kind),
            None => self.check_datetime(span),
        }
    }
    fn value_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.after_comma = self.in_inline_table();
    }
    fn comment(&mut self, span: Span, _error: &mut dyn ErrorSink) {
        if self.in_inline_table() {
            self.flag("a TOML 1.1 comment in an inline table", span);
        }
    }
    fn newline(&mut self, span: Span, _error: &mut dyn ErrorSink) {
        if self.in_inline_table() {
            self.flag("a TOML 1.1 newline in an inline table", span);
        }
    }
}

/// The 1-based line and column (in characters) of byte `at` in `text`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// The `toml` crate's error, which spans several lines (a source excerpt), as
/// one log line.
fn one_line(error: &str) -> String {
    error
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(text: &str) -> String {
        parse(text).expect_err(text)
    }

    #[test]
    fn refusals_say_what_and_where() {
        let tail = ", which TOML 1.0 (Python's tomllib) refuses";
        for (text, what, at) in [
            (
                "x = { a = 1, }\n",
                "a TOML 1.1 trailing comma in an inline table",
                "1, column 14",
            ),
            (
                "x = 1\ny = {\n  a = 1 }\n",
                "a TOML 1.1 newline in an inline table",
                "2, column 6",
            ),
            (
                "x = 1\r\ny = { a = 1, }\r\n",
                "a TOML 1.1 trailing comma in an inline table",
                "2, column 14",
            ),
            (
                "x = \"ab\\x41\"\n",
                "the TOML 1.1 escape \\x",
                "1, column 8",
            ),
            (
                "x = [ 1979-05-27 07:32 ]\n",
                "a TOML 1.1 time without seconds",
                "1, column 7",
            ),
            ("x = 23:59:60\n", "second 60", "1, column 5"),
            ("x = 0000-01-01\n", "year 0", "1, column 5"),
            ("\u{feff}x = 1\n", "a byte-order mark", "1, column 1"),
        ] {
            assert_eq!(refused(text), format!("{what}{tail} (at line {at})"));
        }
    }

    #[test]
    fn crlf_reads_as_lf() {
        let table = parse("x = \"\"\"a\r\nb\"\"\"\r\n").unwrap();
        assert_eq!(table["x"].as_str(), Some("a\nb"));
    }

    #[test]
    fn toml10_lookalikes_are_accepted() {
        for text in [
            "x = { a = [\n1, # c\n2,\n] }\n",
            "x = { a = \"\"\"\nline\"\"\" }\n",
            "x = \"\\\\e \\\\x41\"\ny = '\\e'\n",
            "x = 1 # { a = 1, } \\e\n",
            "x = 07:32:00\ny = 1979-05-27 07:32:00Z\nz = \"07:32\"\n",
        ] {
            parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        }
    }

    #[test]
    fn crate_errors_are_one_line() {
        let err = refused("x = [unclosed\n");
        assert!(!err.contains('\n'), "{err}");
        assert!(!err.is_empty());
    }
}
