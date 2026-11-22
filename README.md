# a2ui-express-compiler-rust

Compiles [A2UI Express](https://a2ui.org/concepts/inference-formats/) into
A2UI v1.0 (and v0.9) JSON messages. A Rust port of the Express format from
Google's A2UI agent SDK, built to embed in eui-core (iOS, Android, desktop)
and to ship as wasm for the web.

Crate: [`a2ui-express`](crates/a2ui-express). Runtime dependencies are limited
to `serde`, `serde_json` (`preserve_order`) and `thiserror`.

## Conformance suite

The test oracle is the upstream, language-neutral conformance suite, vendored
unmodified under `vendor/a2ui/` with upstream paths preserved:

| Path | What |
|---|---|
| `conformance/agent/express/*.yaml` | Express cases: compiler, decompiler, response parser, streaming, prompt generator, scaling |
| `conformance/test_data/{catalogs,examples}/` | Catalogs and examples the cases refer to |
| `specification/v0_9/json/{common_types,server_to_client}.json` | v0.9 schemas |
| `specification/v1_0/json/{common_types,agent_to_renderer}.json` | v1.0 schemas (`agent_to_renderer` is v1.0's server-to-client schema) |
| `catalogs/basic/v1/catalog.json` | v1.0 basic catalog |
| `specification/inference_formats/express/Express.g4` | Express grammar |

It is pinned to the head of
[a2ui-project/a2ui#4184](https://github.com/a2ui-project/a2ui/pull/4184)
(`$defs`/union and nested-array bindings), which is not merged yet. Re-pin
once it lands on `main`.

`vendor/eui/` holds eui's catalog (`core.schema.json`) and the `dashboard`,
`demo-surface` and `kitchen-sink` fixtures from `@exponential_app/eui`.

Both directories are generated; don't edit them by hand:

```sh
scripts/sync-upstream.sh <a2ui-commit-sha>   # -> vendor/a2ui/, pin in vendor/a2ui/UPSTREAM
scripts/sync-eui.sh <eui-version>            # -> vendor/eui/,  pin in vendor/eui/UPSTREAM
```

CI re-runs both at the recorded pins and fails if `vendor/` changes.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p a2ui-express --target wasm32-unknown-unknown
```

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
