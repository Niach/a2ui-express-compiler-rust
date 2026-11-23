# a2ui-express-compiler-rust

Compiles [A2UI Express](https://a2ui.org/concepts/inference-formats/) into
A2UI v1.0 (and v0.9) JSON messages. A Rust port of the Express format from
Google's A2UI agent SDK, built to embed in eui-core (iOS, Android, desktop)
and to ship as wasm for the web.

Crate: [`a2ui-express`](crates/a2ui-express). Runtime dependencies are limited
to `serde`, `serde_json` (`preserve_order`) and `thiserror`.

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

## Prompt generator

`a2ui_express::prompt::generate_prompt` writes the system-prompt section
that teaches a model Express for the active catalogs. `PromptOptions::style`
picks the format:

- `PromptStyle::Upstream` (the default) is upstream's format byte for byte;
  the conformance suite (`prompt_generator.yaml`) checks it.
- `PromptStyle::Compact` is opt-in, for large catalogs. Each enum or map
  shape several properties share (a style map, an icon enum) is listed once
  under *Shared Types* and referred to by name (`- icon: Icon`); a property
  line several components share is listed once under *Common Properties*;
  bindable properties are marked `~`, numbers `:n`, booleans `:b`; component
  descriptions are cut to their first sentence and property and function
  descriptions are left out. The rules are upstream's, less the two the
  notation makes moot (`(static)`, the `action` argument), plus a sentence
  on what a check reads and a rule on binding data and templating repeated
  content, generated from the catalog (#AXC-21).

```rust
use a2ui_express::prompt::{generate_prompt, PromptOptions, PromptStyle};

let options = PromptOptions { style: PromptStyle::Compact, ..Default::default() };
let prompt = generate_prompt(&catalogs, &options)?;
```

On eui's core catalog (`vendor/eui/catalog/core.schema.json`, 104
components) the upstream format is 457,928 characters (~114k tokens at
chars / 4); the compact one is about 39.5k characters (~9.9k tokens, inside
eui's 10k budget). [`eval/prompt/`](eval/prompt/) compares it with eui's
hand-written `catalogPrompt` on sample UIs.

```sh
cargo run -q -p a2ui-express --example express -- prompt vendor/eui/catalog/core.schema.json --compact
```

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
```

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
