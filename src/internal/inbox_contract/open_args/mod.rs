//! The `inbox open` arguments: how another plugin opens the inbox popup on a question
//! or a pane's questions. **Pure.**
//!
//! `inbox open [--question Qn | --group G | --pane P | --session S] [--no-hold]` writes a
//! one-shot focus request (`{kind, value, hold}`) next to the inbox database and opens
//! the popup, which starts on that target.
//!
//! - Writing (herdr-panorama's `inbox_link.py`): [`deep_link_args`], with
//!   [`supports_deep_link`] and [`supports_no_hold`] deciding from the inbox's version
//!   and its `inbox open --help`.
//! - Reading (herdr-inbox's `cli.py` and `deeplink.py`): [`parse_open_args`] and
//!   [`valid`], giving the request the popup is opened for.
//! - Question references (herdr-inbox's `deeplink.question_ref`, `question_id` and
//!   `describe`, herdr-inbox#57): [`question_ref`], [`question_id`] and
//!   [`describe_question`]. A reference is `[machine:]` plus at most one `Q`/`q` plus
//!   the number; the inbox stores it without the `Q` (`qnap:Q12` → `qnap:12`, `Q12` →
//!   `12`), and readers take both forms.
//!
//! Goldens: `fixtures/inbox/open-args.json`.

mod deep_link;
mod parse;
mod question;
mod request;

pub use deep_link::{DEEP_LINK_VERSION, deep_link_args, supports_deep_link, supports_no_hold};
pub use parse::{parse_open_args, valid};
pub use question::{describe_question, question_id, question_ref};
pub use request::{OpenArgsError, OpenKind, OpenRequest, OpenTarget};

#[cfg(test)]
mod tests;
