//! Python's text and value semantics, one implementation for the whole crate.
//! **Pure; crate-private.**
//!
//! The formats the kit reads and writes were written by Python (herdr-panorama,
//! herdr-inbox), so the ports follow Python's rules, never Rust's own
//! (`char::is_whitespace` lacks U+001C to U+001F, `str::lines` splits on fewer
//! characters, JSON truthiness has no Rust counterpart):
//!
//! - [`chars`]: character classes (`str.isspace`, `str.splitlines` breaks,
//!   `\w`);
//! - [`strings`]: string operations (`str.strip`, `str.splitlines`,
//!   `bytes.strip`, `len`, `int()`);
//! - [`values`]: JSON values as Python sees them (truthiness, `str()`, float
//!   `repr`);
//! - [`fields`]: reading one field of a JSON object as panorama does.

mod chars;
mod fields;
mod strings;
mod values;

#[cfg(feature = "internal")]
pub(crate) use chars::py_line_break;
pub(crate) use chars::{py_isspace, py_word};
pub(crate) use fields::{non_empty, number, py_int, str_or_empty};
pub(crate) use strings::{ascii_digits, blank, parse_id, py_splitlines, py_strip};
#[cfg(feature = "internal")]
pub(crate) use strings::{py_int_str, py_len};
pub(crate) use values::{py_str, truthy};
