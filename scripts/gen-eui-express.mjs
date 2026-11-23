#!/usr/bin/env node
// Writes the Express form of eui's sample surfaces, and the messages they
// must compile to, into crates/a2ui-express/tests/eui/.
//
//   scripts/gen-eui-express.mjs <extracted @exponential_app/eui package dir>
//
// The package must be the version pinned in vendor/eui/UPSTREAM (unpack it
// with `npm pack @exponential_app/eui@<version> && tar -xzf *.tgz`). Its own
// `flattenSurface` turns each nested fixture in vendor/eui/fixtures/ into the
// flat A2UI component list; the top node takes the id `root`, which a
// surface-creating Express block needs. `<name>.expected.json` is the
// createSurface message (with `<name>.data.json` as its data model, when
// there is one), and `<name>.a2ui` the same surface written in Express:
// one `id = Component(prop=value, …)` statement per component, children and
// slots by variable reference, `$path` bindings, renderer calls as calls,
// `Event(name, context)` actions, `_template($path, item)` child templates
// and one `$/key = value` statement per top-level data key.

import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const [pkgDir] = process.argv.slice(2);
if (!pkgDir) {
  console.error('usage: scripts/gen-eui-express.mjs <eui package dir>');
  process.exit(2);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pinned = /version: (\S+)/.exec(fs.readFileSync(path.join(root, 'vendor/eui/UPSTREAM'), 'utf8'))[1];
const pkg = JSON.parse(fs.readFileSync(path.join(pkgDir, 'package.json'), 'utf8'));
if (pkg.version !== pinned) {
  console.error(`eui ${pkg.version} is not the pinned ${pinned}`);
  process.exit(1);
}
const {flattenSurface} = await import(pathToFileURL(path.join(pkgDir, 'dist/index.js')).href);
const catalog = JSON.parse(fs.readFileSync(path.join(root, 'vendor/eui/catalog/core.schema.json'), 'utf8'));
const out = path.join(root, 'crates/a2ui-express/tests/eui');
fs.mkdirSync(out, {recursive: true});

const IDENT = /^[\p{XID_Start}_][\p{XID_Continue}]*$/u;
const RESERVED = new Set(['true', 'false', 'null', '_']);
const isIdent = s => IDENT.test(s) && !RESERVED.has(s);

function str(s) {
  return '"' + s.replace(/[\\"\n\r\t]/g, c => ({'\\': '\\\\', '"': '\\"', '\n': '\\n', '\r': '\\r', '\t': '\\t'})[c]) + '"';
}

const isObject = v => v !== null && typeof v === 'object' && !Array.isArray(v);

/** A value as Express. `ids` are the component ids, written as variables. */
function value(v, ids) {
  if (v === null || typeof v === 'boolean' || typeof v === 'number') return JSON.stringify(v);
  if (typeof v === 'string') return str(v);
  if (Array.isArray(v)) return `[${v.map(x => value(x, ids)).join(', ')}]`;
  const keys = Object.keys(v);
  if (keys.length === 1 && typeof v['@path'] === 'string' && /^[A-Za-z0-9_/]*$/.test(v['@path'])) {
    return '$' + v['@path'];
  }
  if (typeof v['@call'] === 'string' && keys.every(k => k === '@call' || k === 'args')) {
    const args = Object.entries(v.args ?? {}).map(([k, x]) => `${k}=${value(x, ids)}`);
    return `${v['@call']}(${args.join(', ')})`;
  }
  if (keys.length === 1 && isObject(v.event) && typeof v.event.name === 'string') {
    const ev = v.event;
    const rest = Object.keys(ev).filter(k => k !== 'name' && k !== 'context');
    if (rest.length === 0) {
      return ev.context === undefined
        ? `Event(${str(ev.name)})`
        : `Event(${str(ev.name)}, ${value(ev.context, ids)})`;
    }
  }
  if (keys.length === 1 && isObject(v.functionCall)) return value(v.functionCall, ids);
  return `{${keys.map(k => `${isIdent(k) ? k : str(k)}: ${value(v[k], ids)}`).join(', ')}}`;
}

/** A component reference: the variable when it is one, else the id. */
const ref = (id, ids) => (ids.has(id) && isIdent(id) ? id : str(id));

function prop(name, v, ids) {
  if (name === 'children') {
    if (Array.isArray(v) && v.every(x => typeof x === 'string')) {
      return `[${v.map(x => ref(x, ids)).join(', ')}]`;
    }
    if (isObject(v) && typeof v.componentId === 'string' && typeof v.path === 'string') {
      const extra = Object.keys(v).filter(k => k !== 'componentId' && k !== 'path');
      if (extra.length === 0) return `_template($${v.path}, ${ref(v.componentId, ids)})`;
    }
  }
  if (name === 'slots' && isObject(v) && Object.values(v).every(x => typeof x === 'string')) {
    return `{${Object.entries(v).map(([k, x]) => `${k}: ${ref(x, ids)}`).join(', ')}}`;
  }
  return value(v, ids);
}

function express(components, data) {
  const ids = new Set(components.map(c => c.id));
  const lines = [];
  for (const c of components) {
    if (!isIdent(c.id)) throw new Error(`component id ${c.id} is not an identifier`);
    const args = Object.entries(c)
      .filter(([k]) => k !== 'id' && k !== 'component')
      .map(([k, v]) => `${k === '@checks' ? 'checks' : k}=${prop(k, v, ids)}`);
    lines.push(`${c.id} = ${c.component}(${args.join(', ')})`);
  }
  for (const [k, v] of Object.entries(data ?? {})) {
    if (k === '$comment') continue;
    if (!/^[A-Za-z0-9_]+$/.test(k)) throw new Error(`data key ${k} is not a path segment`);
    lines.push(`$/${k} = ${value(v, new Set())}`);
  }
  return lines.join('\n') + '\n';
}

for (const name of ['dashboard', 'demo-surface', 'kitchen-sink']) {
  const tree = JSON.parse(fs.readFileSync(path.join(root, `vendor/eui/fixtures/${name}.json`), 'utf8'));
  const dataFile = path.join(root, `vendor/eui/fixtures/${name}.data.json`);
  const data = fs.existsSync(dataFile) ? JSON.parse(fs.readFileSync(dataFile, 'utf8')) : undefined;
  if (data) delete data.$comment;

  const components = flattenSurface(tree);
  const top = components[0].id;
  if (top !== 'root') {
    if (JSON.stringify(components.slice(1)).includes(JSON.stringify(top))) {
      throw new Error(`${name}: the top id ${top} is referenced`);
    }
    components[0].id = 'root';
  }

  // eui writes the check mixin under the legacy bare `checks`; its v1.0
  // catalog inherits `@checks` from Checkable, which is what the compiler
  // emits (and what a decompile → compile round trip yields upstream too).
  for (const c of components) {
    if ('checks' in c && !('@checks' in c)) {
      const entries = Object.entries(c).map(([k, v]) => [k === 'checks' ? '@checks' : k, v]);
      for (const k of Object.keys(c)) delete c[k];
      Object.assign(c, Object.fromEntries(entries));
    }
  }

  const create = {surfaceId: 'main', catalogId: catalog.catalogId, components};
  if (data && Object.keys(data).length > 0) create.dataModel = data;
  const expected = [{version: '1.0', createSurface: create}];

  fs.writeFileSync(path.join(out, `${name}.a2ui`), express(components, data));
  fs.writeFileSync(path.join(out, `${name}.expected.json`), JSON.stringify(expected, null, 2) + '\n');
  console.log(`${name}: ${components.length} components`);
}
