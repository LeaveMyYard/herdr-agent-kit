//! Opening the inbox on a target from herdr-panorama (`inbox_link.py`): the
//! `inbox open` arguments and the version checks that gate them.

use crate::pytext::ascii_digits;

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
mod tests;
