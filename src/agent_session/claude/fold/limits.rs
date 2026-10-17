//! The status rules' time limits and caps.

/// An agent written within this many seconds is running (zoetrope's
/// `INTERACTIVE_IDLE_SECS`).
pub const ACTIVE: f64 = 120.0;
/// A tool call in flight keeps its agent running for at most this long.
pub const PENDING_CAP: f64 = 3600.0;
/// A background agent not notified yet is running for at most this long
/// after its last record.
pub const ASYNC_QUIET: f64 = 600.0;
/// An agent's final line can land a moment after its own notice: activity
/// reopens an agent only when it is this much later.
pub const REOPEN_MARGIN: f64 = 5.0;
/// An open background task nobody can check (another machine, no process
/// table) counts this long after it started.
pub const BG_CAP: f64 = 24.0 * 3600.0;

/// How many levels of descendants [`Session::status_of`](super::Session::status_of) looks through for a
/// running one (Claude Code nests a few; a deeper `parentAgentId` chain
/// written into metas must not exhaust the stack).
pub const NESTING_CAP: usize = 64;
/// A Monitor past its timeout by this much has expired even without its last
/// event.
pub const MONITOR_SLACK: f64 = 60.0;
/// The version of the kit's [hook-record format](super#hook-records-format-v1)
/// that [`Session::apply_hook`](super::Session::apply_hook) reads (herdr-panorama's
/// `hooks.VERSION` writes it); records of other versions are skipped.
pub const HOOK_VERSION: f64 = 1.0;
/// A recorded hook stop and a notice at most this long after it are the same
/// ending.
pub(super) const STOP_NOTICE_WINDOW: f64 = 60.0;
/// A prompt is kept to this many characters.
pub(super) const PROMPT_MAX: usize = 200;
