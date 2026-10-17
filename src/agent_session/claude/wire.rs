//! The transcript wire format. **Pure.**
//!
//! Lenient parsing of Claude Code transcript records, after panorama's
//! `scan/wire.py` (itself after zoetrope's `wire.rs`, MIT): unknown record
//! types and content blocks are skipped, every field is optional,
//! `message.content` may be a string or a list, `toolUseResult` a string or an
//! object, and a blank or unparsable line states nothing and never panics.
//!
//! - [`Line::parse`] turns one transcript line into a JSON object (or `None`),
//!   [`Line::record`] views it as a [`Record`], and [`ts`] reads a timestamp.
//! - [`MARKERS`], [`wanted`] and [`wanted_lines`] are the marker prefilter: in
//!   a full-file pass only lines holding a marker are worth a JSON parse.
//! - [`summarize_tool`], [`short_path`] and [`truncate_summary`] are the
//!   one-line tool summaries the cards show; [`TailInfo`] is what
//!   [`super::io::summarize_tail`] reads from the end of a transcript.
//!
//! # Deviations from Python
//!
//! Python's `json` and `datetime` accept more than any transcript contains.
//! Where matching them would need a second JSON or date parser, this module
//! is stricter, and a record it cannot read is skipped like any broken line:
//!
//! - JSON: `NaN`, `Infinity` and numbers out of `f64` range are rejected, a
//!   line nested deeper than 128 levels is rejected (Python: about 1000), and
//!   UTF-16/32 lines are rejected (Python detects them; a UTF-8 BOM is accepted
//!   by both).
//! - A string holding a **lone UTF-16 surrogate escape** (`"\ud83d"` with no
//!   low half; Node writes one when it cuts a string inside an emoji) is
//!   accepted like Python accepts it, but the lone surrogate reads as
//!   U+FFFD (a Rust string cannot hold it; Python keeps the surrogate). Text
//!   compared with such a string differs from Python only at that character.
//! - Timestamps: see [`ts`].
//! - Text fields that are not strings (a numeric `text`, `name` or `id`) are
//!   treated as absent; Python would use their `str()`.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use memchr::memmem::Finder;
use regex::Regex;
use serde_json::{Map, Value};

use super::{SessionId, Ts};

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

/// The spawning tools (`Agent`, and `Task` from older Claude Code versions).
pub const SPAWN_TOOLS: [&str; 2] = ["Agent", "Task"];

/// Summaries longer than this many characters are cut with `…`.
pub const SUMMARY_MAX: usize = 200;

/// Record types Claude Code writes as bookkeeping, captured from real
/// transcripts. They never move a conversation: the continuation rule does not
/// depend on this list (only `user` and `assistant` records clear a
/// continuation), so a type Claude Code adds tomorrow is just as harmless.
pub const BOOKKEEPING: [&str; 4] = ["last-prompt", "cost-state", "mode", "file-history-snapshot"];

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

// -- timestamps ------------------------------------------------------------------------------

/// A record's `timestamp` as epoch seconds: panorama's `wire.ts` (and the
/// inbox's `transcript._ts`).
///
/// - A number is seconds, or milliseconds when it is above `1e11`.
/// - A string is an ISO-8601 date-time (`Z` is UTC), else an integer
///   (`"1760432400000"`, read as a number as above).
/// - `null`, booleans, arrays and objects are no time.
///
/// Python's `datetime.fromisoformat` accepts more forms than Claude Code
/// writes. This reads `YYYY-MM-DD`, optionally followed by `T` or a space
/// and `HH[:MM[:SS[.fraction]]]` (`,` also starts a fraction; digits after the
/// sixth are dropped, as in Python), optionally followed by an offset `Z`,
/// `±HH`, `±HHMM` or `±HH:MM[:SS]`. Week dates, the basic format
/// (`20261014T0900`), other date-time separators and fractional offsets are
/// not read, nor are an empty fraction (`12.Z`) and integer strings in
/// non-ASCII digits. A date-time **without an offset is read as UTC**; Python
/// reads it in the machine's local zone, which a pure function cannot know
/// (Claude Code always writes `Z`). A 13-digit string is milliseconds here;
/// Python reads it as a basic-format date first (year 1376 for
/// `"1376050891932"`).
///
/// A time of `0` is a time here (as in Python's `is not None` checks); the
/// continuation rule treats it as no time where both Pythons do (see
/// [`super::Continuation`]).
pub fn ts(value: &Value) -> Option<Ts> {
    match value {
        Value::Number(n) => n.as_f64().and_then(number_ts),
        Value::String(s) => iso_ts(s).or_else(|| py_int(s).and_then(number_ts)),
        _ => None,
    }
}

fn number_ts(v: f64) -> Option<Ts> {
    let t = if v > 1e11 { v / 1000.0 } else { v };
    t.is_finite().then_some(Ts(t))
}

/// Python's `int(text)` for ASCII digits, as an `f64` (correctly rounded, like
/// Python's int-to-float conversion): surrounding whitespace, a sign, and
/// single underscores between digits.
fn py_int(text: &str) -> Option<f64> {
    let s = py_strip(text);
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return None;
    }
    let mut digits = String::with_capacity(body.len() + 1);
    if neg {
        digits.push('-');
    }
    for c in body.chars() {
        match c {
            '0'..='9' => digits.push(c),
            '_' => {}
            _ => return None,
        }
    }
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn digits(&mut self, n: usize) -> Option<i64> {
        let end = self.i.checked_add(n)?;
        let part = self.s.get(self.i..end)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.i = end;
        Some(part.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
    }

    fn at_digit(&self) -> bool {
        self.peek().is_some_and(|c| c.is_ascii_digit())
    }

    /// `.digits` or `,digits`: microseconds (the first six digits).
    fn fraction(&mut self) -> Option<i64> {
        if !(self.eat(b'.') || self.eat(b',')) {
            return Some(0);
        }
        let start = self.i;
        while self.at_digit() {
            self.i += 1;
        }
        let digits = &self.s[start..self.i];
        if digits.is_empty() {
            return None;
        }
        let mut us = 0i64;
        for k in 0..6 {
            us = us * 10 + digits.get(k).map_or(0, |d| i64::from(d - b'0'));
        }
        Some(us)
    }

    fn done(&self) -> bool {
        self.i == self.s.len()
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn iso_ts(text: &str) -> Option<Ts> {
    let mut c = Cursor {
        s: text.as_bytes(),
        i: 0,
    };
    let year = c.digits(4)?;
    if !c.eat(b'-') {
        return None;
    }
    let month = c.digits(2)?;
    if !c.eat(b'-') {
        return None;
    }
    let day = c.digits(2)?;
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
    {
        return None;
    }
    let (mut secs, mut micros) = (days_from_civil(year, month, day) * 86_400, 0i64);
    if !c.done() {
        if !(c.eat(b'T') || c.eat(b' ')) {
            return None;
        }
        let hour = c.digits(2)?;
        let (mut minute, mut second) = (0, 0);
        if c.eat(b':') {
            minute = c.digits(2)?;
            if c.eat(b':') {
                second = c.digits(2)?;
                micros = c.fraction()?;
            }
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        secs += hour * 3600 + minute * 60 + second;
        if !c.done() {
            let sign = match c.peek() {
                Some(b'Z') => 0,
                Some(b'+') => -1,
                Some(b'-') => 1,
                _ => return None,
            };
            c.i += 1;
            if sign != 0 {
                let oh = c.digits(2)?;
                let (mut om, mut os) = (0, 0);
                if c.eat(b':') {
                    om = c.digits(2)?;
                    if c.eat(b':') {
                        os = c.digits(2)?;
                    }
                } else if c.at_digit() {
                    om = c.digits(2)?;
                }
                if oh > 23 || om > 59 || os > 59 {
                    return None;
                }
                secs += sign * (oh * 3600 + om * 60 + os);
            }
            if !c.done() {
                return None;
            }
        }
    }
    // datetime.timestamp(): the exact microsecond count, divided once (correctly rounded)
    let total = secs.checked_mul(1_000_000)?.checked_add(micros)?;
    Some(Ts(total as f64 / 1e6))
}

// -- Python text helpers -----------------------------------------------------------------------

/// Python's `str.isspace` for one character: Unicode `White_Space` plus the
/// four ASCII separators `\x1c`..`\x1f`.
pub(crate) fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Python's `str.strip()`.
pub(crate) fn py_strip(s: &str) -> &str {
    s.trim_matches(py_isspace)
}

/// Python's `bytes.strip()` is empty: the line is blank (`b" \t\n\r\x0b\x0c"`).
pub(crate) fn blank(raw: &[u8]) -> bool {
    raw.iter()
        .all(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
}

/// Python's `str.splitlines()` (without the line ends).
pub(crate) fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = iter.peek() {
                    iter.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python truthiness of a JSON value.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// The code unit of a `\uXXXX` escape starting at `raw[i]` (the backslash).
fn escape_unit(raw: &[u8], i: usize) -> Option<u16> {
    let hex = raw.get(i + 2..i + 6)?;
    if raw.get(i + 1) != Some(&b'u') || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
}

/// `raw` with every unpaired UTF-16 surrogate escape (`\ud800`..`\udfff`
/// not part of a high-low pair) replaced by `\ufffd`. Escaped backslashes are
/// skipped, so `\\ud83d` (a backslash, then text) is left alone.
fn replace_lone_surrogates(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        match escape_unit(raw, i) {
            Some(0xD800..=0xDBFF) if matches!(escape_unit(raw, i + 6), Some(0xDC00..=0xDFFF)) => {
                out.extend_from_slice(&raw[i..i + 12]);
                i += 12;
            }
            Some(0xD800..=0xDFFF) => {
                out.extend_from_slice(b"\\ufffd");
                i += 6;
            }
            _ => {
                // any other escape (or a stray backslash): copy it and the byte it escapes
                out.extend_from_slice(&raw[i..(i + 2).min(raw.len())]);
                i += 2;
            }
        }
    }
    out
}

// -- lines and records -------------------------------------------------------------------------

/// One parsed transcript line: a JSON object. Everything else (a broken line,
/// an array, a number) is not a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Line(Map<String, Value>);

impl Line {
    /// Parses one raw line (trailing `\r` and surrounding JSON whitespace
    /// allowed; a leading UTF-8 BOM skipped): panorama's `wire.parse_line`.
    pub fn parse(raw: &[u8]) -> Option<Line> {
        let raw = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(raw);
        match serde_json::from_slice::<Value>(raw) {
            Ok(Value::Object(map)) => Some(Line(map)),
            Ok(_) => None,
            // Python's json accepts a lone UTF-16 surrogate escape (Node writes one
            // when a string is cut inside an emoji); serde_json does not
            Err(_) if contains(raw, b"\\ud") || contains(raw, b"\\uD") => {
                match serde_json::from_slice::<Value>(&replace_lone_surrogates(raw)) {
                    Ok(Value::Object(map)) => Some(Line(map)),
                    _ => None,
                }
            }
            Err(_) => None,
        }
    }

    /// A line built from an already parsed object.
    pub fn from_map(map: Map<String, Value>) -> Line {
        Line(map)
    }

    /// The whole object, owned.
    pub fn into_map(self) -> Map<String, Value> {
        self.0
    }

    /// The whole object.
    pub fn map(&self) -> &Map<String, Value> {
        &self.0
    }

    /// One top-level field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// The record type (`"type"`), when it is a string.
    pub fn kind(&self) -> Option<&str> {
        self.str_field("type")
    }

    /// The record's **own** (top-level) timestamp. A nested one, such as a
    /// `file-history-snapshot`'s `snapshot.timestamp`, never dates a record.
    pub fn ts(&self) -> Option<Ts> {
        self.get("timestamp").and_then(ts)
    }

    /// `isSidechain: true` (exactly the JSON `true`).
    pub fn is_sidechain(&self) -> bool {
        matches!(self.get("isSidechain"), Some(Value::Bool(true)))
    }

    fn str_field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// `message`, when it is an object.
    pub fn message(&self) -> Option<&Map<String, Value>> {
        self.get("message").and_then(Value::as_object)
    }

    /// The object blocks of `message.content` when it is a list (any record
    /// type): panorama's `wire.blocks`.
    pub fn blocks(&self) -> Vec<Block<'_>> {
        match self.message().and_then(|m| m.get("content")) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_object)
                .map(Block::of)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `message.content` when it is a string that is not blank: panorama's
    /// `wire.string_content`.
    pub fn string_content(&self) -> Option<&str> {
        match self.message().and_then(|m| m.get("content")) {
            Some(Value::String(s)) if !py_strip(s).is_empty() => Some(s),
            _ => None,
        }
    }

    /// The text a typed prompt or a `<task-notification>` arrives in, from any
    /// of its three carriers (a `queue-operation` `enqueue`, an `attachment`
    /// `queued_command`, a `user` string): panorama's `wire.notice_text`.
    pub fn notice_text(&self) -> Option<&str> {
        match self.record() {
            Record::QueueEnqueue { content } => content,
            Record::Attachment(Attachment::QueuedCommand { prompt }) => prompt,
            Record::User { .. } => self.string_content(),
            _ => None,
        }
    }

    /// An operator prompt: panorama's `wire.is_human_prompt` (zoetrope's).
    /// `origin.kind == "human"`; a legacy record without an origin counts when
    /// its string content is not a notification or a local command. Meta
    /// records are never prompts.
    pub fn is_human_prompt(&self) -> bool {
        if self.kind() != Some("user") || self.get("isMeta").is_some_and(truthy) {
            return false;
        }
        let Some(text) = self.string_content() else {
            return false;
        };
        match self
            .get("origin")
            .and_then(Value::as_object)
            .and_then(|o| o.get("kind"))
        {
            Some(kind) if !kind.is_null() => kind.as_str() == Some("human"),
            _ => {
                let t = text.trim_start_matches(py_isspace);
                !["<task-notification>", "<local-command", "<command-"]
                    .iter()
                    .any(|p| t.starts_with(p))
            }
        }
    }

    /// The record this line holds. Lenient: an unknown type, or a known one
    /// whose fields do not have the expected shape, is [`Record::Other`].
    pub fn record(&self) -> Record<'_> {
        let Some(kind) = self.kind() else {
            return Record::Other;
        };
        match kind {
            "assistant" => Record::Assistant {
                model: self
                    .message()
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str),
                blocks: self.blocks(),
            },
            "user" => Record::User {
                content: match self.message().and_then(|m| m.get("content")) {
                    Some(Value::String(s)) => Content::Text(s),
                    Some(Value::Array(_)) => Content::Blocks(self.blocks()),
                    _ => Content::None,
                },
                tool_use_result: self.get("toolUseResult"),
            },
            "queue-operation" if self.str_field("operation") == Some("enqueue") => {
                Record::QueueEnqueue {
                    content: self.str_field("content"),
                }
            }
            "attachment" => {
                Record::Attachment(match self.get("attachment").and_then(Value::as_object) {
                    Some(att) => match att.get("type").and_then(Value::as_str) {
                        Some("queued_command") => Attachment::QueuedCommand {
                            prompt: att.get("prompt").and_then(Value::as_str),
                        },
                        Some("hook_additional_context") => Attachment::HookAdditionalContext {
                            content: match att.get("content") {
                                Some(Value::Array(items)) => {
                                    items.iter().filter_map(Value::as_str).collect()
                                }
                                Some(Value::String(s)) => vec![s.as_str()],
                                _ => Vec::new(),
                            },
                        },
                        other => Attachment::Other(other),
                    },
                    None => Attachment::Other(None),
                })
            }
            "continued-in" => Record::ContinuedIn {
                from: self.get("sessionId").and_then(SessionId::from_value),
                to: self
                    .get("continuedInSessionId")
                    .and_then(SessionId::from_value),
                sidechain: self.is_sidechain(),
            },
            "custom-title" | "agent-name" | "ai-title" => {
                let (title, key) = match kind {
                    "custom-title" => (TitleKind::Custom, "customTitle"),
                    "agent-name" => (TitleKind::Agent, "agentName"),
                    _ => (TitleKind::Ai, "aiTitle"),
                };
                match self.str_field(key) {
                    Some(v) => Record::Title(title, v),
                    None => Record::Other,
                }
            }
            k if BOOKKEEPING.contains(&k) => Record::Bookkeeping(k),
            _ => Record::Other,
        }
    }
}

/// What one transcript line says, borrowed from its [`Line`]. The time is
/// [`Line::ts`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Record<'a> {
    /// A model turn: its model and content blocks.
    Assistant {
        /// `message.model`, when a string.
        model: Option<&'a str>,
        /// The object blocks of `message.content`.
        blocks: Vec<Block<'a>>,
    },
    /// A user record: a typed prompt, a notification, or tool results.
    User {
        /// `message.content`.
        content: Content<'a>,
        /// `toolUseResult`: an object, or a plain string for some errors.
        tool_use_result: Option<&'a Value>,
    },
    /// A `queue-operation` `enqueue`: a prompt queued mid-turn.
    QueueEnqueue {
        /// `content`, when a string.
        content: Option<&'a str>,
    },
    /// An `attachment` record.
    Attachment(Attachment<'a>),
    /// A `continued-in` record: the conversation moved on to another session
    /// id. **Raw**: whether it counts is the continuation rule's decision
    /// ([`super::Continuation`]).
    ContinuedIn {
        /// `sessionId`, when it is a session id.
        from: Option<SessionId>,
        /// `continuedInSessionId`, when it is a session id.
        to: Option<SessionId>,
        /// `isSidechain: true`.
        sidechain: bool,
    },
    /// A `custom-title`, `agent-name` or `ai-title` record with a string value.
    Title(TitleKind, &'a str),
    /// A known bookkeeping record ([`BOOKKEEPING`]), by type.
    Bookkeeping(&'a str),
    /// Anything else, including types Claude Code adds later.
    Other,
}

/// The three title records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TitleKind {
    /// `custom-title` (`customTitle`): set by the user.
    Custom,
    /// `agent-name` (`agentName`).
    Agent,
    /// `ai-title` (`aiTitle`): generated.
    Ai,
}

/// `message.content` of a user record.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Content<'a> {
    /// A string (a typed prompt, a notification, a local command).
    Text(&'a str),
    /// A list: its object blocks.
    Blocks(Vec<Block<'a>>),
    /// Missing or another shape.
    None,
}

/// The `attachment` of an attachment record.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Attachment<'a> {
    /// `queued_command`: a prompt typed while the model was busy.
    QueuedCommand {
        /// `prompt`, when a string.
        prompt: Option<&'a str>,
    },
    /// `hook_additional_context`: text a hook added (`content`, its strings).
    HookAdditionalContext {
        /// The strings of `content` (a list, or one string).
        content: Vec<&'a str>,
    },
    /// Another attachment type (or none).
    Other(Option<&'a str>),
}

/// One object block of `message.content`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Block<'a> {
    /// `text`.
    Text(Option<&'a str>),
    /// `tool_use`: a tool call.
    ToolUse {
        /// `id`, when a non-empty string.
        id: Option<&'a str>,
        /// `name`, when a non-empty string.
        name: Option<&'a str>,
        /// `input`, when an object.
        input: Option<&'a Map<String, Value>>,
    },
    /// `tool_result`: the result of a call.
    ToolResult {
        /// `tool_use_id`, when a non-empty string.
        tool_use_id: Option<&'a str>,
        /// `is_error`, when a boolean.
        is_error: Option<bool>,
        /// `content`: a string or a list of blocks.
        content: Option<&'a Value>,
    },
    /// Another block type, by type.
    Other(Option<&'a str>),
}

fn non_empty_str<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

impl<'a> Block<'a> {
    fn of(b: &'a Map<String, Value>) -> Block<'a> {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => Block::Text(b.get("text").and_then(Value::as_str)),
            Some("tool_use") => Block::ToolUse {
                id: non_empty_str(b, "id"),
                name: non_empty_str(b, "name"),
                input: b.get("input").and_then(Value::as_object),
            },
            Some("tool_result") => Block::ToolResult {
                tool_use_id: non_empty_str(b, "tool_use_id"),
                is_error: b.get("is_error").and_then(Value::as_bool),
                content: b.get("content"),
            },
            other => Block::Other(other),
        }
    }
}

// -- task notifications and question ids --------------------------------------------------------

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

/// The text of a `tool_result`'s `content`: a string as it is, a list of
/// blocks as their `text`s joined by spaces (panorama's `wire.text_of`). A
/// `text` that is not a string reads as Python's `str()` of it; anything else is no
/// text.
pub fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_object)
            .map(|b| match b.get("text") {
                Some(t) if truthy(t) => py_str(t),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// Python's `str()` of a JSON value, for the few places panorama converts a
/// field it does not check: a string as it is, `None`, `True`/`False`, an
/// integer, a float as Python's `repr`. An array or object reads as its JSON
/// text (Python prints its `repr`; no field this is used for holds one in a
/// real transcript).
pub(crate) fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => py_float_repr(f),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// Python's `repr` of a float: the shortest text that reads back as the same
/// value, positional for exponents from -4 to 15, scientific otherwise.
pub(crate) fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    // `{:e}` is the shortest round-trip digits in scientific form: "1.5e-5"
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent");
    let (neg, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let sign = if neg { "-" } else { "" };
    if !(-4..16).contains(&exp) {
        let (head, tail) = digits.split_at(1);
        let frac = if tail.is_empty() {
            String::new()
        } else {
            format!(".{tail}")
        };
        let esign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{head}{frac}e{esign}{:02}", exp.abs());
    }
    let point = exp + 1; // digits before the decimal point
    let body = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
        let (a, b) = digits.split_at(point as usize);
        format!("{a}.{b}")
    };
    format!("{sign}{body}")
}

// -- summaries ---------------------------------------------------------------------------------

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

/// One tool call still waiting for its result (or the last call made).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ToolCall {
    /// The tool name (`"?"` when the record has none).
    pub name: String,
    /// Its `input` object (empty when the record has none).
    pub input: Map<String, Value>,
    /// The record's own time.
    pub ts: Option<Ts>,
}

/// What the end of a transcript says: activity, calls in flight, last text,
/// model and times. Panorama's `TailInfo`, read by
/// [`super::io::summarize_tail`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TailInfo {
    /// The first line's own time.
    pub first_ts: Option<Ts>,
    /// The time of the last dated `user`, `assistant` or `system` record.
    pub last_ts: Option<Ts>,
    /// The last assistant model (placeholders such as `<synthetic>` skipped).
    pub model: Option<String>,
    /// The last non-blank assistant text, trimmed.
    pub last_text: String,
    /// [`summarize_tool`] of the newest call in flight, else of the last call.
    pub activity: String,
    /// Calls without a result yet, oldest first.
    pub pending: Vec<ToolCall>,
    /// The agent called `SubagentHandback`.
    pub handback: bool,
}

impl TailInfo {
    /// Folds the tail window's lines (each one raw line, partial first line
    /// already dropped), with the file's first line `head` when it was read
    /// whole. Pure; [`super::io::summarize_tail`] does the reading.
    pub fn from_lines<L: AsRef<[u8]>>(
        head: Option<&[u8]>,
        lines: &[L],
        cwd: Option<&str>,
    ) -> TailInfo {
        let mut info = TailInfo {
            first_ts: head.and_then(Line::parse).and_then(|l| l.ts()),
            ..TailInfo::default()
        };
        // first-insertion order, as a Python dict keeps it when a key is set again
        let mut order: Vec<String> = Vec::new();
        let mut uses: std::collections::HashMap<String, ToolCall> =
            std::collections::HashMap::new();
        let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_use: Option<String> = None;
        for raw in lines {
            let raw = raw.as_ref();
            if blank(raw) {
                continue;
            }
            let Some(rec) = Line::parse(raw) else {
                continue;
            };
            let when = rec.ts();
            let kind = rec.kind();
            if when.is_some() && matches!(kind, Some("user" | "assistant" | "system")) {
                info.last_ts = when;
            }
            let assistant = kind == Some("assistant");
            if assistant
                && let Some(model) = rec
                    .message()
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                && !model.starts_with('<')
            {
                info.model = Some(model.to_string());
            }
            for b in rec.blocks() {
                match b {
                    Block::ToolUse {
                        id: Some(id),
                        name,
                        input,
                    } if assistant => {
                        let call = ToolCall {
                            name: name.unwrap_or("?").to_string(),
                            input: input.cloned().unwrap_or_default(),
                            ts: when,
                        };
                        if uses.insert(id.to_string(), call).is_none() {
                            order.push(id.to_string());
                        }
                        last_use = Some(id.to_string());
                        if name == Some("SubagentHandback") {
                            info.handback = true;
                        }
                    }
                    Block::Text(Some(text)) if assistant && !py_strip(text).is_empty() => {
                        info.last_text = py_strip(text).to_string();
                    }
                    Block::ToolResult {
                        tool_use_id: Some(id),
                        ..
                    } => {
                        done.insert(id.to_string());
                    }
                    _ => {}
                }
            }
        }
        info.pending = order
            .iter()
            .filter(|id| !done.contains(*id))
            .filter_map(|id| uses.get(id).cloned())
            .collect();
        let current = info
            .pending
            .last()
            .cloned()
            .or_else(|| last_use.and_then(|id| uses.remove(&id)));
        if let Some(call) = current {
            info.activity = summarize_tool(&call.name, Some(&call.input), cwd);
        }
        info
    }
}

#[cfg(test)]
mod tests;
