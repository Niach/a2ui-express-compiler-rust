# a2ui-express-compiler-rust

Compiles [A2UI Express](https://a2ui.org/concepts/inference-formats/) into
A2UI v1.0 (and v0.9) JSON messages. A Rust port of the Express format from
Google's A2UI agent SDK, built to embed in eui-core (iOS, Android, desktop)
and to ship as wasm for the web.

Crate: [`a2ui-express`](crates/a2ui-express). Runtime dependencies are limited
to `serde`, `serde_json` (`preserve_order`) and `thiserror`. The same compiler
ships to JavaScript (wasm, `@niach/a2ui-express`) and Python (`a2ui-express`);
see [Bindings](#bindings).

## Streaming

`response::ExpressParser` reads model output: prose with Express blocks
between `<a2ui>` tags (or a bare Express body).

```rust
let parser = ExpressParser::new(compiler, "main");
let mut stream = parser.open_stream();
for chunk in model_output {
    for part in stream.push(chunk)? {   // Part::Text | Part::A2ui(messages) | Part::Error
        render(part);
    }
}
render_all(stream.finish()?);
```

Each statement compiles as soon as it is complete: a new surface's
`createSurface` once `root` is there, then `updateComponents` /
`updateDataModel` deltas. A component may name a child that comes later (it
renders once the child arrives), and an open string of a `text` property heals,
so a sentence types in live. Every message is checked against a
`state::RendererState` mirror of what the renderer holds before it is
returned. Streamed in any chunk size, a response leaves the renderer with the
same surfaces as `parse_response` on the whole text
([`tests/streaming.rs`](crates/a2ui-express/tests/streaming.rs) checks every
conformance input one character at a time). A step costs about what its
statement costs: eui's kitchen-sink surface (251 components, 30 KB) streams
character by character in ~0.2 s in release builds
(`cargo run --release -p a2ui-express --example stream_bench`).

`response::{unwrap, wrap, has_format_content}` split a whole response into
text and raw blocks and back.

Samples meant for streaming should be written top-down (`root` first):
`decompile` with `Order::TopDown`.

## Bindings

One implementation everywhere: both bindings are thin shells over
[`a2ui-express-ffi`](crates/a2ui-express-ffi), a JSON-in/JSON-out layer, so
they take the same options and return the same shapes:

- a parsed part: `{text}` | `{a2ui: [messages]}` | `{error: {category, message}}`;
- a raw part (`unwrap`/`wrap`): `{text}` | `{a2ui_raw, is_final?}`;
- an error: category `ParseError` | `ValidationError` | `CatalogError` (plus
  `ArgumentError` for a malformed call), message, and `partial`, the parts a
  response read made ready before it failed.

Both run every compile, decompile, parse_response, streaming (as written and
one character at a time) and prompt conformance case in CI
([`conformance/bindings_skip.txt`](conformance/bindings_skip.txt) lists the
four that check harness-only behaviour), so neither can drift from the crate.

### JavaScript / wasm: [`packages/npm`](packages/npm)

```js
import init, { compile, StreamCompiler, parseResponse, decompile, generatePrompt } from "@niach/a2ui-express";
await init(); // or initSync({ module: wasmBytes })

const stream = new StreamCompiler(catalog, { surfaceId: "main" });
for await (const chunk of modelOutput) {
  for (const part of stream.push(chunk)) render(part); // throws an Error with .category/.partial
}
stream.finish().forEach(render);
```

`compile(catalogs, source, {surfaceId?, version?})`,
`decompile(catalogs, messages, {keywordArgs?, order?: "upstream" | "top-down"})`,
`parseResponse(catalogs, text, {surfaceId?, wrapped?, progressiveKeys?})`,
`new StreamCompiler(catalogs, {surfaceId?, wrapped?, progressiveKeys?, afterRejectedBlock?: "end" | "continue"})`
with `push`/`finish`/`inBlock`, `unwrap`/`wrap`/`hasFormatContent`,
`generatePrompt(catalogs, {examples?, allowedMessages?})`,
`agentCallableFunctions(catalogs)`. TypeScript types included.

Each free function loads its catalogs anew. For a large catalog load them
once into a `Catalogs` and compile against that: eui's catalog takes ~0.1–0.2 s
to load in wasm, a 30 KB block ~20 ms to compile against it, and opening a
stream on it nothing.

```js
const catalogs = new Catalogs(euiCatalog, { surfaceId: "main" });
const stream = catalogs.stream();       // also catalogs.compile / parseResponse / decompile / generatePrompt
```

The crate is [`crates/a2ui-express-wasm`](crates/a2ui-express-wasm) and
builds with plain `wasm-pack build --target web` (or `cargo build --target
wasm32-unknown-unknown --release` + `wasm-bindgen --target web` +
`wasm-opt -Oz`). `scripts/build-npm.sh` builds the package's two flavours:

| build | entry | calls | wasm, gzip -9 | brotli |
|---|---|---|---|---|
| full (default features) | `@niach/a2ui-express` | all | 228 KB | 187 KB |
| renderer (`--no-default-features`) | `@niach/a2ui-express/renderer` | compile, stream, parse, unwrap/wrap | 180 KB | 149 KB |

The web renderers load the renderer build; CI fails it at 200 KB gzipped
(the decompiler and prompt generator are agent-side and add ~48 KB). eui's
catalog is a runtime input, not bundled (741 KB JSON, 62 KB gzipped).
wasm32 builds are optimised for size (`.cargo/config.toml`: `opt-level=z`,
fat LTO, wasm-opt `-Oz`), which makes them about 2.5× slower than native
`-O3`: eui's kitchen-sink surface (251 components) streamed one character at
a time, the worst case, takes ~0.6 s. Publishing is a manual workflow
([`.github/workflows/publish-npm.yml`](.github/workflows/publish-npm.yml)).

### Python: [`crates/a2ui-express-py`](crates/a2ui-express-py)

A pyo3 + maturin abi3 wheel (CPython ≥ 3.9), `import a2ui_express`:
`compile`, `decompile(order="top-down")`, `parse_response`, `StreamCompiler`,
`unwrap`/`wrap`/`has_format_content`, `generate_prompt`,
`agent_callable_functions`, and `Catalogs` (load once, call its methods in a
loop), raising `ParseError` / `ValidationError` /
`CatalogError` (subclasses of `ExpressError`, with `.category` and
`.partial`). See its [README](crates/a2ui-express-py/README.md).

```sh
cd crates/a2ui-express-py && maturin develop --release && pytest tests
```

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

`crates/a2ui-express/tests/eui/` holds the Express form of the eui fixtures
and the `createSurface` each must compile to (`tests/eui.rs`). Both are
generated from the pinned eui package, whose `flattenSurface` gives the flat
components: `scripts/gen-eui-express.mjs <unpacked-eui-package>`.

The lexer's identifier tables (`crates/a2ui-express/src/syntax/xid.rs`) are
generated too, from upstream's ANTLR lexer at the same pin, so identifiers
lex exactly as upstream lexes them: `scripts/gen-xid-tables.py <a2ui-checkout>`.

## Running the suite

`cargo test` runs every Express case through
[`tests/conformance.rs`](crates/a2ui-express/tests/conformance.rs), as written
(`v1.0`) and again lowered to v0.9 (`v0.9-lowered`, mirroring upstream's Python
and TypeScript harnesses), and prints the passing count per suite. The number
of passing cases is the project's progress metric.

The crate is reached only through
[`tests/conformance/sdk.rs`](crates/a2ui-express/tests/conformance/sdk.rs):
one JSON-in, JSON-out function per suite call, each `unimplemented` until it is
wired to the crate.

[`conformance/known_failures.txt`](conformance/known_failures.txt) lists the
cases expected to fail (`<case>`, or `<case>::<version>`). The test fails when
an unlisted case fails, a listed case passes, or an entry names no case, so the
list only shrinks. After fixing cases, regenerate it:

```sh
cargo test -p a2ui-express --test conformance -- <name-substring>   # a subset
CONFORMANCE_VERBOSE=1 cargo test -p a2ui-express --test conformance # every failure's reason
CONFORMANCE_BLESS=1 cargo test -p a2ui-express --test conformance   # rewrite known_failures.txt
```

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p a2ui-express --target wasm32-unknown-unknown
scripts/build-npm.sh && (cd packages/npm && npm ci && npm test)   # wasm binding
(cd crates/a2ui-express-py && maturin develop --release && pytest tests)   # Python binding
```

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
