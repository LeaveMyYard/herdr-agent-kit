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

mod line;
mod markers;
mod notice;
mod record;
mod tail;
mod timestamp;
mod tool_summary;
mod tools;

pub use line::Line;
pub(crate) use markers::contains;
pub use markers::{MARKERS, PROMPT_MARKERS, wanted, wanted_lines};
pub use notice::{Ending, Notice, ORPHAN, parse_notice, parse_notices};
pub use record::{Attachment, BOOKKEEPING, Block, Content, Record, TitleKind, text_of};
pub use tail::{TailInfo, ToolCall};
pub use timestamp::ts;
pub use tool_summary::{SUMMARY_MAX, short_path, summarize_tool, truncate_summary};
pub(crate) use tools::{BASH, MONITOR};
pub use tools::{BG_STOP_TOOLS, SPAWN_TOOLS};
