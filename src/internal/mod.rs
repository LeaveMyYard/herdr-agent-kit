//! Plugin-only parts of the kit. **Not public API, outside semver.**
//!
//! Behind the `internal` feature and hidden from the docs. These modules exist
//! for herdr-panorama and herdr-inbox and change whenever those plugins need
//! it, in any release, without a version bump that signals it. Do not depend
//! on them from other code.
//!
//! - [`inbox_contract`]: herdr-inbox's published formats, which herdr-panorama
//!   reads (plan §2.1).
//! - [`plugin_paths`]: the two plugins' plugin ids and state layouts (lock,
//!   socket and state paths).

pub mod inbox_contract;
pub mod plugin_paths;
