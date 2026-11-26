// Replays the at_exponential_express fixtures (conformance/extensions/*.json)
// through the wasm binding, as crates/a2ui-express/tests/extensions.rs and the
// Python binding's test do: compile (also streamed one character at a time,
// and decompiled back to a fixed point), decompile, expandTheme, compilePatch
// (also as a property: applying the patch equals compiling the new screen).
// The renderer build runs the compile and stream halves.
//
// Needs `npm run build` (dist/) first.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { loadFlavours } from "./load.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const FIXTURES = join(ROOT, "conformance/extensions");
const NAMES = ["theme", "hex", "style", "motion", "patch"];
const readJson = (name) => JSON.parse(readFileSync(join(FIXTURES, name), "utf8"));

function catalog(path) {
  const doc = readJson(path);
  doc.metadata ??= {};
  doc.metadata.extensions ??= {};
  doc.metadata.extensions.at_exponential_express ??= true;
  return doc;
}

function setPointer(root, path, value) {
  if (path === "" || path === "/") return value;
  const tokens = path.replace(/^\//, "").split("/").map((t) => t.replaceAll("~1", "/").replaceAll("~0", "~"));
  let cur = root;
  for (const t of tokens.slice(0, -1)) {
    if (typeof cur[t] !== "object" || cur[t] === null || Array.isArray(cur[t])) cur[t] = {};
    cur = cur[t];
  }
  const last = tokens[tokens.length - 1];
  if (value === null) delete cur[last];
  else cur[last] = value;
  return root;
}

function model(messages, surfaces = {}) {
  const blank = () => ({ components: {}, data: {}, theme: null });
  for (const m of messages) {
    if (m.createSurface) {
      const b = m.createSurface;
      surfaces[b.surfaceId] = {
        components: Object.fromEntries((b.components ?? []).map((c) => [c.id, c])),
        data: b.dataModel ?? {},
        theme: b.metadata?.extensions?.at_exponential_theme ?? null,
      };
    } else if (m.updateComponents) {
      const s = (surfaces[m.updateComponents.surfaceId] ??= blank());
      for (const c of m.updateComponents.components) s.components[c.id] = c;
    } else if (m.updateDataModel) {
      const b = m.updateDataModel;
      const s = (surfaces[b.surfaceId] ??= blank());
      s.data = setPointer(s.data, b.path ?? "/", b.value);
    } else if (m.callRendererFunction?.callFunction?.["@call"] === "setTheme") {
      const b = m.callRendererFunction;
      const s = (surfaces[b.surfaceId ?? "default_surface"] ??= blank());
      s.theme = b.callFunction.args.theme;
    }
  }
  return surfaces;
}

function strings(v, out) {
  if (typeof v === "string") out.push(v);
  else if (Array.isArray(v)) v.forEach((x) => strings(x, out));
  else if (v && typeof v === "object") for (const [k, x] of Object.entries(v)) if (k !== "id") strings(x, out);
}

function drawn(surfaces) {
  const out = {};
  for (const [sid, s] of Object.entries(surfaces)) {
    const seen = new Set();
    const todo = ["root"];
    while (todo.length) {
      const c = todo.pop();
      if (seen.has(c) || !(c in s.components)) continue;
      seen.add(c);
      const found = [];
      strings(s.components[c], found);
      todo.push(...found);
    }
    out[sid] = [Object.fromEntries(Object.entries(s.components).filter(([k]) => seen.has(k))), s.data, s.theme];
  }
  return out;
}

function stream(mod, cat, source) {
  const s = new mod.StreamCompiler(cat, { surfaceId: "main" });
  const out = [];
  for (const ch of `<a2ui>\n${source}\n</a2ui>`) for (const p of s.push(ch)) out.push(...(p.a2ui ?? []));
  for (const p of s.finish()) out.push(...(p.a2ui ?? []));
  return out;
}

function throws(fn, want = {}) {
  let caught;
  try {
    fn();
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, "expected an error");
  if (want.message) assert.ok(caught.message.includes(want.message), caught.message);
}

const flavours = await loadFlavours();

for (const [flavour, mod] of Object.entries(flavours)) {
  const full = flavour === "full";
  for (const name of NAMES) {
    const doc = readJson(`${name}.json`);
    const cat = catalog(doc.catalog ?? "catalog.json");
    for (const c of doc.cases) {
      if (!full && c.action !== "compile") continue;
      test(`${flavour}: ${name}/${c.name}`, () => {
        if (c.action === "compile") {
          if (c.expect_error) return throws(() => mod.compile(cat, c.input, { surfaceId: "main" }), c.expect_error);
          const got = mod.compile(cat, c.input, { surfaceId: "main" });
          assert.deepEqual(got, c.expect);
          if (full) {
            const text = mod.decompile(cat, got, { order: "top-down" });
            assert.equal(text, c.decompiled);
            assert.deepEqual(mod.compile(cat, text, { surfaceId: "main" }), got);
          }
          if (c.stream !== false) assert.deepEqual(drawn(model(stream(mod, cat, c.input))), drawn(model(got)));
        } else if (c.action === "decompile") {
          assert.equal(mod.decompile(cat, c.input, { order: "top-down" }), c.expect);
        } else if (c.action === "expand_theme") {
          if (c.expect_error) return throws(() => mod.expandTheme(c.input), c.expect_error);
          assert.deepEqual(mod.expandTheme(c.input), c.expect);
        } else if (c.action === "patch") {
          if (c.expect_error) return throws(() => mod.compilePatch(cat, c.screen, c.patch, { surfaceId: "main" }), c.expect_error);
          const p = mod.compilePatch(cat, c.screen, c.patch, { surfaceId: "main" });
          assert.deepEqual(p.messages, c.expect);
          assert.equal(p.screen, c.screen_after);
          const before = model(mod.compile(cat, c.screen, { surfaceId: "main" }));
          assert.deepEqual(drawn(model(p.messages, before)), drawn(model(mod.compile(cat, p.screen, { surfaceId: "main" }))));
        } else {
          assert.fail(`unknown action ${c.action}`);
        }
      });
    }
  }
}
