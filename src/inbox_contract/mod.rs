//! The inbox's published formats.
//!
//! herdr-inbox owns these formats; herdr-panorama only reads them.
//!
//! - The formats are **pure** parsing and encoding: the `[inbox] …` delivery
//!   text ([`delivery`]), the `ask_operator` receipt ([`receipt`]) and the
//!   `inbox open` arguments ([`open_args`]).
//! - `sqlite` (behind the `sqlite` feature) is **IO**: a read-only reader of
//!   the 0.1 `questions` columns. A missing file, a lock or a missing table
//!   reads as "no inbox".
//!
//! The canonical goldens for these formats live in this repo's `fixtures/inbox/`,
//! captured from the Python plugins and the herdr-inbox-ui mod (its `capture/`
//! directory re-creates them). herdr-inbox vendors a copy for the mod's tests.
//!
//! These are byte-for-byte ports, quirks included: where Python and TypeScript read a
//! format differently, each function says which one it follows.

pub mod delivery;
pub mod open_args;
pub mod receipt;
#[cfg(feature = "sqlite")]
pub mod sqlite;
mod text;
