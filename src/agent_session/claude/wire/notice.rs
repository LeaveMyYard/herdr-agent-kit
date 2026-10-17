//! `<task-notification>`s and inbox question ids.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use super::py_isspace;

/// The tools that stop a background task (`TaskStop`, and
/// `KillShell` from older Claude Code versions).
pub const BG_STOP_TOOLS: [&str; 2] = ["TaskStop", "KillShell"];

/// Claude Code's marker task id in the notice that lists the tasks a restart
/// orphaned: never a task of its own.
pub const ORPHAN: &str = "__orphan_summary__";

/// How a task ended, from a `<task-notification>`'s `<status>`.
///
/// Ordered as panorama compares the status strings (`"cxl" < "done" <
/// "fail"`), so the latest of two notices at the same time is picked the same
/// way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Ending {
    /// `killed` or `stopped`.
    Cancelled,
    /// `completed`, or a Monitor's `[Monitor expired` event.
    Done,
    /// `failed` or `error`.
    Failed,
}

impl Ending {
    /// panorama's `NOTICE_STATUS`.
    pub fn of_status(status: &str) -> Option<Ending> {
        match status {
            "completed" => Some(Ending::Done),
            "failed" | "error" => Some(Ending::Failed),
            "killed" | "stopped" => Some(Ending::Cancelled),
            _ => None,
        }
    }
}

/// One task a `<task-notification>` names: panorama's `parse_notice` dict.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Notice {
    /// The task (an agent id, a background task id, a workflow task id).
    pub task: String,
    /// The `<tool-use-id>` of the call that started it, when the notice names it.
    pub tool_use: Option<String>,
    /// How it ended; `None` when the notice does not say.
    pub status: Option<Ending>,
    /// Its `<tool_uses>` count, when that is a number.
    pub tool_uses: Option<u64>,
}

struct NoticeRes {
    block: Regex,
    task_ids: Regex,
    tags: [(&'static str, Regex); 4],
    event: Regex,
    qid: Regex,
    qid_json: Regex,
}

fn notice_res() -> &'static NoticeRes {
    static R: OnceLock<NoticeRes> = OnceLock::new();
    // panorama's `_TAG` is one pattern with a backreference (`</\1>`), which
    // `regex` does not have: one pattern per tag finds the same matches,
    // because a match holds no `<` between its tags, so two can never overlap.
    let tag = |name: &str| Regex::new(&format!(r"<{name}>\s*([^<]*?)\s*</{name}>")).expect("tag");
    R.get_or_init(|| NoticeRes {
        block: Regex::new(r"(?s)<task-notification>.*?</task-notification>").expect("block"),
        task_ids: Regex::new(r"<task-id>\s*([^<]*?)\s*</task-id>").expect("task ids"),
        tags: [
            ("task-id", tag("task-id")),
            ("tool-use-id", tag("tool-use-id")),
            ("status", tag("status")),
            ("tool_uses", tag("tool_uses")),
        ],
        event: Regex::new(r"<event>\s*\[Monitor expired").expect("event"),
        qid: Regex::new(r"(?:Recorded as Q|\bQ)([0-9]+)\b").expect("qid"),
        qid_json: Regex::new(r#""id"\s*:\s*([0-9]+)\s*,\s*"status"\s*:\s*"(?:open|already_open)""#)
            .expect("qid json"),
    })
}

fn starts_notice(text: &str) -> bool {
    text.trim_start_matches(py_isspace)
        .starts_with("<task-notification>")
}

/// Every task the `<task-notification>` blocks of a carrier name, one
/// [`Notice`] per task: panorama's `wire.parse_notices`. A carrier can hold
/// several blocks, and the notice a resumed session gets for the background
/// tasks its previous process left names several `<task-id>`s (plus
/// [`ORPHAN`], skipped). Text that does not start with a notice names none.
pub fn parse_notices(text: &str) -> Vec<Notice> {
    if !starts_notice(text) {
        return Vec::new();
    }
    let r = notice_res();
    let mut out = Vec::new();
    for block in r.block.find_iter(text) {
        let block = block.as_str();
        let Some(notice) = parse_notice(block) else {
            continue;
        };
        for id in task_ids(block) {
            out.push(Notice {
                task: id.to_string(),
                ..notice.clone()
            });
        }
    }
    out
}

fn task_ids(block: &str) -> Vec<&str> {
    notice_res()
        .task_ids
        .captures_iter(block)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .filter(|i| !i.is_empty() && !i.starts_with(ORPHAN))
        .collect()
}

/// One `<task-notification>` block as a [`Notice`] about its first task, or
/// `None` when it names no task: panorama's `wire.parse_notice`.
///
/// A tag given twice counts with its last value (Python's `dict` of the
/// matches). `<tool_uses>` counts only when it is ASCII digits (Python's
/// `str.isdigit` also takes other Unicode digits).
pub fn parse_notice(block: &str) -> Option<Notice> {
    if !starts_notice(block) {
        return None;
    }
    let r = notice_res();
    let first = *task_ids(block).first()?;
    let tag = |name: &str| -> Option<&str> {
        let re = &r.tags.iter().find(|(n, _)| *n == name)?.1;
        re.captures_iter(block)
            .last()
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
    };
    let mut status = tag("status").and_then(Ending::of_status);
    if status.is_none() && r.event.is_match(block) {
        status = Some(Ending::Done); // "[Monitor expired after 30m ...]": the watch is over
    }
    let uses = tag("tool_uses").unwrap_or("");
    Some(Notice {
        task: first.to_string(),
        tool_use: tag("tool-use-id").map(str::to_string),
        status,
        tool_uses: (!uses.is_empty() && uses.bytes().all(|b| b.is_ascii_digit()))
            .then(|| uses.parse().ok())
            .flatten(),
    })
}

/// The inbox question ids an `ask_operator` result names (`Recorded as Q12`,
/// `Q12`, or a JSON receipt's `"id": 12, "status": "open"`): panorama's
/// `wire.question_ids`. Only ASCII digits are read (Python's `\d` also takes
/// other Unicode digits), and an id beyond `u64` is skipped.
pub fn question_ids(text: &str) -> BTreeSet<u64> {
    let r = notice_res();
    r.qid
        .captures_iter(text)
        .chain(r.qid_json.captures_iter(text))
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}
