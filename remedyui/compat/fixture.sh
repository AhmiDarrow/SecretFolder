#!/usr/bin/env bash
# Makes a test vault with a release tag's own vault code, for
# `shipped_vaults_open_edit_and_reopen_without_loss`:
#
#   compat/fixture.sh <tag> <password>
#
# writes tests/fixtures/vault-<tag>/ (the vault), <tag>.recovery (its recovery key) and
# vault-<tag>.expected.json (what the tag's code reads back from it). The vault is made in a
# fresh temporary folder; nothing outside this repository and the temp folder is touched.
set -euo pipefail
tag=$1
pw=$2
here="$(cd "$(dirname "$0")" && pwd)"
fixtures="$here/../tests/fixtures"
if [ -e "$fixtures/vault-$tag" ]; then
  echo "fixture.sh: $fixtures/vault-$tag already exists" >&2
  exit 1
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
"$here/shipped.sh" "$tag" make "$work/vault" "$pw" | tr -d '\r\n' > "$work/recovery"
"$here/shipped.sh" "$tag" dump "$work/vault" "$pw" > "$work/expected.json"
# The recovery key opens it to the same contents, by the tag's own code.
"$here/shipped.sh" "$tag" dump-recovery "$work/vault" "$(cat "$work/recovery")" > "$work/by-recovery.json"
cmp -s "$work/expected.json" "$work/by-recovery.json" || { echo "fixture.sh: recovery key reads differently" >&2; exit 1; }
cp -R "$work/vault" "$fixtures/vault-$tag"
cp "$work/recovery" "$fixtures/$tag.recovery"
cp "$work/expected.json" "$fixtures/vault-$tag.expected.json"
echo "fixture.sh: made vault-$tag"
