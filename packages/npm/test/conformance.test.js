// Runs the upstream Express conformance suite through the wasm binding, so
// it cannot drift from the crate.
//
// Every compile, decompile, parse_response, parse_chunk (also fed one
// character at a time) and generate_prompt_snippet case runs as written
// (v1.0), except those conformance/bindings_skip.txt and out_of_scope.txt list; the Python
// binding's test (crates/a2ui-express-py/tests) runs the same cases. The
// renderer build runs the cases its calls cover.
//
// Needs `npm run build` (dist/) first.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import YAML from "yaml";

import { loadFlavours } from "./load.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const CONFORMANCE = join(ROOT, "vendor/a2ui/conformance");
const SUITES = ["compiler", "decompiler", "response_parser", "response_streaming", "prompt_generator"];
const DEFAULT_CATALOG = "test_data/catalogs/simplified_catalog_v1_0.json";

const readJson = (path) => JSON.parse(readFileSync(join(CONFORMANCE, path), "utf8"));

const CASES = new Map();
for (const suite of SUITES) {
  const text = readFileSync(join(CONFORMANCE, `agent/express/${suite}.yaml`), "utf8");
  for (const c of YAML.parse(text)) CASES.set(c.name, c);
}

const SKIPPED = ["bindings_skip", "out_of_scope"]
  .flatMap((name) => readFileSync(join(ROOT, `conformance/${name}.txt`), "utf8").split("\n"))
  .map((l) => l.split("#")[0].trim())
  .filter(Boolean);

// A catalog entry as upstream's harness reads it: a path, an inline document,
// or `{catalog, transformers}`; `protocolVersion` defaults to 1.0 and
// `catalogId` to the file's basename (or `inline`).
function resolveCatalog(entry) {
  let transformers = [];
  if (entry && typeof entry === "object" && "catalog" in entry) {
    transformers = entry.transformers ?? [];
    entry = entry.catalog;
  }
  let doc;
  let defaultId;
  if (typeof entry === "string") {
    doc = readJson(entry);
    defaultId = basename(entry, ".json");
  } else {
    doc = { ...entry };
    defaultId = "inline";
  }
  for (const spec of transformers) {
    for (const [key, kind] of [["component_pruning", "components"], ["function_pruning", "functions"]]) {
      if (!(key in spec)) continue;
      const keep = spec[key];
      if (Array.isArray(doc[kind])) doc[kind] = doc[kind].filter((f) => keep.includes(f.name));
      else if (doc[kind]) doc[kind] = Object.fromEntries(Object.entries(doc[kind]).filter(([k]) => keep.includes(k)));
    }
  }
  doc.protocolVersion ??= "1.0";
  doc.catalogId ??= defaultId;
  return doc;
}

function catalogsOf(args = {}, { prompt = false } = {}) {
  if (args.catalogs) return args.catalogs.map(resolveCatalog);
  if (args.catalog) return [resolveCatalog(args.catalog)];
  return prompt ? [] : [resolveCatalog(DEFAULT_CATALOG)];
}

function checkError(error, expected = {}) {
  assert.ok(error instanceof Error, `not an Error: ${error}`);
  if (expected.category) {
    assert.equal(error.category, expected.category, `${error.category}: ${error.message}`);
    assert.equal(error.name, expected.category);
  }
  assert.ok(Array.isArray(error.partial));
  if (expected.message) assert.match(error.message, new RegExp(expected.message));
}

function expectThrow(fn, expected) {
  let caught;
  try {
    fn();
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, `expected a ${expected.category ?? "failure"}`);
  checkError(caught, expected);
  return caught;
}

// Error parts carry a message the suites do not state: drop it.
const comparable = (parts) => parts.map((p) => ("error" in p ? { error: { category: p.error.category } } : p));

function mergeText(parts) {
  const out = [];
  for (const p of parts) {
    const last = out[out.length - 1];
    if ("text" in p && last && "text" in last) out[out.length - 1] = { text: last.text + p.text };
    else out.push(p);
  }
  return out;
}

function fragments(text, c) {
  for (const f of c.expect_contains ?? []) assert.ok(text.includes(f), `missing ${JSON.stringify(f)}\n${text}`);
  for (const f of c.expect_absent ?? []) assert.ok(!text.includes(f), `unexpected ${JSON.stringify(f)}\n${text}`);
}

const RUNNERS = {
  compile(ax, c) {
    const catalogs = catalogsOf(c.args);
    if (c.expect_error) return expectThrow(() => ax.compile(catalogs, c.input), c.expect_error);
    assert.deepEqual(ax.compile(catalogs, c.input), c.expect);
  },

  decompile(ax, c) {
    const catalogs = catalogsOf(c.args);
    if (c.expect_error) return expectThrow(() => ax.decompile(catalogs, c.messages), c.expect_error);
    const notation = ax.decompile(catalogs, c.messages);
    fragments(notation, c);
    if (c.expect_round_trip) assert.deepEqual(ax.compile(catalogs, notation), c.messages);
    if (c.expect_recompiled) assert.deepEqual(ax.compile(catalogs, notation), c.expect_recompiled);
  },

  parse_response(ax, c) {
    const catalogs = catalogsOf(c.args);
    const opts = { wrapped: c.args?.wrapped ?? true };
    if (c.expect_error) return expectThrow(() => ax.parseResponse(catalogs, c.input, opts), c.expect_error);
    assert.deepEqual(comparable(ax.parseResponse(catalogs, c.input, opts)), c.expect);
  },

  parse_chunk(ax, c) {
    const args = c.args ?? {};
    const open = () =>
      new ax.StreamCompiler(catalogsOf(args), {
        wrapped: args.wrapped ?? true,
        progressiveKeys: args.progressive_keys,
        afterRejectedBlock: args.after_rejected_block ?? "end",
      });

    // The steps as written.
    let stream = open();
    let failed = false;
    for (const [index, step] of c.steps.entries()) {
      if (step.expect_error) {
        const error = expectThrow(() => stream.push(step.input), step.expect_error);
        if (step.expect_partial_results) assert.deepEqual(comparable(error.partial), step.expect_partial_results);
        failed = true;
        break;
      }
      const parts = stream.push(step.input);
      assert.deepEqual(comparable(parts), step.expect ?? [], `step ${index} (${JSON.stringify(step.input)})`);
      if ("expect_in_block" in step) assert.equal(stream.inBlock, step.expect_in_block, `step ${index}: inBlock`);
    }
    if (!failed) {
      if (c.expect_error) expectThrow(() => stream.finish(), c.expect_error);
      else {
        const closed = stream.finish();
        if (c.expect_close) assert.deepEqual(comparable(closed), c.expect_close);
      }
    }
    stream.free();

    // The same response one character at a time: the same text, and the
    // same failure (or none).
    const text = c.steps.map((s) => s.input).join("");
    const expectedError = c.steps.find((s) => s.expect_error)?.expect_error ?? c.expect_error;
    stream = open();
    const parts = [];
    try {
      for (const ch of text) parts.push(...stream.push(ch));
      parts.push(...stream.finish());
    } catch (error) {
      assert.ok(expectedError, `char by char: unexpected ${error.category}: ${error.message}`);
      checkError(error, { category: expectedError.category });
      return;
    } finally {
      stream.free();
    }
    assert.ok(!expectedError, "char by char: no error raised");
    if (c.expect_matches_single_shot) {
      const single = ax.parseResponse(catalogsOf(args), text, { wrapped: args.wrapped ?? true });
      const texts = (ps) => mergeText(ps).filter((p) => "text" in p);
      assert.deepEqual(texts(parts), texts(single));
    }
  },

  generate_prompt_snippet(ax, c) {
    const args = c.args ?? {};
    const catalogs = catalogsOf(args, { prompt: true });
    const examples = (args.examples ?? []).map((p) => {
      const doc = readJson(p);
      return Array.isArray(doc) ? doc : [doc];
    });
    const call = () => ax.generatePrompt(catalogs, { examples, allowedMessages: args.allowed_messages });
    if (c.expect_error) return expectThrow(call, c.expect_error);
    const snippet = call();
    if (c.expect_file) assert.equal(snippet, readFileSync(join(CONFORMANCE, c.expect_file), "utf8"));
    fragments(snippet, c);
    if (c.expect_callable_functions) assert.deepEqual(ax.agentCallableFunctions(catalogs), c.expect_callable_functions);
    if (c.expect_deterministic) assert.equal(call(), snippet);
  },
};

// What the renderer build leaves out.
const RENDERER_ACTIONS = new Set(["compile", "parse_response", "parse_chunk"]);

const flavours = await loadFlavours();

test("the skip list names existing cases", () => {
  assert.deepEqual(SKIPPED.filter((n) => !CASES.has(n)), []);
});

for (const [flavour, ax] of Object.entries(flavours)) {
  const names = [...CASES.keys()].filter((n) => {
    const action = CASES.get(n).action;
    return action in RUNNERS && !SKIPPED.includes(n) && (flavour === "full" || RENDERER_ACTIONS.has(action));
  });
  test(`${flavour}: runs enough cases`, () => assert.ok(names.length > (flavour === "full" ? 200 : 150)));
  for (const name of names) {
    test(`${flavour}: ${name}`, () => RUNNERS[CASES.get(name).action](ax, CASES.get(name)));
  }
}
