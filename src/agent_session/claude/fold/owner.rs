//! [`Owner`]: whose file a record came from, and which copy of a record
//! repeated in several files counts.

use super::times::or_inf;

/// Whose file a record came from: the session's main transcript or an
/// agent's own transcript (panorama's `MAIN` or the agent id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
    /// The session's main transcript.
    Main,
    /// The transcript of the agent with this id.
    Agent(String),
}

/// Whether a copy of a record from `owner` at `when` replaces the copy kept
/// from `old_owner` at `old_when`.
///
/// A fork's file repeats its parent's history (spawning calls and background
/// launches included), so one record can be read from several files. The
/// copy that counts does not depend on the order files are read: main's
/// wins, then the earliest record (panorama's `(when or inf)`: no time, or a
/// time of `0`, sorts last).
pub(super) fn replaces(
    owner: &Owner,
    when: Option<f64>,
    old_owner: &Owner,
    old_when: Option<f64>,
) -> bool {
    if *old_owner == Owner::Main {
        return false;
    }
    if *owner == Owner::Main {
        return true;
    }
    or_inf(when) < or_inf(old_when)
}
