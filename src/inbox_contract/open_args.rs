//! The `inbox open` arguments: how another plugin opens the inbox popup on a question
//! or a pane's questions. **Pure.**
//!
//! `inbox open [--question Qn | --group G | --pane P | --session S] [--no-hold]` writes a
//! one-shot focus request (`{kind, value, hold}`) next to the inbox database and opens
//! the popup, which starts on that target.
//!
//! - Writing (herdr-panorama's `inbox_link.py`): [`deep_link_args`], with
//!   [`supports_deep_link`] and [`supports_no_hold`] deciding from the inbox's version
//!   and its `inbox open --help`.
//! - Reading (herdr-inbox's `cli.py` and `deeplink.py`): [`parse_open_args`] and
//!   [`valid`], giving the request the popup is opened for.
//!
//! Goldens: `fixtures/inbox/open-args.json`.

use super::text::{ascii_digits, py_space};

/// What `inbox open` can be pointed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    /// A question (`Qn`, `n`, or `machine:Qn` for a remote machine's): its group.
    Question,
    /// A question group by number.
    Group,
    /// A herdr pane id: its oldest open group.
    Pane,
    /// A Claude Code session id: its oldest open group.
    Session,
}

impl OpenKind {
    /// Every kind, in the order `inbox open` checks them.
    pub const ALL: [OpenKind; 4] = [
        OpenKind::Question,
        OpenKind::Group,
        OpenKind::Pane,
        OpenKind::Session,
    ];

    /// The name in the request file (`"question"`, …), which is also the flag without `--`.
    pub fn as_str(self) -> &'static str {
        match self {
            OpenKind::Question => "question",
            OpenKind::Group => "group",
            OpenKind::Pane => "pane",
            OpenKind::Session => "session",
        }
    }

    fn from_flag(flag: &str) -> Option<OpenKind> {
        OpenKind::ALL.into_iter().find(|k| k.as_str() == flag)
    }
}

/// The focus request `inbox open` writes for the popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    /// What it points at.
    pub kind: OpenKind,
    /// The value as stored: a question's `Q`/`q` prefix is removed (`Q7` → `7`, `007`
    /// stays `007`, `laptop:Q12` stays as it is); other kinds are stored as given.
    pub value: String,
}

/// A parsed `inbox open` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    /// The target; `None` opens the popup plainly (no flag, or an empty value).
    pub target: Option<OpenTarget>,
    /// `false` with `--no-hold`: a normal popup instead of one held until Tab or →.
    /// Without a target nothing is stored, so it has no effect then.
    pub hold: bool,
}

/// Why an `inbox open` command line was refused (argparse's exit 2 in Python).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenArgsError {
    /// An argument that is not one of the flags, or a flag given a value it takes none.
    Unrecognized(String),
    /// A flag given without its value.
    MissingValue(OpenKind),
    /// Two different target flags.
    Conflict(OpenKind, OpenKind),
    /// A value [`valid`] refuses.
    Invalid(OpenKind, String),
}

impl std::fmt::Display for OpenArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenArgsError::Unrecognized(arg) => write!(f, "unrecognized arguments: {arg}"),
            OpenArgsError::MissingValue(k) => {
                write!(f, "argument --{}: expected one argument", k.as_str())
            }
            OpenArgsError::Conflict(a, b) => {
                write!(
                    f,
                    "argument --{}: not allowed with argument --{}",
                    b.as_str(),
                    a.as_str()
                )
            }
            OpenArgsError::Invalid(k, v) => {
                let expected = match k {
                    OpenKind::Question => "Qn or n",
                    OpenKind::Group => "a group number",
                    OpenKind::Pane | OpenKind::Session => "an id",
                };
                write!(f, "--{} '{v}': expected {expected}", k.as_str())
            }
        }
    }
}

impl std::error::Error for OpenArgsError {}

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
        OpenKind::Pane => !value.is_empty() && !value.contains(py_space),
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
/// like a negative number. Unlike argparse, flags cannot be abbreviated (`--quest`) and
/// `-h` is not handled here. An empty value means no target.
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
                Some(v) if !v.starts_with('-') || looks_negative(v) => {
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
                OpenKind::Question => value.trim_start_matches(['Q', 'q']).to_owned(),
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

/// The first inbox version with `inbox open --question/--pane` (0.6.0).
pub const DEEP_LINK_VERSION: (u64, u64, u64) = (0, 6, 0);

/// Whether this inbox takes `inbox open --question/--pane`: its version is at least
/// [`DEEP_LINK_VERSION`], else its `inbox open --help` (`None`: not available) names
/// `--pane` (herdr-panorama's `supports_deep_link`).
///
/// The version is read as `major.minor[.patch]` at its start; anything else counts as 0.
pub fn supports_deep_link(version: &str, help: Option<&str>) -> bool {
    version_prefix(version) >= DEEP_LINK_VERSION || help.is_some_and(|h| h.contains("--pane"))
}

/// Whether `inbox open --help` names `--no-hold` (herdr-inbox 0.6.1+).
pub fn supports_no_hold(help: Option<&str>) -> bool {
    help.is_some_and(|h| h.contains("--no-hold"))
}

fn version_prefix(version: &str) -> (u64, u64, u64) {
    fn number(s: &str) -> Option<(u64, &str)> {
        let d = ascii_digits(s);
        // Python's int() has no limit; a saturated part still compares correctly.
        (!d.is_empty()).then(|| (d.parse().unwrap_or(u64::MAX), &s[d.len()..]))
    }
    let parsed = (|| {
        let (major, rest) = number(version)?;
        let (minor, rest) = number(rest.strip_prefix('.')?)?;
        let patch = rest
            .strip_prefix('.')
            .and_then(number)
            .map_or(0, |(p, _)| p);
        Some((major, minor, patch))
    })();
    parsed.unwrap_or((0, 0, 0))
}

/// The arguments herdr-panorama passes to `inbox open` (`inbox_link.open_inbox`): on
/// `question`'s group, else on `pane`'s oldest open group, with `--no-hold` when this
/// inbox has it; `None` when there is no target or the inbox has no deep link, and the
/// plain popup action is used instead.
///
/// The arguments are raw: they still go through herdr's argument sanitising.
pub fn deep_link_args(
    question: Option<u64>,
    pane: Option<&str>,
    version: &str,
    help: Option<&str>,
) -> Option<Vec<String>> {
    let pane = pane.filter(|p| !p.is_empty());
    if question.is_none() && pane.is_none() || !supports_deep_link(version, help) {
        return None;
    }
    let mut args = match (question, pane) {
        (Some(q), _) => vec!["open".to_owned(), "--question".to_owned(), format!("Q{q}")],
        (None, Some(p)) => vec!["open".to_owned(), "--pane".to_owned(), p.to_owned()],
        (None, None) => return None,
    };
    if supports_no_hold(help) {
        args.push("--no-hold".to_owned());
    }
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panorama_args_round_trip_through_the_inbox_parser() {
        let args = deep_link_args(Some(7), None, "0.10.6", Some("[--no-hold]")).unwrap();
        assert_eq!(args, ["open", "--question", "Q7", "--no-hold"]);
        let req = parse_open_args(&args[1..]).unwrap();
        assert_eq!(
            req,
            OpenRequest {
                target: Some(OpenTarget {
                    kind: OpenKind::Question,
                    value: "7".into()
                }),
                hold: false
            }
        );
    }

    #[test]
    fn versions() {
        assert_eq!(version_prefix("0.10.6"), (0, 10, 6));
        assert_eq!(version_prefix("0.6"), (0, 6, 0));
        assert_eq!(version_prefix("1.0.0-rc1"), (1, 0, 0));
        assert_eq!(version_prefix("v1.0.0"), (0, 0, 0));
        assert_eq!(version_prefix("7"), (0, 0, 0));
        assert_eq!(
            version_prefix("99999999999999999999999.1"),
            (u64::MAX, 1, 0)
        );
    }

    #[test]
    fn negative_looking_values() {
        assert!(looks_negative("-1") && looks_negative("-.5") && looks_negative("-1.5"));
        assert!(!looks_negative("--no-hold") && !looks_negative("-") && !looks_negative("-1."));
        assert_eq!(
            parse_open_args(&["--question", "--no-hold"]),
            Err(OpenArgsError::MissingValue(OpenKind::Question))
        );
    }
}
