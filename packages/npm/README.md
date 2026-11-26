# @mrniach/a2ui-express

The [A2UI Express compiler](https://github.com/Niach/a2ui-express-compiler-rust)
for JavaScript: the Rust compiler as WebAssembly, so the web renders exactly
what eui-core renders natively and what the training pipeline (Python wheel)
validates. ES module, `--target web` glue; works in browsers, bundlers, Node,
Deno and Bun.

```js
import init, { Catalogs } from "@mrniach/a2ui-express/renderer"; // or "@mrniach/a2ui-express" for every call
await init(); // fetches a2ui_express_bg.wasm next to the module; or initSync({ module: bytes })

const catalogs = new Catalogs(catalogDocument, { surfaceId: "main" }); // load once
const stream = catalogs.stream();
for await (const chunk of modelOutput) {
  for (const part of stream.push(chunk)) {
    if ("text" in part) showText(part.text);
    else if ("a2ui" in part) renderer.apply(part.a2ui); // createSurface / updateComponents / updateDataModel …
  }
}
stream.finish().forEach(render);
stream.free();
```

Parts are `{text}`, `{a2ui: messages}` or, with `afterRejectedBlock:
"continue"`, `{error: {category, message}}`. Errors are thrown as `Error`s
named after their category (`ParseError`, `ValidationError`, `CatalogError`,
`ArgumentError`) with `.category` and `.partial` (the parts made ready before
the failure, to show as well).

| export | |
|---|---|
| `compile(catalogs, source, {surfaceId?, version?})` | one block to messages |
| `parseResponse(catalogs, text, {surfaceId?, wrapped?, progressiveKeys?, version?})` | a whole response to parts |
| `new StreamCompiler(catalogs, {surfaceId?, wrapped?, progressiveKeys?, afterRejectedBlock?, version?})` | `push(chunk)`, `finish()`, `inBlock` |
| `unwrap(text)`, `wrap(rawParts)`, `hasFormatContent(text, complete?)` | raw parts `{text}` / `{a2ui_raw, is_final?}` |
| `new Catalogs(catalogs, {version?, surfaceId?})` | the above as methods, catalogs loaded once (`stream(opts)` opens a `StreamCompiler`) |
| `decompile(catalogs, messages, {keywordArgs?, order?})` | full build only; `order: "top-down"` writes `root` first |
| `generatePrompt(catalogs, {examples?, allowedMessages?, style?: "upstream" | "compact"})`, `agentCallableFunctions(catalogs)` | full build only |

`catalogs` is a catalog document or a list of them.

## Builds

| entry | wasm gzip -9 | brotli |
|---|---|---|
| `@mrniach/a2ui-express/renderer` (compile, stream, parse) | 180 KB | 149 KB |
| `@mrniach/a2ui-express` (also decompile, generatePrompt) | 228 KB | 187 KB |

Built from `crates/a2ui-express-wasm` with `scripts/build-npm.sh`
(`wasm-pack build --target web`, wasm-opt `-Oz`); `npm test` runs the
upstream conformance cases through both builds.

Apache-2.0.
