# Contributing

Thanks for helping. Bug reports, fixes, docs and ideas are all welcome. This project follows the
[Code of Conduct](CODE_OF_CONDUCT.md). The kit is a library for the herdr plugins
[herdr-panorama](https://github.com/LeaveMyYard/herdr-panorama) and
[herdr-inbox](https://github.com/LeaveMyYard/herdr-inbox), so a change here is judged by what it
does for both. Open an issue before a large change or a new public API.

- Kit PRs run the kit's own checks: fmt, rustdoc with `-D warnings`, clippy with
  `-D warnings` and the tests for each feature set on Linux and macOS, and a
  compile check for the plugins' four targets (musl and darwin, x86_64 and
  aarch64). The kit itself ships source only; that check is not a release build.
- Kit PRs also run **both plugins' Rust test suites** against the PR's kit (the
  `plugin-suites` job runs `.github/scripts/plugin-suite.sh`: it moves the
  plugin's lock to this checkout and fails if the patch is not used). A change
  that breaks a plugin is visible before it is tagged. A plugin without a Cargo
  workspace, or not yet depending on the kit, is skipped with a notice.
- Keep the purity boundary: pure modules (`claude_session::fold`, the
  `inbox_contract` formats) do no IO, start no threads and read no environment.
- Fixtures are captured, never invented. See [fixtures/README.md](fixtures/README.md).
- Run locally before pushing (the feature-off sets are where cfg-related
  warnings show up):

  ```sh
  cargo fmt --all --check
  RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
  for f in "" fake tui remote sqlite; do
    cargo clippy --all-targets --no-default-features --features "$f" -- -D warnings
    cargo test --no-default-features --features "$f"
  done
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-features
  ```

The kit's tests use a fake herdr and fixtures. As a habit, unset every `HERDR_*` variable in the
shell you test from, so nothing can reach a live herdr session:

```sh
unset $(env | sed -n 's/^\(HERDR_[A-Za-z0-9_]*\)=.*/\1/p') 2>/dev/null || true
```

## Pull requests

- Keep a PR small and about one thing. Describe what changed and why; link the issue it fixes.
- A fix or a new behavior comes with a test.
- Commit messages: a short imperative summary line, then the reason in the body if it is not obvious.
- CI must be green. The maintainer reviews every PR and may ask for changes; please answer in the PR.
- Start as a draft if the work is not finished.

## Questions and bugs

Ask questions and float ideas in [Discussions](https://github.com/LeaveMyYard/herdr-agent-kit/discussions).
Bugs and concrete requests go in an [issue](https://github.com/LeaveMyYard/herdr-agent-kit/issues/new/choose).
Looking for a first contribution? Try the
[good first issue](https://github.com/LeaveMyYard/herdr-agent-kit/labels/good%20first%20issue) and
[help wanted](https://github.com/LeaveMyYard/herdr-agent-kit/labels/help%20wanted) labels. Report security
problems privately, see [SECURITY.md](SECURITY.md).
