#!/usr/bin/env bash
# Build the crate as `cargo package` makes it, with every feature: proves the
# package holds every file the code embeds (`include_str!`). `cargo package`'s
# own verify step builds default features only.
set -euo pipefail
cargo package --locked --no-verify
name=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')
version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
target_dir=$(cargo metadata --no-deps --format-version 1 | jq -r '.target_directory')
crate="$target_dir/package/$name-$version.crate"
# Inside the target dir (in the repo), so the pinned rust-toolchain.toml applies.
unpacked=$(mktemp -d "$target_dir/package-check.XXXXXX")
trap 'rm -rf "$unpacked"' EXIT
tar -xzf "$crate" -C "$unpacked"
# The kit's Cargo.lock pins the dependency versions CI tests with.
cp Cargo.lock "$unpacked/$name-$version/"
(cd "$unpacked/$name-$version" && cargo build --locked --all-features --target-dir "$target_dir/package-check")
echo "packaged $name $version builds with --all-features"
