#!/usr/bin/env bash
# Builds the npm package (packages/npm/dist) from crates/a2ui-express-wasm:
#
#   dist/full/      every call (compile, stream, parse, decompile, generatePrompt)
#   dist/renderer/  what a renderer needs (compile, stream, parse, unwrap/wrap):
#                   no decompiler or prompt generator, ~50 KB smaller gzipped
#
# Each is plain `wasm-pack build --target web` output (wasm-opt -Oz included),
# and prints its size raw / gzip -9 / brotli. Needs wasm-pack and the
# wasm32-unknown-unknown target.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
crate="$root/crates/a2ui-express-wasm"
dist="$root/packages/npm/dist"

# A RUSTFLAGS variable would replace the size flags in .cargo/config.toml.
unset RUSTFLAGS

build() { # <out dir> [cargo args...]
  local out="$1"
  shift
  rm -rf "$out"
  wasm-pack build "$crate" --target web --release --no-pack \
    --out-dir "$out" --out-name a2ui_express "$@"
  rm -f "$out/.gitignore"
}

build "$dist/full"
build "$dist/renderer" -- --no-default-features

cp "$root/LICENSE" "$root/NOTICE" "$root/packages/npm/"

for flavour in full renderer; do
  wasm="$dist/$flavour/a2ui_express_bg.wasm"
  raw=$(wc -c <"$wasm")
  gz=$(gzip -9c "$wasm" | wc -c)
  br=$( (command -v brotli >/dev/null && brotli -c -q 11 "$wasm" | wc -c) || echo "n/a")
  printf '%-9s wasm: %7d bytes raw, %7d gzip -9, %7s brotli\n' "$flavour" "$raw" "$gz" "$br"
done
