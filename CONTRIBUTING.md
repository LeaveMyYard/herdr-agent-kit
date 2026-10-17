# Contributing

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
