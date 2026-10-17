//! The inbox's published formats.
//!
//! herdr-inbox owns these formats; herdr-panorama only reads them.
//!
//! - The formats are **pure** parsing and encoding: the `[inbox] …` delivery
//!   text, the `ask_operator` receipt and the `inbox open` arguments.
//! - `sqlite` (behind the `sqlite` feature) is **IO**: a read-only reader of
//!   the 0.1 `questions` columns. A missing file, a lock or a missing table
//!   reads as "no inbox".
//!
//! The canonical goldens for these formats live in this repo's `fixtures/`.

#[cfg(feature = "sqlite")]
pub mod sqlite;
