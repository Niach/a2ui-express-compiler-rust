#!/usr/bin/env bash
# Vendor the A2UI Express conformance suite, schemas, catalog and grammar from
# a2ui-project/a2ui at a pinned commit into vendor/a2ui/.
#
#   scripts/sync-upstream.sh <commit-sha>
#
# Files keep their upstream paths, so the relative paths inside the YAML cases
# (e.g. "test_data/catalogs/...", relative to conformance/) resolve as-is.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <commit-sha>" >&2
  exit 2
fi
commit="$1"
if [[ ! "$commit" =~ ^[0-9a-f]{40}$ ]]; then
  echo "error: pass a full 40-character commit sha, got '$commit'" >&2
  exit 2
fi

repo_url="https://github.com/a2ui-project/a2ui.git"
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/vendor/a2ui"

paths=(
  LICENSE
  conformance/agent/express
  conformance/test_data/catalogs
  conformance/test_data/examples
  specification/v0_9/json/common_types.json
  specification/v0_9/json/server_to_client.json
  specification/v1_0/json/common_types.json
  specification/v1_0/json/agent_to_renderer.json
  catalogs/basic/v1/catalog.json
  specification/inference_formats/express/Express.g4
)

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git -C "$tmp" init -q
git -C "$tmp" remote add origin "$repo_url"
# GitHub serves any reachable commit by sha, including PR heads.
git -C "$tmp" fetch -q --depth 1 --filter=blob:none origin "$commit"
git -C "$tmp" sparse-checkout set --no-cone "${paths[@]}"
git -C "$tmp" -c advice.detachedHead=false checkout -q FETCH_HEAD

rm -rf "$dest"
mkdir -p "$dest"
for p in "${paths[@]}"; do
  if [[ ! -e "$tmp/$p" ]]; then
    echo "error: $p not found at $commit" >&2
    exit 1
  fi
  mkdir -p "$dest/$(dirname "$p")"
  cp -R "$tmp/$p" "$dest/$p"
done
# Express suite: only the YAML cases.
find "$dest/conformance/agent/express" -type f ! -name '*.yaml' -delete

# The crate embeds the common types, so it ships its own copy of them.
for v in v0_9 v1_0; do
  mkdir -p "$root/crates/a2ui-express/schemas/$v"
  cp "$dest/specification/$v/json/common_types.json" "$root/crates/a2ui-express/schemas/$v/common_types.json"
done

cat > "$dest/UPSTREAM" <<META
repository: https://github.com/a2ui-project/a2ui
commit: $commit
synced-by: scripts/sync-upstream.sh $commit
META

echo "vendored a2ui-project/a2ui@$commit into vendor/a2ui/"
