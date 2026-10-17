#!/usr/bin/env bash
# Run one plugin's Rust test suite against this checkout of the kit.
# Run from the plugin's checkout, with the kit at ../kit and $PATCH set to the
# cargo `patch` config that points the kit's git URL at ../kit.
#
# A skip is always a visible notice, never a quiet pass: the job fails unless
# the patched kit is what the plugin actually resolved.
set -euo pipefail
plugin=$1

if [ ! -f Cargo.toml ]; then
  echo "::notice::$plugin has no Cargo workspace yet; skipped"
  exit 0
fi
# The plugin's own pinned toolchain (rust-toolchain.toml), before any cargo call.
rustup toolchain install

# Ask cargo, not grep: a comment that mentions the kit is not a dependency.
# `--no-deps` lists the workspace members with their declared dependencies
# (normal, dev and build) without resolving or fetching anything.
deps=$(cargo metadata --no-deps --format-version 1 \
  | jq '[.packages[].dependencies[] | select(.name == "herdr-agent-kit")] | length')
if [ "$deps" -eq 0 ]; then
  echo "::notice::$plugin does not depend on herdr-agent-kit yet; skipped"
  exit 0
fi

# Move the lock to the patched kit. Without this, a patched kit whose version
# no longer fits the plugin's Cargo.lock (any kit version bump) is ignored with
# only a warning, and the plugin is tested against its pinned tag instead.
cargo update -p herdr-agent-kit --config "$PATCH"

# The resolved kit must be the path checkout (path packages have no source).
resolved=$(cargo metadata --format-version 1 --locked --config "$PATCH" \
  | jq -r '[.packages[] | select(.name == "herdr-agent-kit") | (.source // "path")] | unique | join(",")')
if [ "$resolved" != "path" ]; then
  echo "::error::$plugin resolved herdr-agent-kit from '$resolved', not this kit checkout"
  exit 1
fi

log=$(mktemp)
cargo test --locked --workspace --all-features --config "$PATCH" 2>&1 | tee "$log"
if grep -q 'was not used in the crate graph' "$log"; then
  echo "::error::the kit patch was not used for $plugin"
  exit 1
fi
