#!/usr/bin/env bun
// AXC-20: scores the sample UIs a model wrote from each prompt.
//
//   bun eval/prompt/score.ts <eui-package-dir> [--blind [--keep-key]]
//
// <eui-package-dir> is packages/exponential-ui of a checkout of
// github.com/Niach/exponential (the version vendor/eui/UPSTREAM pins). Both
// arms go through the same eui pipeline: the A2UI messages (Express compiled
// by this crate, JSON as the model wrote it) are fed to eui's
// `surfaceUpdates` (the bare-surface router: version check, 0.9 bridge,
// per-entry check), and the surface's components to `reduceSurface` with
// prop validation on. A sample `validates` when it compiles (Express) or
// parses (JSON) and eui refuses nothing and reports no issue.
//
// Writes eval/prompt/results.json; with --blind, also the judge packets
// (eval/prompt/judge/<request>.md: each sample's rendered outline under a
// shuffled label, with the key in judge/key.json).

import { spawnSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"

const HERE = import.meta.dir
const ROOT = resolve(HERE, `../..`)
const CATALOG = join(ROOT, `vendor/eui/catalog/core.schema.json`)
const euiDir = resolve(process.argv[2] ?? ``)
const blind = process.argv.includes(`--blind`)
if (!existsSync(join(euiDir, `src/index.ts`))) {
  console.error(`usage: bun eval/prompt/score.ts <eui-package-dir> [--blind]`)
  process.exit(2)
}

const eui = await import(join(euiDir, `src/index.ts`))
const { reduceSurface, surfaceUpdates, newSurfaceBridge } = eui
const CORE: string = eui.CORE_CATALOG_ID

interface Request {
  id: string
  title: string
  request: string
}
const requests: Request[] = JSON.parse(readFileSync(join(HERE, `requests.json`), `utf8`))
const ARMS = [`express-compact`, `eui-json`] as const
type Arm = (typeof ARMS)[number]

const run = (args: string[], input: string) => {
  const out = spawnSync(`cargo`, [`run`, `-q`, `-p`, `a2ui-express`, `--example`, `express`, `--`, ...args], {
    cwd: ROOT,
    input,
    encoding: `utf8`,
    maxBuffer: 64 << 20,
  })
  return { ok: out.status === 0, stdout: out.stdout, stderr: out.stderr }
}

/** #AXC-19: eui's catalog declares the v1.0 check mixin as `@checks`
 *  (common_types `Checkable`), which the compiler writes, but eui's runtime
 *  still reads `checks` and refuses `@checks`. Until eui reads `@checks`,
 *  the compiled components are given eui's spelling; the count is reported. */
function renameChecks(messages: unknown[]): number {
  let n = 0
  for (const m of messages as Record<string, Record<string, unknown>>[]) {
    for (const key of [`createSurface`, `updateComponents`]) {
      const list = m?.[key]?.components
      if (!Array.isArray(list)) continue
      for (const c of list as Record<string, unknown>[]) {
        if (c && `@checks` in c) {
          c.checks = c[`@checks`]
          delete c[`@checks`]
          n++
        }
      }
    }
  }
  return n
}

/** The messages a reply carries, or why there are none. */
function messages(arm: Arm, reply: string): { messages?: unknown[]; error?: string; renamedChecks?: number } {
  if (arm === `express-compact`) {
    if (!/<a2ui>[\s\S]*<\/a2ui>/.test(reply)) return { error: `no <a2ui>…</a2ui> block` }
    const block = reply.slice(reply.indexOf(`<a2ui>`), reply.lastIndexOf(`</a2ui>`) + `</a2ui>`.length)
    const out = run([`compile`, CATALOG], block)
    const parsed = JSON.parse(out.stdout || `{"error": ${JSON.stringify(out.stderr)}}`)
    return out.ok ? { messages: parsed, renamedChecks: renameChecks(parsed) } : { error: `compile: ${parsed.error}` }
  }
  const fenced = reply.match(/```(?:json)?\s*\n([\s\S]*?)```/)
  const text = (fenced ? fenced[1] : reply).trim()
  try {
    const value = JSON.parse(text)
    return { messages: Array.isArray(value) ? value : [value] }
  } catch {
    // One message per line (JSONL).
    try {
      return { messages: text.split(`\n`).filter((l) => l.trim()).map((l) => JSON.parse(l)) }
    } catch (e) {
      return { error: `JSON: ${(e as Error).message}` }
    }
  }
}

interface Node {
  id: string
  component: string
  props: Record<string, unknown>
  children: Node[]
  slots?: Record<string, Node>
  /** A data template's children: one `component` per item at `path`. */
  template?: { component: string; path: string }
  /** eui keeps a component's actions here, beside its props. */
  on?: Record<string, unknown>
  style?: Record<string, unknown>
  /** eui keeps a component's `visible` condition here too. */
  visible?: unknown
}

/** Props the outline leaves out: the tree, events and checks are drawn apart. */
const SKIP_PROPS = new Set([`id`, `component`, `children`, `slots`, `on`, `checks`, `@checks`, `style`, `accessibility`, `visible`])
/** Long values are cut here (#AXC-22: was 60, and 80 for a check, which hid
 *  a check's later arguments). */
const MAX_VALUE = 240

const cut = (s: string) => (s.length > MAX_VALUE ? `${s.slice(0, MAX_VALUE)}…` : s)

/** A dynamic value as the judge reads it: a binding `$/path`, a call
 *  `name(arg=…)`, else the literal. */
function value(v: unknown): string {
  if (v && typeof v === `object` && !Array.isArray(v)) {
    const o = v as Record<string, unknown>
    const path = o[`@path`] ?? o.path
    if (typeof path === `string` && Object.keys(o).length === 1) return `$${path}`
    const call = o[`@call`] ?? o.call
    if (typeof call === `string`) {
      const args = o.args && typeof o.args === `object` ? (o.args as Record<string, unknown>) : {}
      const list = Array.isArray(o.args)
        ? (o.args as unknown[]).map(value)
        : Object.entries(args).map(([k, a]) => `${k}=${value(a)}`)
      return `${call}(${list.join(`, `)})`
    }
    return `{${Object.entries(o).map(([k, x]) => `${k}: ${value(x)}`).join(`, `)}}`
  }
  if (Array.isArray(v)) return `[${v.map(value).join(`, `)}]`
  return JSON.stringify(v)
}

/** A prop as the outline shows it: every value, lists of items by their
 *  label, long values cut at MAX_VALUE. */
function prop(k: string, v: unknown): string {
  if (Array.isArray(v) && v.some((x) => x && typeof x === `object`)) {
    const names = v.map((x) => {
      const o = (x ?? {}) as Record<string, unknown>
      const n = o.label ?? o.title ?? o.name ?? o.key ?? o.value
      return n === undefined ? value(x) : value(n)
    })
    return `${k}[${v.length}: ${cut(names.join(` | `))}]`
  }
  return `${k}=${cut(value(v))}`
}

/** The rendered tree as an indented outline: the judge's view of a sample. */
function outline(node: Node, depth = 0, lines: string[] = []): string[] {
  const props = node.props ?? {}
  const shown = Object.entries(props)
    .filter(([k, v]) => !SKIP_PROPS.has(k) && v !== undefined)
    .map(([k, v]) => prop(k, v))
    .join(` `)
  // #AXC-22: eui keeps `on` on the node, not in its props; the outline
  // used to read only the props and so showed no actions at all.
  const onMap = node.on ?? props.on
  const on = onMap && typeof onMap === `object` ? Object.entries(onMap as Record<string, unknown>) : []
  const events = on.map(([k, a]) => {
    const act = (a ?? {}) as Record<string, Record<string, unknown>>
    const target = act.event ? `event ${String(act.event.name)}${act.event.context ? ` ${value(act.event.context)}` : ``}` : act.functionCall ? value(act.functionCall) : value(a)
    return `${k}→${cut(target)}`
  })
  const rawChecks = props.checks ?? props[`@checks`]
  const checks = Array.isArray(rawChecks)
    ? (rawChecks as Record<string, unknown>[]).map((c) => `${value(c?.condition)}${c?.message !== undefined ? ` ${JSON.stringify(c.message)}` : ``}`)
    : []
  const styleMap = node.style ?? props.style
  const style = styleMap && typeof styleMap === `object` ? Object.keys(styleMap as object) : []
  // #AXC-27: `visible` is on the node as well. Without it, two branches
  // swapped by `visible=$/loading` read as both rendered at once.
  const visible = node.visible ?? props.visible
  const extra = [
    visible !== undefined ? `visible=${cut(value(visible))}` : ``,
    node.template ? `children=[template ${node.template.component}, once per item of $${node.template.path}]` : ``,
    events.length ? `on=[${events.join(`, `)}]` : ``,
    checks.length ? `checks=[${checks.join(`, `)}]` : ``,
    style.length ? `style{${style.join(`, `)}}` : ``,
  ]
    .filter(Boolean)
    .join(` `)
  lines.push(`${`  `.repeat(depth)}${node.component}${shown ? ` ${shown}` : ``}${extra ? ` ${extra}` : ``}`)
  for (const [name, slot] of Object.entries(node.slots ?? {})) {
    lines.push(`${`  `.repeat(depth + 1)}[slot ${name}]`)
    outline(slot, depth + 2, lines)
  }
  for (const child of node.children ?? []) outline(child, depth + 1, lines)
  return lines
}

function collect(node: Node, out: string[] = []): string[] {
  out.push(node.component)
  for (const slot of Object.values(node.slots ?? {})) collect(slot, out)
  for (const child of node.children ?? []) collect(child, out)
  return out
}

interface Row {
  arm: Arm
  request: string
  replyChars: number
  parsed: boolean
  error?: string
  refused: number
  issues: number
  issueSamples: string[]
  validates: boolean
  /** Components whose `@checks` were renamed `checks` (#AXC-19). */
  renamedChecks: number
  components: number
  distinct: number
  types: string[]
  outline?: string
}

const rows: Row[] = []
for (const arm of ARMS) {
  for (const req of requests) {
    const file = join(HERE, `outputs`, arm, `${req.id}.txt`)
    if (!existsSync(file)) continue
    const reply = readFileSync(file, `utf8`)
    const row: Row = {
      arm,
      request: req.id,
      replyChars: reply.length,
      parsed: false,
      refused: 0,
      issues: 0,
      issueSamples: [],
      validates: false,
      renamedChecks: 0,
      components: 0,
      distinct: 0,
      types: [],
    }
    const got = messages(arm, reply)
    if (got.error || !got.messages) {
      row.error = got.error
      rows.push(row)
      continue
    }
    row.parsed = true
    row.renamedChecks = got.renamedChecks ?? 0
    const bridge = newSurfaceBridge()
    const byId = new Map<string, Record<string, unknown>>()
    const refusals: string[] = []
    const data: string[] = []
    for (const m of got.messages) {
      const { updates, refused } = surfaceUpdates(m, bridge)
      refusals.push(...refused.map((r: { message: string; path?: string }) => `${r.path ?? ``} ${r.message}`.trim()))
      for (const u of updates) {
        if (u.kind === `create` || u.kind === `components`) for (const c of u.components) byId.set(c.id, c)
        if (u.kind === `create` && u.dataModel) data.push(JSON.stringify(u.dataModel))
        if (u.kind === `data`) data.push(`${u.path || `/`} = ${JSON.stringify(u.value)}`)
      }
    }
    const reduced = reduceSurface([...byId.values()], { catalogId: bridge.catalogId ?? CORE })
    const issues = reduced.issues.map((i: { id?: string; message: string }) => `${i.id ?? ``}: ${i.message}`)
    row.refused = refusals.length
    row.issues = issues.length
    row.issueSamples = [...refusals, ...issues].slice(0, 8)
    // The authored tree (macros not expanded), templates included: what
    // the variety count and the judge read.
    const authored = reduceSurface([...byId.values()], { catalogId: bridge.catalogId ?? CORE, expand: false, validate: false })
    const templates = Object.entries((authored.templates ?? {}) as Record<string, Node>)
    const types = [authored.root as Node, ...templates.map(([, t]) => t)]
      .flatMap((n) => collect(n))
      .filter((c) => c !== `Unknown`)
    row.components = byId.size
    row.types = [...new Set(types)].sort()
    row.distinct = row.types.length
    row.validates = byId.size > 0 && refusals.length === 0 && issues.length === 0
    row.outline = [
      ...outline(authored.root as Node),
      ...templates.flatMap(([id, t]) => [`[template ${id}: repeated once per item of its list]`, ...outline(t, 1)]),
      ...(data.length ? [`[data model] ${data.join(`; `).slice(0, 1500)}`] : []),
    ].join(`\n`)
    rows.push(row)
  }
}

writeFileSync(
  join(HERE, `results.json`),
  `${JSON.stringify(rows.map(({ outline: _, ...r }) => r), null, 2)}\n`,
)

if (blind) {
  // Shuffle the arms per request; the key is written apart from the packets.
  mkdirSync(join(HERE, `judge`), { recursive: true })
  // --keep-key: requests already in judge/key.json keep their labels, so a
  // verdict on them stays valid when new requests are added (#AXC-22).
  const keyFile = join(HERE, `judge`, `key.json`)
  const kept: Record<string, Record<string, Arm>> =
    process.argv.includes(`--keep-key`) && existsSync(keyFile) ? JSON.parse(readFileSync(keyFile, `utf8`)) : {}
  const key: Record<string, Record<string, Arm>> = {}
  for (const req of requests) {
    const old = kept[req.id]
    const arms = old ? Object.keys(old).sort().map((l) => old[l]) : [...ARMS].sort(() => Math.random() - 0.5)
    key[req.id] = {}
    const parts = [`# ${req.title}\n\nRequest: ${req.request}\n`]
    arms.forEach((arm, i) => {
      const label = String.fromCharCode(65 + i)
      key[req.id][label] = arm
      const row = rows.find((r) => r.arm === arm && r.request === req.id)
      parts.push(`## Sample ${label}\n\n\`\`\`\n${row?.outline ?? `(nothing rendered: the reply did not parse)`}\n\`\`\`\n`)
    })
    writeFileSync(join(HERE, `judge`, `${req.id}.md`), parts.join(`\n`))
  }
  writeFileSync(keyFile, `${JSON.stringify(key, null, 2)}\n`)
}

for (const r of rows)
  console.log(
    `${r.arm.padEnd(16)} ${r.request.padEnd(16)} ${r.validates ? `valid` : r.parsed ? `INVALID` : `NO-PARSE`} refused=${r.refused} issues=${r.issues} components=${r.components} distinct=${r.distinct} chars=${r.replyChars}${r.error ? ` ${r.error}` : ``}`,
  )
