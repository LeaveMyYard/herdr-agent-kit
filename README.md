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
Claude Code session parsing, agent process checks, the plugin environment,
remote machines, the TUI toolkit and the inbox's published formats.

It is **one library crate** (no binaries, no plugin manifests), so there is one
thing to tag and bump. Edition 2024, MSRV 1.96 (pinned in `rust-toolchain.toml`).
Targets herdr 0.9.4 or newer.

## Modules and features

| Module | Feature | Pure or IO | Purpose |
|---|---|---|---|
| `herdr_api` | always | IO | The only code that talks to herdr: socket client, the CLI path, `HerdrError`, `Sanitized` |
| `herdr_api::fake` | `fake` | test IO | `FakeHerdr`, a scripted herdr socket server that records requests |
| `claude_session::fold` | always | pure | Session fold, status rules, the continuation rule |
| `claude_session::io` | always | IO | Projects root, transcript discovery, tailing |
| `agent_procs` | always | IO | "Is this Claude Code", nested Claude, liveness |
| `plugin_env` | always | IO | Plugin environment, `config.toml` with reload, state dirs, flock, socket dirs |
| `remote_machines` | `remote` | IO | Machine list, ssh options, the shared ControlPath scheme |
| `tui_kit` | `tui` | IO | Terminal setup, arming, identity cursors, click targets, theme |
| `inbox_contract` | always | pure | The inbox's published formats: `[inbox] …` text, `ask_operator` receipt, `inbox open` args |
| `inbox_contract::sqlite` | `sqlite` | IO | Read-only reader of the 0.1 `questions` columns |

Pure code takes data and the current time and returns data: no IO, no threads,
no environment. No feature is on by default.

`fixtures/` holds captured test inputs (transcripts, herdr socket recordings,
inbox-format goldens); see [fixtures/README.md](fixtures/README.md).

## Using it from a plugin

Plugins depend on the kit as a git dependency **pinned by tag** and enable only
the features they use:

```toml
[dependencies]
herdr-agent-kit = { git = "https://github.com/LeaveMyYard/herdr-agent-kit", tag = "v0.1.0", features = ["sqlite"] }

[dev-dependencies]
herdr-agent-kit = { git = "https://github.com/LeaveMyYard/herdr-agent-kit", tag = "v0.1.0", features = ["fake"] }
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
