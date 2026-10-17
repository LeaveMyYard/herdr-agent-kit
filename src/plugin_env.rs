//! The plugin's environment. **IO** (environment, files, locks).
//!
//! The herdr plugin environment variables, `config.toml` with reload, state
//! and config directories, `flock`-based locks and private socket directories.
//! Code outside this module and [`crate::herdr_api`] does not read the
//! environment.
