//! The marker prefilter: in a full-file pass only lines holding a marker are
//! worth a JSON parse.

use std::sync::OnceLock;

use memchr::memmem::Finder;

/// Lines worth a full JSON parse in a full-file pass: panorama's
/// `wire.MARKERS`, byte for byte.
pub const MARKERS: &[&[u8]] = &[
    br#""name":"Agent""#,
    br#""name":"Task""#,
    br#""name":"SendMessage""#,
    br#""name":"Workflow""#,
    b"async_launched",
    br#""totalToolUseCount""#,
    b"resumedAgentId",
    b"runId",
    b"task-notification",
    b"ask_operator",
    b"Recorded as Q",
    b"already with the operator",
    br#""type":"custom-title""#,
    br#""type":"agent-name""#,
    br#""type":"ai-title""#,
    // the conversation moved on to a new session id (the continuation rule follows it)
    br#""type":"continued-in""#,
    // background work: Bash in the background, Monitor watches, TaskStop / KillShell
    br#""run_in_background":true"#,
    br#""backgroundTaskId""#,
    br#""name":"Monitor""#,
    br#""timeoutMs""#,
    br#""name":"TaskStop""#,
    br#""name":"KillShell""#,
    br#""task_type""#,
    br#""shell_id""#,
];

/// A user message with string content, in any key order: a line holding
/// **both** markers is wanted too.
pub const PROMPT_MARKERS: [&[u8]; 2] = [br#""role":"user""#, br#""content":""#];

struct Finders {
    markers: Vec<Finder<'static>>,
    prompt: [Finder<'static>; 2],
}

fn finders() -> &'static Finders {
    static F: OnceLock<Finders> = OnceLock::new();
    F.get_or_init(|| Finders {
        markers: MARKERS.iter().map(|m| Finder::new(*m)).collect(),
        prompt: [
            Finder::new(PROMPT_MARKERS[0]),
            Finder::new(PROMPT_MARKERS[1]),
        ],
    })
}

/// Whether a raw line holds a marker (or both prompt markers): panorama's
/// `wire.wanted`.
pub fn wanted(raw: &[u8]) -> bool {
    let f = finders();
    f.markers.iter().any(|m| m.find(raw).is_some())
        || f.prompt.iter().all(|m| m.find(raw).is_some())
}

/// The indexes of the lines [`wanted`] accepts, in order: panorama's
/// `wire.wanted_lines`.
pub fn wanted_lines<L: AsRef<[u8]>>(lines: &[L]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| wanted(l.as_ref()))
        .map(|(i, _)| i)
        .collect()
}

/// Whether `needle` occurs in `haystack` (a byte search, as Python's `in`).
pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    memchr::memmem::find(haystack, needle).is_some()
}
