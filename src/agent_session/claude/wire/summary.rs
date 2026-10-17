//! The one-line tool summaries the cards show.

use serde_json::{Map, Value};

use super::{py_isspace, py_splitlines, py_strip};

/// The spawning tools (`Agent`, and `Task` from older Claude Code versions).
pub const SPAWN_TOOLS: [&str; 2] = ["Agent", "Task"];

/// Summaries longer than this many characters are cut with `…`.
pub const SUMMARY_MAX: usize = 200;

/// Whitespace folded to single spaces and cut to [`SUMMARY_MAX`] characters
/// with `…`: panorama's `wire.truncate_summary`.
pub fn truncate_summary(s: &str) -> String {
    let flat = s
        .split(py_isspace)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= SUMMARY_MAX {
        flat
    } else {
        let mut cut: String = flat.chars().take(SUMMARY_MAX - 1).collect();
        cut.push('…');
        cut
    }
}

/// A path relative to `cwd` at a path-component boundary, else as given; cut
/// from the left to [`SUMMARY_MAX`] characters: panorama's `wire.short_path`.
pub fn short_path(path: &str, cwd: Option<&str>) -> String {
    let mut rel = path;
    if let Some(cwd) = cwd.filter(|c| !c.is_empty())
        && let Some(rest) = path.strip_prefix(cwd)
        && (rest.starts_with('/') || cwd.ends_with('/'))
        && !rest.trim_matches('/').is_empty()
    {
        rel = rest.trim_start_matches('/');
    }
    let n = rel.chars().count();
    if n <= SUMMARY_MAX {
        rel.to_string()
    } else {
        let tail: String = rel.chars().skip(n - (SUMMARY_MAX - 1)).collect();
        format!("…{tail}")
    }
}

/// `Name(one-line summary)` of a tool call: panorama's `wire.summarize_tool`.
/// MCP tools show their last name part; Bash prefers the human
/// `description` over the raw command (the cards are narrow).
pub fn summarize_tool(name: &str, input: Option<&Map<String, Value>>, cwd: Option<&str>) -> String {
    let short = if name.starts_with("mcp__") {
        name.split("__").last().unwrap_or(name)
    } else {
        name
    };
    let Some(inp) = input else {
        return short.to_string();
    };
    let pick = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .filter_map(|k| inp.get(*k).and_then(Value::as_str))
            .find(|v| !py_strip(v).is_empty())
            .map(str::to_string)
    };
    let s = match name {
        "Bash" => pick(&["description", "command"]),
        "Read" | "Write" | "Edit" | "NotebookEdit" => {
            pick(&["file_path", "path", "notebook_path"]).map(|p| short_path(&p, cwd))
        }
        n if SPAWN_TOOLS.contains(&n) => pick(&["description", "subagent_type"]),
        "WebFetch" => pick(&["url"]),
        "ToolSearch" | "WebSearch" | "Grep" | "Glob" => pick(&["query", "pattern"]),
        "SendMessage" => pick(&["to"]),
        _ => pick(&[
            "description",
            "query",
            "command",
            "file_path",
            "pattern",
            "url",
            "to",
        ]),
    };
    let Some(s) = s.filter(|s| !s.is_empty()) else {
        return short.to_string();
    };
    let first = py_splitlines(&s)
        .into_iter()
        .find(|l| !py_strip(l).is_empty())
        .unwrap_or(&s);
    let cut: String = truncate_summary(first).chars().take(80).collect();
    format!("{short}({cut})")
}
