# a2ui-express (Python)

Python bindings for the [A2UI Express compiler](https://github.com/Niach/a2ui-express-compiler-rust):
the same Rust compiler that ships to web clients (as the `@mrniach/a2ui-express`
npm package), so training data validated and converted here behaves exactly
like it will on a client.

```python
import a2ui_express as ax

catalog = {...}  # an A2UI catalog document (or a list of them)

messages = ax.compile(catalog, 'root = Text("Hi")')            # Express -> A2UI messages
express = ax.decompile(catalog, messages, order="top-down")   # A2UI messages -> Express
parts = ax.parse_response(catalog, 'Here.\n<a2ui>root = Text("Hi")</a2ui>')
# [{"text": "Here."}, {"a2ui": [...]}]

stream = ax.StreamCompiler(catalog, surface_id="main")
for chunk in model_output:
    for part in stream.push(chunk):   # {"text"} | {"a2ui": [...]} | {"error": {...}}
        render(part)
render_all(stream.finish())

prompt = ax.generate_prompt(catalog, examples=[messages])     # the system-prompt section

# In a loop, load the catalogs once: every free function loads them anew.
catalogs = ax.Catalogs(catalog)
for sample in dataset:
    messages = catalogs.compile(sample)   # also .decompile, .parse_response, .stream, .generate_prompt
```

| call | |
|---|---|
| `compile(catalogs, source, *, surface_id=None, version=None)` | one block (tags optional) to messages |
| `decompile(catalogs, messages, *, keyword_args=False, order="upstream")` | `order="top-down"` writes `root` first, for streaming |
| `parse_response(catalogs, text, *, surface_id=None, wrapped=True, progressive_keys=None, version=None)` | a whole response |
| `StreamCompiler(catalogs, *, surface_id=None, wrapped=True, progressive_keys=None, after_rejected_block="end", version=None)` | `.push(chunk)`, `.finish()`, `.in_block` |
| `unwrap(text)`, `wrap(parts)`, `has_format_content(text, complete=False)` | raw parts `{"text"}` / `{"a2ui_raw", "is_final"?}` |
| `generate_prompt(catalogs, *, surface_id=None, examples=None, allowed_messages=None, style=None)` (`style="compact"` for large catalogs), `agent_callable_functions(catalogs)` | prompt generator |
| `Catalogs(catalogs, *, version=None, surface_id=None)` | loaded once; `.compile`, `.decompile`, `.parse_response`, `.stream`, `.generate_prompt`, `.agent_callable_functions`, `.version`, `.ids`; thread-safe |

Errors are `ParseError`, `ValidationError` and `CatalogError`, subclasses of
`ExpressError`, each with `.category` (the conformance suite's name) and
`.partial` (the parts a response read made ready before it failed). A bad
argument (an unknown `order`, a non-list) is a `ValueError`. Values are plain
`dict`/`list`/`str`/numbers, with dict keys in emission order. A
`StreamCompiler` belongs to the thread that opened it.

## Building

```sh
pip install maturin
cd crates/a2ui-express-py
maturin develop --release          # into the active virtualenv
maturin build --release            # an abi3 wheel (CPython >= 3.9) under target/wheels
pip install pytest pyyaml && pytest tests
```
