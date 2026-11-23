#!/usr/bin/env bun
// AXC-20: scores the sample UIs a model wrote from each prompt.
//
//   bun eval/prompt/score.ts <eui-package-dir> [--blind]
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
}

const TEXT_PROPS = [`title`, `label`, `text`, `heading`, `subtitle`, `description`, `placeholder`, `value`, `message`, `actionLabel`, `name`, `variant`, `kind`, `type`, `icon`]

/** The rendered tree as an indented outline: the judge's view of a sample. */
function outline(node: Node, depth = 0, lines: string[] = []): string[] {
  const shown = TEXT_PROPS.filter((k) => node.props?.[k] !== undefined)
    .map((k) => {
      const v = node.props[k]
      return `${k}=${typeof v === `string` ? JSON.stringify(v) : JSON.stringify(v).slice(0, 60)}`
    })
    .join(` `)
  const lists = [`columns`, `options`, `items`, `tabs`, `rows`, `series`]
    .filter((k) => Array.isArray(node.props?.[k]))
    .map((k) => `${k}[${(node.props[k] as unknown[]).length}]`)
    .join(` `)
  const on = node.props?.on && typeof node.props.on === `object` ? Object.keys(node.props.on as object) : []
  const checks = Array.isArray(node.props?.checks) ? (node.props.checks as unknown[]).map((c) => JSON.stringify(c).slice(0, 80)) : []
  const extra = [on.length ? `on=[${on.join(`, `)}]` : ``, checks.length ? `checks=[${checks.join(`, `)}]` : ``].filter(Boolean).join(` `)
  lines.push(`${`  `.repeat(depth)}${node.component}${shown ? ` ${shown}` : ``}${lists ? ` ${lists}` : ``}${extra ? ` ${extra}` : ``}`)
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
      ...(data.length ? [`[data model] ${data.join(`; `).slice(0, 800)}`] : []),
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
  const key: Record<string, Record<string, Arm>> = {}
  for (const req of requests) {
    const arms = [...ARMS].sort(() => Math.random() - 0.5)
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
  writeFileSync(join(HERE, `judge`, `key.json`), `${JSON.stringify(key, null, 2)}\n`)
}

for (const r of rows)
  console.log(
    `${r.arm.padEnd(16)} ${r.request.padEnd(16)} ${r.validates ? `valid` : r.parsed ? `INVALID` : `NO-PARSE`} refused=${r.refused} issues=${r.issues} components=${r.components} distinct=${r.distinct} chars=${r.replyChars}${r.error ? ` ${r.error}` : ``}`,
  )
