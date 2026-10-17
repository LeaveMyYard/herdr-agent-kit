//! Reading an `inbox open` command line (herdr-inbox's `cli.py` argparse
//! setup and `deeplink.valid`).

use super::{OpenArgsError, OpenKind, OpenRequest, OpenTarget, question_ref};
use crate::pytext::{ascii_digits, py_isspace};

/// Whether `value` is a well-formed target of `kind` (herdr-inbox's `deeplink.valid`).
///
/// - question: `Qn`, `qn` or `n`, optionally after `machine:` (1 to 64 of `A-Za-z0-9._-`);
/// - group: digits;
/// - pane: any non-empty text without whitespace;
/// - session: a canonical 8-4-4-4-12 hex UUID, any case.
///
/// Digits are ASCII (Python's `\d` would also take other scripts' digits; no inbox or
/// herdr id has them).
pub fn valid(kind: OpenKind, value: &str) -> bool {
    match kind {
        OpenKind::Question => {
            let id = match value.split_once(':') {
                Some((machine, id)) => {
                    let ok = (1..=64).contains(&machine.len())
                        && machine
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
                    if !ok {
                        return false;
                    }
                    id
                }
                None => value,
            };
            let id = id.strip_prefix(['Q', 'q']).unwrap_or(id);
            !id.is_empty() && ascii_digits(id).len() == id.len()
        }
        OpenKind::Group => !value.is_empty() && ascii_digits(value).len() == value.len(),
        OpenKind::Pane => !value.is_empty() && !value.contains(py_isspace),
        OpenKind::Session => is_session_id(value),
    }
}

/// A canonical 8-4-4-4-12 hex UUID, any case (herdr-inbox's `session_id_or_none` rule;
/// the shared validator lives with the Claude session code).
fn is_session_id(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Parse the arguments after `inbox open` (herdr-inbox's argparse setup, then its
/// validation).
///
/// Accepts `--kind value`, `--kind=value` and `--no-hold`, in any order; a repeated flag
/// keeps its last value. As in argparse, a value may start with `-` only when it looks
/// like a negative number (or is `-` alone). Unlike argparse, flags cannot be abbreviated
/// (`--quest`), `-h` is not handled here, and the error is the first problem met (argparse
/// reports every unrecognized argument at once, after the others). An empty value means no target.
pub fn parse_open_args<S: AsRef<str>>(args: &[S]) -> Result<OpenRequest, OpenArgsError> {
    let mut given: Option<(OpenKind, String)> = None;
    let mut hold = true;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_ref();
        i += 1;
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (arg, None),
        };
        if flag == "--no-hold" {
            if inline.is_some() {
                return Err(OpenArgsError::Unrecognized(arg.to_owned()));
            }
            hold = false;
            continue;
        }
        let Some(kind) = flag.strip_prefix("--").and_then(OpenKind::from_flag) else {
            return Err(OpenArgsError::Unrecognized(arg.to_owned()));
        };
        let value = match inline {
            Some(v) => v.to_owned(),
            None => match args.get(i).map(AsRef::as_ref) {
                // argparse: `-` alone and negative numbers are values, not flags
                Some(v) if !v.starts_with('-') || v == "-" || looks_negative(v) => {
                    i += 1;
                    v.to_owned()
                }
                _ => return Err(OpenArgsError::MissingValue(kind)),
            },
        };
        match &given {
            Some((other, _)) if *other != kind => {
                return Err(OpenArgsError::Conflict(*other, kind));
            }
            _ => given = Some((kind, value)),
        }
    }
    let target = match given {
        Some((kind, value)) if !value.is_empty() => {
            if !valid(kind, &value) {
                return Err(OpenArgsError::Invalid(kind, value));
            }
            let value = match kind {
                OpenKind::Question => question_ref(&value),
                _ => value,
            };
            Some(OpenTarget { kind, value })
        }
        _ => None,
    };
    Ok(OpenRequest { target, hold })
}

/// argparse's `^-\d+$|^-\d*\.\d+$`: an argument starting with `-` that is still a value.
fn looks_negative(arg: &str) -> bool {
    let Some(num) = arg.strip_prefix('-') else {
        return false;
    };
    let int = ascii_digits(num);
    if !int.is_empty() && int.len() == num.len() {
        return true;
    }
    match num[int.len()..].strip_prefix('.') {
        Some(frac) => !frac.is_empty() && ascii_digits(frac).len() == frac.len(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
