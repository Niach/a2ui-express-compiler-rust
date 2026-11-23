// The JS surface of the binding: shapes, options, errors, and what the
// renderer build leaves out.
import assert from "node:assert/strict";
import { test } from "node:test";

import { loadFlavours } from "./load.js";

const { full: ax, renderer } = await loadFlavours();

const CATALOG = {
  catalogId: "demo",
  protocolVersion: "1.0",
  components: {
    Text: {
      properties: { component: { const: "Text" }, text: { $ref: "common_types.json#/$defs/DynamicString" } },
      required: ["component", "text"],
    },
    Card: {
      properties: { component: { const: "Card" }, child: { $ref: "common_types.json#/$defs/ComponentId" } },
      required: ["component", "child"],
    },
  },
};

test("compile and decompile round trip, keys in emission order", () => {
  const messages = ax.compile(CATALOG, 'root = Card(body)\nbody = Text("Hi")', { surfaceId: "main" });
  const surface = messages[0].createSurface;
  assert.equal(surface.surfaceId, "main");
  assert.deepEqual(Object.keys(surface.components[0]), ["id", "component", "child"]);
  const text = ax.decompile([CATALOG], messages, { order: "top-down" });
  assert.ok(text.indexOf("root") < text.indexOf("body"));
  assert.deepEqual(ax.compile(CATALOG, text, { surfaceId: "main" }), messages);
  assert.equal(ax.compile(CATALOG, 'root = Text("x")')[0].createSurface.surfaceId, "default_surface");
});

test("errors are Errors with a category and the partial parts", () => {
  assert.throws(
    () => ax.parseResponse(CATALOG, "Before.\n<a2ui>root = Nope()</a2ui>"),
    (e) => e instanceof Error && e.name === "ValidationError" && e.category === "ValidationError" &&
      JSON.stringify(e.partial) === JSON.stringify([{ text: "Before." }]),
  );
  assert.throws(() => ax.compile(CATALOG, "root = Text("), { category: "ParseError" });
  assert.throws(() => ax.compile({}, "root = Text('x')"), { category: "CatalogError" });
  assert.throws(() => ax.decompile(CATALOG, [], { order: "sideways" }), { category: "ArgumentError" });
  assert.throws(() => new ax.StreamCompiler(CATALOG, { surfaceID: "x" }), { category: "ArgumentError" });
});

test("StreamCompiler streams parts", () => {
  const stream = new ax.StreamCompiler(CATALOG, { surfaceId: "main" });
  const parts = stream.push('Here.\n<a2ui>\nroot = Card(body)\nbody = Text("Hi');
  assert.equal(stream.inBlock, true);
  parts.push(...stream.push('")\n</a2ui>'), ...stream.finish());
  assert.deepEqual(parts[0], { text: "Here." });
  assert.ok("createSurface" in parts[1].a2ui[0]);
  assert.equal(stream.inBlock, false);
  stream.free();
});

test("afterRejectedBlock: continue yields an error part", () => {
  const stream = new ax.StreamCompiler(CATALOG, { afterRejectedBlock: "continue" });
  const parts = [...stream.push("<a2ui>root = Nope()</a2ui> then text"), ...stream.finish()];
  assert.equal(parts[0].error.category, "ValidationError");
  assert.equal(typeof parts[0].error.message, "string");
  assert.deepEqual(parts.at(-1), { text: "then text" });
});

test("unwrap, wrap, hasFormatContent, generatePrompt", () => {
  const raw = ax.unwrap('Hi\n<a2ui>\nroot = Text("x")\n</a2ui>');
  assert.deepEqual(raw, [{ text: "Hi" }, { a2ui_raw: 'root = Text("x")' }]);
  assert.deepEqual(ax.unwrap(ax.wrap(raw)), raw);
  assert.equal(ax.hasFormatContent(ax.wrap(raw), true), true);
  const prompt = ax.generatePrompt(CATALOG, { examples: [ax.compile(CATALOG, 'root = Text("x")')] });
  assert.ok(prompt.includes("Card("));
  assert.deepEqual(ax.agentCallableFunctions(CATALOG), []);
});

test("the renderer build has the renderer's calls only", () => {
  for (const name of ["compile", "parseResponse", "unwrap", "wrap", "hasFormatContent", "StreamCompiler"]) {
    assert.equal(typeof renderer[name], "function", name);
  }
  for (const name of ["decompile", "generatePrompt", "agentCallableFunctions"]) {
    assert.equal(renderer[name], undefined, name);
  }
});

test("Catalogs: load once, compile many times", () => {
  for (const build of [ax, renderer]) {
    const catalogs = new build.Catalogs([CATALOG]);
    assert.equal(catalogs.version, "1.0");
    assert.deepEqual(catalogs.ids, ["demo"]);
    const messages = catalogs.compile('root = Text("x")', { surfaceId: "main" });
    assert.deepEqual(messages, build.compile(CATALOG, 'root = Text("x")', { surfaceId: "main" }));
    for (let i = 0; i < 2; i++) {
      const stream = catalogs.stream({ surfaceId: "main" });
      assert.deepEqual([...stream.push('<a2ui>root = Text("x")</a2ui>'), ...stream.finish()], [{ a2ui: messages }]);
    }
    assert.deepEqual(catalogs.parseResponse('<a2ui>root = Text("x")</a2ui>', { surfaceId: "main" }), [{ a2ui: messages }]);
    assert.throws(() => catalogs.compile("root = Text('x')", { version: "0.9" }), { category: "ArgumentError" });
    assert.throws(() => new build.Catalogs([]), { category: "CatalogError" });
  }
  const catalogs = new ax.Catalogs(CATALOG);
  const messages = catalogs.compile('root = Text("x")');
  assert.deepEqual(catalogs.compile(catalogs.decompile(messages)), messages);
  assert.ok(catalogs.generatePrompt().includes("Text("));
  assert.deepEqual(catalogs.agentCallableFunctions(), []);
  assert.equal(new renderer.Catalogs(CATALOG).decompile, undefined);
});
