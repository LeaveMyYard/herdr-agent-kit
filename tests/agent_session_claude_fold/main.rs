//! The status rules of herdr-panorama's `scan/session.py` (its docstring,
//! lines 1-44), as a table: each row builds a session from captured
//! templates (`tests/common`, as panorama's `tests/scenario.py` does), reads
//! it with [`Scanner`] and asks [`Session::status_of`] for one agent at one
//! instant.
//!
//! The hook records are panorama's own v1 format (`hooks.py` `_append` /
//! `record`), not a Claude Code wire format.
//!
//! One topic per file: `templates`, `transcript` and `scenario` build
//! sessions; `rows` and `rows_*` are the status-rules table; `copies`, `status`,
//! `parentage`, `order`, `questions` and `updates` are the other cases.

#[path = "../common/mod.rs"]
mod common;

// Building sessions from captured templates.
mod scenario;
mod templates;
mod transcript;
mod transcript_background;

// The status-rules table.
mod rows;
mod rows_background;
mod rows_evidence;
mod rows_hooks;
mod rows_liveness;
mod rows_reopen;
mod rows_review;

// The other cases.
mod copies;
mod order;
mod parentage;
#[cfg(feature = "internal")]
mod questions;
mod status;
mod updates;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use common::{BASE, CWD, SLUG, tmp_dir, tpl};
use herdr_agent_kit::agent_procs::fake::FakeLiveness;
use herdr_agent_kit::agent_procs::{Liveness, NoProcs, ProcessIdentity};
use herdr_agent_kit::agent_session::claude::Scanner;
use herdr_agent_kit::agent_session::claude::fold::{
    ACTIVE, ASYNC_QUIET, BG_CAP, MONITOR_SLACK, Owner, PENDING_CAP, REOPEN_MARGIN,
};
use herdr_agent_kit::agent_session::{SessionId, Status, Ts};
use serde_json::{Map, Value, json};

const SID: &str = "55555555-5555-4555-8555-555555555555";
const M: f64 = 60.0;
const B: f64 = BASE as f64;

use rows::*;
use scenario::{Sc, bg_agent};
use templates::*;
use transcript::Tx;
use updates::{read_ts, set_mtime};
