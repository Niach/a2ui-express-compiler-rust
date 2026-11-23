# Report: compact Express prompt vs eui's catalogPrompt (AXC-20)

How it was measured: [README.md](README.md). Generator: Claude Sonnet, one fresh
run per sample. Judge: Claude Opus, blind.

## Prompt size (eui core catalog, 104 components)

| Prompt | Characters | chars / 4 | o200k_base | cl100k_base |
|---|---:|---:|---:|---:|
| Express, `PromptStyle::Upstream` (default) | 457,575 | 114,394 | 125,811 | 121,875 |
| **Express, `PromptStyle::Compact`** | **39,474** | **9,869** | **11,067** | **10,980** |
| eui `catalogPrompt()` (JSON, hand-written) | 39,463 | 9,866 | 11,047 | 11,015 |

The budget in eui's `prompt-budget.json` is 10,000 tokens, counted as chars / 4
(characters as JavaScript counts them). The compact prompt fits that budget, and
a real tokenizer (tiktoken, offline) puts it level with eui's own prompt. Of the
compact prompt, about 1.1k o200k tokens are upstream's Express rules, kept
verbatim, and 2.1k are the 591 icon names, listed once.

## Samples (final compact prompt)

Each cell: validates in eui (✅/❌) · distinct component types · reply size (chars / 4) · judge fidelity F and component use C, each 1–5.

| Request | Express (compact prompt) | eui JSON (catalogPrompt) | Judge |
|---|---|---|---|
| Login form | ✅ · 6 types · 175 tok · F3 C4 | ✅ · 7 types · 355 tok · F5 C5 | eui JSON |
| Dashboard | ✅ · 5 types · 291 tok · F4 C4 | ✅ · 6 types · 411 tok · F4 C5 | eui JSON |
| Settings page | ✅ · 7 types · 305 tok · F4 C5 | ✅ · 7 types · 367 tok · F4 C5 | tie |
| Chat composer | ✅ · 6 types · 223 tok · F4 C5 | ❌ composerBox: style.borderTop: not in the Box style whitelist · 7 types · 411 tok · F4 C4 | eui JSON |
| Product card grid | ✅ · 10 types · 582 tok · F4 C4 | ✅ · 10 types · 490 tok · F4 C5 | eui JSON |
| Empty state | ✅ · 2 types · 79 tok · F5 C5 | ✅ · 2 types · 134 tok · F5 C5 | tie |
| Checkout form (held out) | ✅ · 11 types · 517 tok · F3 C4 | ✅ · 11 types · 937 tok · F5 C5 | eui JSON |
| Issue detail (held out) | ✅ · 8 types · 423 tok · F4 C3 | ✅ · 9 types · 551 tok · F5 C5 | eui JSON |
| **Total** | 8/8 validate · 6.88 types · 324 tok/reply · F3.88 C4.25 | 7/8 validate · 7.38 types · 457 tok/reply · F4.50 C4.88 | Express 0, eui 6, tie 2 |

The first six requests were also used while tuning the compact prompt (see
*Iterations*). The last two were held out and run once, after tuning.

### What the numbers say

- **Validity.** All 8 Express replies compile and pass eui's validation; 7 of 8
  JSON replies do (the chat reply uses `borderTop`, which is not in eui's Box
  style whitelist). The Express login reply has 2 `@checks` keys that the score
  renames `checks` for eui (#AXC-19).
- **Output cost.** Express replies are about 30% smaller (324 vs 457 tokens per
  reply at chars / 4).
- **Quality.** The blind judge prefers the JSON replies: eui 6, Express 0, 2 ties.
  Fidelity averages 3.88 vs 4.50, component use 4.25 vs 4.88. The judge's notes
  point to two gaps:
  1. **Data binding.** The Express replies bind less often. They hard-code the
     product cards and the issue timeline where the JSON replies use a template
     over the data model, and they leave inputs unbound. eui's prompt says
     "Keep data in the model; bind to it" and points at List and templates.
     Upstream's Express rules say less about this, and the compact prompt keeps
     them verbatim.
  2. **Checks without a bound value.** `?required("Email is required")` on an
     unbound Input compiles to `required(value: "Email is required")`. The
     check has no value of its own to read, so the message string fills its
     argument. This happened in the login and checkout replies. The rule text
     says the message is an extra string argument, but it never says the
     check needs the field's value bound.
- **Same components.** Both prompts led the model to similar components
  (6.9 vs 7.4 distinct types per reply): Page, Form, Card, Grid, StatTile,
  Table, Tabs, Composer, EmptyState.

## Iterations (Express arm, same six requests)

| Prompt version | Valid | Failures |
|---|---|---|
| 1: shared types, common properties, `~` for bindable | 3/6 | `action=` on Button (from upstream's rule-1 example); `aspectRatio="1/1"` (number); `Page("Projects", [children])` (positional children landed in `subtitle`) |
| 2: + action hint (`on={…: Event(…)}`, "no `action` argument"), number marks | 4/6 | positional children landed in `Grid.columns`; `cancel=Event(…)` on a boolean |
| 3 (final): + "name every argument", `:n`/`:b` marks | 6/6 (+2/2 held out) | – |

Replies from versions 1 and 2 are in `rounds/`.

## Caveats

- One sample per (arm, request), one generator model, one judge. This is a
  smoke test, not a benchmark.
- The judge reads an outline of the rendered component tree, not pixels.
- The eui arm runs on a prompt written for this catalog by hand. The compact
  prompt is generated from the catalog schema for any catalog, and the only
  catalog text it keeps is the catalog's own `instructions`.
