//! The focus request `inbox open` stores, and why a command line is refused.

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

    pub(super) fn from_flag(flag: &str) -> Option<OpenKind> {
        OpenKind::ALL.into_iter().find(|k| k.as_str() == flag)
    }
}

/// The focus request `inbox open` writes for the popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    /// What it points at.
    pub kind: OpenKind,
    /// The value as stored: a question's reference goes through
    /// [`question_ref`](super::question_ref), which removes the one `Q`/`q` right before
    /// the number (`Q7` → `7`, `007` stays `007`, `laptop:Q12` → `laptop:12`, `qnap:Q12` →
    /// `qnap:12`); other kinds are stored as given.
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
    /// A value [`valid`](super::valid) refuses.
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
