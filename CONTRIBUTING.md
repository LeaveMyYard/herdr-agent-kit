# Contributing

- Kit PRs run the kit's own checks (fmt, clippy with `-D warnings`, tests for
  each feature set on Linux and macOS, and a build for the four release targets)
  **and both plugins' Rust test suites** against the PR's kit (the
  `plugin-suites` job patches the kit dependency to the checkout). A change that
  breaks a plugin is visible before it is tagged.
- Keep the purity boundary: pure modules (`claude_session::fold`, the
  `inbox_contract` formats) do no IO, start no threads and read no environment.
- Fixtures are captured, never invented. See [fixtures/README.md](fixtures/README.md).
- Run locally before pushing:

  ```sh
  cargo fmt --all --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-features
  ```
