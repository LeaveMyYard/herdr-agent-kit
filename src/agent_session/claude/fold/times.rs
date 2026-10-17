//! Times as the fold compares them: totally ordered, and Python's
//! truthiness of `0.0`.

use crate::agent_session::claude::wire::Ending;

/// A time ordered totally, for sets of times.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct T(pub(super) f64);

impl Eq for T {}

impl PartialOrd for T {
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for T {
    fn cmp(&self, other: &T) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// A notice about a task: `(time, ending, tool uses)`, ordered as panorama
/// compares its tuples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct NoticeAt(pub(super) T, pub(super) Ending, pub(super) Option<u64>);

/// panorama's `(when or inf)`: no time, or a time of `0`, sorts last.
pub(super) fn or_inf(t: Option<f64>) -> f64 {
    match t {
        Some(x) if x != 0.0 => x,
        _ => f64::INFINITY,
    }
}

/// Python's `a or b` over optional times (`0.0` is falsy).
pub(super) fn or_time(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match a {
        Some(x) if x != 0.0 => Some(x),
        _ => b,
    }
}

pub(super) fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}
