//! Rust building blocks for tools around coding agents in
//! [herdr](https://herdr.dev), shared by the herdr plugins
//! [herdr-panorama](https://github.com/LeaveMyYard/herdr-panorama) and
//! [herdr-inbox](https://github.com/LeaveMyYard/herdr-inbox).
//!
//! Claude Code is the first supported agent harness; other harnesses are
//! planned, and the API keeps a harness-neutral seam for them
//! ([`agent_session`]).
//!
//! # Modules
//!
//! | Module | Pure or IO | Purpose |
//! |---|---|---|
//! | [`herdr_api`] | IO | The herdr client: socket, events, the CLI path, [`herdr_api::HerdrError`], [`herdr_api::Sanitized`] |
//! | [`agent_session`] | pure | Neutral session types: [`agent_session::SessionId`], [`agent_session::Status`], [`agent_session::SessionView`] |
//! | [`agent_session::tail`] | IO | Neutral incremental reading of an append-only file: [`agent_session::tail::Tailer`] |
//! | [`agent_session::claude`] | pure and IO | Claude Code's transcripts: wire format, continuation, the session fold and status rules, discovery and tailing |
//! | [`agent_procs`] | IO | Process table walks and liveness; [`agent_procs::claude`]: "is this Claude Code", nested Claude |
//! | [`plugin_env`] | IO | The plugin environment, `config.toml` with reload, state dirs, flock, private socket dirs |
//!
//! # Features
//!
//! No feature is on by default.
//!
//! | Feature | Enables |
//! |---|---|
//! | `test-support` | Test fakes: `herdr_api::fake` (`FakeHerdr`, a scripted herdr socket server) and `agent_procs::fake` (scripted process tables and liveness) |
//! | `internal` | Plugin-only parts (the inbox's formats, the plugins' state layouts), **not public API and outside semver** (hidden from these docs) |
//! | `sqlite` | A plugin-only SQLite reader inside `internal` (turns on `internal`) |
//!
//! # Purity boundary
//!
//! Pure code takes data and the current time and returns data: it does no IO,
//! starts no threads and reads no environment. The pure parts are
//! [`agent_session`]'s neutral types and, in [`agent_session::claude`], the
//! wire format, the continuation rule and the fold. Everything else is IO:
//! [`herdr_api`], [`agent_session::tail`], [`agent_session::claude::io`] and
//! [`agent_session::claude::scan`], [`agent_procs`] and [`plugin_env`].
//!
//! # Example
//!
//! Follow a Claude Code session's continuations and hand a plugin's core the
//! neutral view (this reads `~/.claude/projects`, so it is not run here):
//!
//! ```no_run
//! use herdr_agent_kit::agent_session::claude::io::{ProjectsRoot, Transcripts};
//! use herdr_agent_kit::agent_session::claude::{Scanner, chain, continuation::MAX_HOPS};
//! use herdr_agent_kit::agent_session::{SessionId, Status};
//!
//! let id = SessionId::parse("0e4a6b1c-9d2f-4a3b-8c7d-1e2f3a4b5c6d").unwrap();
//! let root = ProjectsRoot::from_env().expect("a home directory");
//! let transcript = root.find(&id, None).expect("the session's transcript");
//!
//! let mut scanner = Scanner::new(id.clone(), transcript, None, None);
//! scanner.update(None);
//! let mut files = Transcripts::new(root, None);
//! let hops = chain(&id, &mut files, MAX_HOPS);
//!
//! let view = scanner.session().view(Status::Run, None, Vec::new(), &hops);
//! println!("{} {:?} -> {} successors", view.id, view.title, view.successors.len());
//! ```
//!
//! # Versioning
//!
//! The API is 0.x and follows semver: before 1.0 a breaking change bumps the
//! minor version. The `internal` module is excluded: it changes whenever the
//! two plugins need it. So are the `test-support` fakes, which follow the
//! tests' needs. Types that are expected to grow (statuses, errors,
//! records, herdr's responses, [`agent_session::SessionView`]) are
//! `#[non_exhaustive]`; the ones callers build have constructors.
//!
//! Public dependencies (their types appear in this API): `serde` and
//! `serde_json` 1.x (JSON values in records and herdr responses, `Deserialize`
//! for settings), and `tokio` 1.x (the async herdr client needs a tokio
//! runtime). No other dependency's types are exposed. The design this follows is
//! `docs/plans/rust-rewrite.md` in herdr-panorama (sections 2.1 to 2.3).

#![warn(missing_docs)]

pub mod agent_procs;
pub mod agent_session;
mod ask_operator;
pub mod herdr_api;
#[cfg(feature = "internal")]
#[doc(hidden)]
pub mod internal;
pub mod plugin_env;
mod pytext;
