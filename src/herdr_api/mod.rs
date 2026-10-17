//! The only code that talks to herdr. **IO.**
//!
//! - **Socket** for every method herdr's socket API has: `session.snapshot`,
//!   `pane.*`, `agent.*`, `tab.*`, `layout.*`, `plugin.pane.*`, `plugin.list`,
//!   `plugin.action.invoke`, `events.subscribe`, `notification.show` and
//!   `pane.report_metadata`.
//! - **CLI** (`$HERDR_BIN_PATH`) only for what the socket lacks:
//!   `herdr machine list --json`, `herdr --machine <id> api snapshot` and
//!   `herdr plugin config-dir <id>`, with the same timeouts and error mapping.
//!
//! Planned types: the id newtypes (`PaneId`, `TabId`, `WorkspaceId`,
//! `ServerId`), `HerdrError` (`NotRunning`, `Refused`, `NoReply`, `Garbled`),
//! the `Herdr` client with one typed method per raw method, and `Sanitized`,
//! the only way to build a prompt or label (its constructor is `clean()`).
//!
//! Minimum herdr version: 0.9.4.

#[cfg(feature = "fake")]
pub mod fake;
