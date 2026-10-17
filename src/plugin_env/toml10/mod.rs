//! TOML 1.0, strictly: what Python's `tomllib` accepts. **Pure**. Crate-private:
//! [`super::ConfigFile`] is its public face.
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
//! same values. Some inputs still differ, all the other way round (Rust
//! ignores the file, Python applies it or raises): integers outside the
//! 64-bit range (Python integers in `tomllib`), a float literal beyond `f64`
//! (`1e400`, infinite in `tomllib`), nesting deeper than the `toml` crate's
//! recursion limit, and a file that is not UTF-8 (`tomllib` raises
//! `UnicodeDecodeError`, which the plugins do not catch).

use toml::Table;

mod finder;

/// Parses `text` as `tomllib` does. The error is one line; when it is a
/// construct the `toml` crate accepts and `tomllib` refuses, it says which and
/// where, as `tomllib` counts lines and columns.
pub fn parse(text: &str) -> Result<Table, String> {
    // tomllib: `src = s.replace("\r\n", "\n")` before anything else.
    let text = text.replace("\r\n", "\n");
    let table = text
        .parse::<Table>()
        .map_err(|e| one_line(&e.to_string()))?;
    match finder::first_refused(&text) {
        None => Ok(table),
        Some((what, at)) => {
            let (line, column) = line_column(&text, at);
            Err(format!(
                "{what}, which TOML 1.0 (Python's tomllib) refuses (at line {line}, column {column})"
            ))
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
mod tests;
