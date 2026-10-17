# herdr-agent-kit

Shared Rust library for the herdr plugins
[herdr-panorama](https://github.com/LeaveMyYard/herdr-panorama) and
[herdr-inbox](https://github.com/LeaveMyYard/herdr-inbox).

Rust building blocks for tools around coding agents in [herdr](https://herdr.dev). Claude Code is
the first supported harness; other harnesses are planned, not built yet. The API is 0.x and follows
semver: before 1.0 a breaking change bumps the minor version. The kit is pre-0.1.0 and has no
tag yet; the plugins will depend on it by tag.

The two plugins are separate repos and each runs without the other. The code
whose rules must not drift between them lives here, once: the herdr API client,
Claude Code session parsing, agent process checks, the plugin environment and
the inbox's published formats.

It is **one library crate** (no binaries, no plugin manifests), so there is one
thing to tag and bump. Edition 2024, MSRV 1.96 (pinned in `rust-toolchain.toml`).
Targets herdr 0.9.4 or newer.

## API

The public API is what `cargo doc` shows; everything else is private to the
crate. Run `cargo doc --open --all-features` for the full reference.

| Module | Feature | Pure or IO | Purpose |
|---|---|---|---|
| `herdr_api` | always | IO | The only code that talks to herdr: socket client (async `Herdr`, `blocking::Herdr`), events, the CLI path, `HerdrError`, `Sanitized` |
| `herdr_api::fake` | `test-support` | test IO | `FakeHerdr`, a scripted herdr socket server that records requests |
| `agent_session` | always | pure | The harness-neutral seam: `SessionId` (the one session-id validator), `Ts`, `Status`, `SessionView` |
| `agent_session::tail` | always | IO | Harness-neutral incremental reading of an append-only file (`Tailer`) |
| `agent_session::claude` | always | pure | Claude Code: the wire format, the continuation rule (D6), the session fold and status rules, `Session::view` (the `SessionView` producer) |
| `agent_session::claude::io`, `::scan` | always | IO | Projects root, transcript discovery, tailing, following continuations, `Scanner` |
| `agent_procs` | always | IO | Neutral process table walks, pid checks, the `Liveness` trait |
| `agent_procs::claude` | always | IO | Claude Code: "is this Claude Code", nested Claude, `SystemLiveness` (background tasks) |
| `agent_procs::fake` | `test-support` | pure | Scripted process tables and liveness for tests |
| `plugin_env` | always | IO | Plugin environment, `config.toml` with reload (`ConfigFile::load_as` into any serde type), flock, private socket dirs |

Pure code takes data and the current time and returns data: no IO, no threads,
no environment. No feature is on by default.

**Harness-neutral seam.** Only `agent_session::claude` reads Claude Code's
files or parses its wire format. It hands out neutral data (`SessionView`,
`SessionId`, `Status`), which is all a plugin's core should consume. Claude
Code is the only harness so far; there is deliberately no `Harness` trait
until a second one is built.

**Not public API.** The `internal` feature turns on the `internal` module
(hidden from the docs): `internal::inbox_contract`, herdr-inbox's published
formats (`[inbox] …` text, the `ask_operator` receipt, `inbox open` args), and
with `sqlite` its read-only reader of the 0.1 `questions` columns; and
`internal::plugin_paths`, the two plugins' ids and state layouts (lock, socket
and state paths). It exists for the two plugins only and is **outside
semver**: it can change in any release.

**Semver.** Types expected to grow are `#[non_exhaustive]`. Public
dependencies are `serde`/`serde_json` 1.x and `tokio` 1.x only. The package
holds `src/` and the herdr recordings `FakeHerdr` embeds; CI builds the
packaged crate with every feature.

`fixtures/` holds captured test inputs (transcripts, herdr socket recordings,
inbox-format goldens); see [fixtures/README.md](fixtures/README.md).

## Using it from a plugin

Plugins depend on the kit as a git dependency **pinned by tag** and enable only
the features they use:

```toml
[dependencies]
herdr-agent-kit = { git = "https://github.com/LeaveMyYard/herdr-agent-kit", tag = "v0.1.0" }

[dev-dependencies]
herdr-agent-kit = { git = "https://github.com/LeaveMyYard/herdr-agent-kit", tag = "v0.1.0", features = ["test-support"] }
```

`Cargo.lock` in the plugin records the exact commit, so builds are reproducible.
Never depend on a branch.

To build a plugin against a local kit checkout, use a patch in the plugin's
`.cargo/config.toml` (do not commit it) or on the command line:

```sh
cargo test --config 'patch."https://github.com/LeaveMyYard/herdr-agent-kit".herdr-agent-kit.path="../herdr-agent-kit"'
```

## Releasing and bumping

1. Changes land on `main` through PRs.
2. A release PR bumps `version` in `Cargo.toml` (and `Cargo.lock`). Before 1.0, a
   breaking change bumps the minor version.
3. After it merges, tag that commit on `main`: `git tag vX.Y.Z && git push origin vX.Y.Z`.
   The release workflow checks that the tag matches `Cargo.toml`, runs the
   tests, and creates the GitHub release with generated notes. The kit ships
   source only; there are no binary assets.
4. Each plugin moves to the new tag in its own bump PR (change `tag`, then
   `cargo update -p herdr-agent-kit`), on its own schedule. A kit release never
   forces a release of either plugin.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build, test and send a PR, and follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report security problems as described in [SECURITY.md](SECURITY.md).

## License

MIT, see [LICENSE](LICENSE).
