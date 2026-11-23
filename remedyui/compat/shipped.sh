#!/usr/bin/env bash
# Builds compat/shipped_vault.rs against a release tag's own vault code and runs it.
#
#   compat/shipped.sh <tag> make|dump|dump-recovery <vault.json> <password|key>
#
# The tag's src-tauri/src/{vault,crypto,error}.rs are taken with `git show`, with the crate
# versions that release locked, into a build folder under the system temp directory.
set -euo pipefail
tag=$1; shift
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
build="${TMPDIR:-${TEMP:-/tmp}}/secretfolder-shipped-$tag"
mkdir -p "$build/src"
for f in vault crypto error; do
  git -C "$repo" show "$tag:src-tauri/src/$f.rs" > "$build/src/$f.rs"
done
cp "$here/shipped_vault.rs" "$build/src/main.rs"
# The release's own lock file pins the same crate versions it shipped with.
git -C "$repo" show "$tag:src-tauri/Cargo.lock" > "$build/Cargo.lock"
cat > "$build/Cargo.toml" <<EOF
[package]
name = "secretfolder-shipped-vault"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
base64 = "0.22"
zeroize = { version = "1", features = ["derive"] }
rand = "0.8"
argon2 = "0.5"
chacha20poly1305 = "0.10"
hex = "0.4"
mime_guess = "2"

[profile.dev.package.argon2]
opt-level = 3
[profile.dev.package.blake2]
opt-level = 3
EOF
cargo run -q --manifest-path "$build/Cargo.toml" -- "$@"
