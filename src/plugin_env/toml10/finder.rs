//! The constructs the `toml` crate accepts and `tomllib` refuses, found in a
//! document the crate already parsed: a receiver of `toml_parser`'s events.

use toml_parser::decoder::Encoding;
use toml_parser::parser::{EventReceiver, parse_document};
use toml_parser::{ErrorSink, Source, Span};

/// The first construct in `text` (which the `toml` crate already parsed) that
/// `tomllib` refuses, and its byte offset.
pub(super) fn first_refused(text: &str) -> Option<(&'static str, usize)> {
    if text.starts_with('\u{feff}') {
        return Some(("a byte-order mark", 0));
    }
    // tomllib replaces CRLF once; a CR left over (`\r\r\n`, a lone `\r`) is
    // illegal everywhere in its grammar, while the crate reads `\r\n` again.
    if let Some(at) = text.find('\r') {
        return Some(("a carriage return not followed by a newline", at));
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
