#!/usr/bin/env bash
# Vendor eui's catalog schema and sample surfaces from the npm package
# @exponential_app/eui (Apache-2.0) at a pinned version into vendor/eui/.
#
#   scripts/sync-eui.sh <version>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 2
fi
version="$1"
pkg="@exponential_app/eui"
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/vendor/eui"

files=(
  LICENSE
  NOTICE
  catalog/core.schema.json
  fixtures/dashboard.json
  fixtures/dashboard.data.json
  fixtures/demo-surface.json
  fixtures/kitchen-sink.json
)

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# npm pack checks the tarball against the registry's integrity hash.
tarball="$(cd "$tmp" && npm pack --silent "$pkg@$version")"
integrity="$(npm view "$pkg@$version" dist.integrity)"
tar -xzf "$tmp/$tarball" -C "$tmp"

rm -rf "$dest"
mkdir -p "$dest"
for f in "${files[@]}"; do
  if [[ ! -e "$tmp/package/$f" ]]; then
    echo "error: $f not found in $pkg@$version" >&2
    exit 1
  fi
  mkdir -p "$dest/$(dirname "$f")"
  cp "$tmp/package/$f" "$dest/$f"
done

cat > "$dest/UPSTREAM" <<META
package: $pkg
version: $version
integrity: $integrity
synced-by: scripts/sync-eui.sh $version
META

echo "vendored $pkg@$version into vendor/eui/"
