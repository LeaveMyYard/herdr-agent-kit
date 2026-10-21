# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Versioning before 1.0.** A breaking change bumps the minor version (0.1 to
0.2); anything else bumps the patch. The `internal` and `test-support` features
are outside semver: they can change in any release.

**Distribution.** The kit is not published to crates.io. Depend on it as a git
dependency pinned by tag, for example
`herdr-agent-kit = { git = "https://github.com/LeaveMyYard/herdr-agent-kit", tag = "v0.1.0" }`.

## [Unreleased]

## [0.1.3] - 2026-10-21

### Changed

- Tests only: the event stream's backoff resets on herdr data (a truthy `result` or `data`) and on nothing else, and a connection closed before any ack keeps it growing (#38). No behaviour change.

## [0.1.2] - 2026-10-21

### Added

- `herdr_api`: `with_observer(impl Fn(&CallTrace))` on both clients, and the
  `herdr_api::observe` module (`CallTrace`, `Transport`). The observer hears one
  `CallTrace` (transport, op, start time, duration) per socket request and per CLI run:
  answered, failed, timed out, or dropped unfinished. It runs outside any lock; a panic in
  it is caught and never reaches the call. For timing slow herdr calls (panorama's
  `PANORAMA_TRACE`).

### Changed

- The reconnect backoff growth is a pure function with a test (0.5, 1, 2, 4, 8, 10, 10 s); no behaviour change (#33).

## [0.1.1] - 2026-10-21

### Added

- Tests for the 15 panorama session-fold rules no kit test checked: P1 parity coverage (#29, #31). No code or API change.

## [0.1.0] - 2026-10-18

First release.

### Added

- `herdr_api`: the herdr client (async `Herdr` and `blocking::Herdr`) over the socket, the event stream, the CLI path, `HerdrError` and `Sanitized`. Needs herdr 0.9.4 or newer.
- `agent_session`: harness-neutral session types: `SessionId`, `Ts`, `Status` and `SessionView`.
- `agent_session::tail`: incremental reading of an append-only file (`Tailer`).
- `agent_session::claude`: Claude Code transcripts: the wire format, the continuation rule, the session fold and status rules, and `Session::view`.
- `agent_session::claude::io` and `::scan`: the projects root, transcript discovery, tailing, following continuations and `Scanner`.
- `agent_procs`: process table walks, pid checks and the `Liveness` trait.
- `agent_procs::claude`: "is this Claude Code", nested Claude detection and `SystemLiveness`.
- `plugin_env`: the plugin environment, `config.toml` with reload (`ConfigFile::load_as`), flock and private socket dirs.
- `test-support` feature: `herdr_api::fake` (`FakeHerdr`, a scripted herdr socket server) and `agent_procs::fake` (scripted process tables and liveness).
- `internal` feature (plugin-only, outside semver): herdr-inbox's published formats (`internal::inbox_contract`) and the plugins' state layouts (`internal::plugin_paths`).
- `sqlite` feature (plugin-only, turns on `internal`): a read-only reader of the inbox's 0.1 `questions` columns.

[Unreleased]: https://github.com/LeaveMyYard/herdr-agent-kit/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/LeaveMyYard/herdr-agent-kit/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/LeaveMyYard/herdr-agent-kit/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/LeaveMyYard/herdr-agent-kit/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/LeaveMyYard/herdr-agent-kit/releases/tag/v0.1.0
