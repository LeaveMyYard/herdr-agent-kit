//! Shared library for the herdr plugins
//! [herdr-panorama](https://github.com/LeaveMyYard/herdr-panorama) and
//! [herdr-inbox](https://github.com/LeaveMyYard/herdr-inbox).
//!
//! One crate with modules, so there is one thing to tag and bump. Each plugin
//! depends on it as a git dependency pinned by tag and enables only the
//! features it uses:
//!
//! | Feature  | Enables |
//! |----------|---------|
//! | `fake`   | `herdr_api::fake`: `FakeHerdr`, a scripted herdr socket server for tests |
//! | `tui`    | `tui_kit`: terminal setup, arming, identity cursors, click targets, theme |
//! | `remote` | `remote_machines`: machine list, ssh options, ControlPath scheme |
//! | `sqlite` | `inbox_contract::sqlite`: read-only reader of the 0.1 `questions` columns |
//!
//! # Purity boundary
//!
//! Pure code takes data and the current time and returns data: it does no IO,
//! starts no threads and reads no environment. In this crate the pure parts are
//! [`claude_session::fold`] and the formats in [`inbox_contract`]. Everything
//! else is IO: [`herdr_api`], [`claude_session::io`], [`agent_procs`],
//! [`plugin_env`], `remote_machines` and `inbox_contract::sqlite`.
//!
//! The plan this follows is `docs/plans/rust-rewrite.md` in herdr-panorama
//! (sections 2.1 to 2.3).

pub mod agent_procs;
pub mod claude_session;
pub mod herdr_api;
pub mod inbox_contract;
pub mod plugin_env;
#[cfg(feature = "remote")]
pub mod remote_machines;
#[cfg(feature = "tui")]
pub mod tui_kit;
