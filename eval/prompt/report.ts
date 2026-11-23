#!/usr/bin/env bun
// AXC-20, AXC-21, AXC-22: the result tables (markdown) from results.json and the judge's
// verdict, unblinded with judge/key.json. Prints it; report.md embeds it.
//
//   bun eval/prompt/report.ts

import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"

const HERE = import.meta.dir
const read = (p: string) => JSON.parse(readFileSync(join(HERE, p), `utf8`))
const allRows: { arm: string; request: string; validates: boolean; parsed: boolean; error?: string; issueSamples: string[]; distinct: number; components: number; replyChars: number }[] = read(`results.json`)
const allRequests: { id: string; title: string; set?: string }[] = read(`requests.json`)
const key: Record<string, Record<string, string>> = read(`judge/key.json`)
type Verdict = Record<string, Record<string, { fidelity: number; components: number }> & { winner: string }>
// #AXC-22: the fresh held-out requests were judged in a run of their own.
const verdict: Verdict = {
  ...read(`judge/verdict.json`),
  ...(existsSync(join(HERE, `judge/verdict-fresh.json`)) ? read(`judge/verdict-fresh.json`) : {}),
}
const ARMS = [`express-compact`, `eui-json`]

const score = (req: string, arm: string) => {
  const label = Object.entries(key[req] ?? {}).find(([, a]) => a === arm)?.[0]
  return label ? verdict[req]?.[label] : undefined
}
const winner = (req: string) => {
  const w = verdict[req]?.winner
  return w === `tie` ? `tie` : (key[req]?.[w] ?? `?`) === `express-compact` ? `Express` : `eui JSON`
}

const cell = (req: string, arm: string) => {
  const r = allRows.find((x) => x.arm === arm && x.request === req)
  if (!r) return `–`
  const ok = r.validates ? `✅` : r.parsed ? `❌ ${r.issueSamples[0] ?? ``}` : `❌ ${r.error ?? ``}`
  const s = score(req, arm)
  return `${ok} · ${r.distinct} types · ${Math.ceil(r.replyChars / 4)} tok · F${s?.fidelity ?? `?`} C${s?.components ?? `?`}`
}

const SETS: [string, string][] = [
  [`axc20`, `AXC-20 requests (seen while tuning)`],
  [`axc21`, `AXC-21 requests (run once on the previous AXC-21 prompt, then seen)`],
  [`heldout`, `AXC-21 held out (run once on the AXC-21 prompt, then seen)`],
  [`fresh`, `Fresh held out (written after the AXC-22 prompt was final, run once)`],
  [`all17`, `All 17 (AXC-20 + AXC-21 + AXC-21 held out)`],
]
const inSet = (q: { set?: string }, set: string) =>
  set === `all17` ? (q.set ?? `axc20`) !== `fresh` : (q.set ?? `axc20`) === set
const out: string[] = []
for (const [set, heading] of SETS) {
  const requests = allRequests.filter((q) => inSet(q, set))
  if (!requests.length) continue
  const ids = new Set(requests.map((q) => q.id))
  const rows = allRows.filter((r) => ids.has(r.request))
  const lines = [
    `| Request | Express (compact prompt) | eui JSON (catalogPrompt) | Judge |`,
    `|---|---|---|---|`,
    ...(set === `all17` ? [] : requests.map((q) => `| ${q.title} | ${cell(q.id, ARMS[0])} | ${cell(q.id, ARMS[1])} | ${winner(q.id)} |`)),
  ]
  const total = (arm: string) => {
    const rs = rows.filter((r) => r.arm === arm)
    const f = requests.map((q) => score(q.id, arm)?.fidelity ?? 0)
    const c = requests.map((q) => score(q.id, arm)?.components ?? 0)
    const avg = (xs: number[]) => (xs.reduce((a, b) => a + b, 0) / xs.length).toFixed(2)
    return `${rs.filter((r) => r.validates).length}/${rs.length} validate · ${avg(rs.map((r) => r.distinct))} types · ${Math.round(rs.reduce((a, r) => a + r.replyChars, 0) / 4 / rs.length)} tok/reply · F${avg(f)} C${avg(c)}`
  }
  const wins = (who: string) => requests.filter((q) => winner(q.id) === who).length
  lines.push(`| **Total** | ${total(ARMS[0])} | ${total(ARMS[1])} | Express ${wins(`Express`)}, eui ${wins(`eui JSON`)}, tie ${wins(`tie`)} |`)
  out.push(`### ${heading}`, ``, ...lines, ``)
}
console.log(out.join(`\n`))
