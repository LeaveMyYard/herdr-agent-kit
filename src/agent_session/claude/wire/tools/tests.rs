use std::collections::BTreeSet;

use super::*;
use crate::agent_session::claude::wire::MARKERS;

/// `MARKERS` is panorama's list byte for byte, so it spells tool names
/// itself. Its `"name":"X"` markers must be exactly these names:
///
/// - the spawning tools ([`AGENT`], [`TASK`]) and [`MONITOR`], [`TASK_STOP`],
///   [`KILL_SHELL`], which the fold dispatches on by name;
/// - [`SEND_MESSAGE`], whose result carries the resumed agent's id (the fold
///   reads that id, not the name; the summaries dispatch on the name);
/// - `Workflow`, whose launch the fold reads from its result (`runId`), so
///   the kit has no constant for it.
///
/// [`BASH`] has no name marker, on purpose (panorama has none either): the
/// full-file pass needs only background Bash calls, which it finds through
/// `"run_in_background":true`, `"backgroundTaskId"` and `"timeoutMs"`; the
/// tail summary reads its window without the prefilter. Adding a
/// `"name":"Bash"` marker would break byte-for-byte parity with panorama.
#[test]
fn the_name_markers_are_these_tools() {
    let names: BTreeSet<&[u8]> = MARKERS
        .iter()
        .filter_map(|m| m.strip_prefix(br#""name":""#)?.strip_suffix(b"\""))
        .collect();
    let want: BTreeSet<&[u8]> = [
        AGENT,
        TASK,
        MONITOR,
        TASK_STOP,
        KILL_SHELL,
        SEND_MESSAGE,
        "Workflow",
    ]
    .iter()
    .map(|t| t.as_bytes())
    .collect();
    assert_eq!(names, want);
    assert!(!names.contains(BASH.as_bytes()));
}
