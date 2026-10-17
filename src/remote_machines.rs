//! Remote machines, behind the `remote` feature. **IO** (herdr CLI, ssh).
//!
//! The machine list (through the herdr CLI path), ssh options and the
//! ControlPath naming scheme. Both plugins use the same scheme, so when both
//! run, ssh's `ControlMaster=auto` makes them share one master per host. Each
//! plugin keeps its own pull logic (panorama's rsync mirror, the inbox's
//! `export`/`apply`).
