# Report: compact Express prompt vs eui's catalogPrompt (AXC-20, AXC-21)

How it was measured: [README.md](README.md). Generator: Claude Sonnet, one fresh
run per sample. Judge: Claude Opus, blind, one run over all 17 requests.

#AXC-21 changed the compact prompt and the compiler after #AXC-20's run (that
run's replies, results and verdict are in `rounds/3/`):

- **Rule 7** (checks) now ends: *A check reads the value its component binds
  to `value` (`value=$/form/email`), so bind it; on a component that binds
  none, name the value first: ?required($/form/terms, "Required"). Check
  arguments are positional, never named: ?length(8, "Too short").*
- **New rule 12, Data:** keep data in the data model and bind to it; bind
  every input's `value` and send it in events; repeated content is one
  template over a list (`List(children=_template($/items, item))`), not
  copies; only `~` properties take a binding, in a template too.
- Upstream's `(static)` rule and required-`action` rule are gone from the
  compact prompt: the `~` mark replaces the first, and no eui component takes
  an `action` argument (the compact legend already says actions go in `on`).
  That pays for the two additions.
- Every name in the new text (`value`, `required`, `length`, `List`,
  `children`, `Text(text=…)`) is read from the catalog, not hard-coded.
- **Compiler:** a check on a component that binds nothing for it to read
  (`?required("Email is required")` on an Input with no `value=`) is now a
  validation error instead of `required(value: "Email is required")`. No
  conformance case relies on the old behaviour; the suite stays 291/291 +
  229/229. The default (upstream) prompt is unchanged byte for byte.

## Prompt size (eui core catalog, 104 components)

| Prompt | Characters | chars / 4 | o200k_base | cl100k_base |
|---|---:|---:|---:|---:|
| Express compact, AXC-20 | 39,474 | 9,869 | 11,067 | 10,980 |
| **Express compact, AXC-21** | **39,615** | **9,904** | **11,142** | **11,054** |
| eui `catalogPrompt()` (JSON, hand-written) | 39,463 | 9,866 | 11,047 | 11,015 |

Still within eui's budget (10,000 at chars / 4); 75 o200k tokens over the
AXC-20 version, 95 over eui's own prompt.

## Samples (final compact prompt)

Each cell: validates in eui (✅/❌) · distinct component types · reply size (chars / 4) · judge fidelity F and component use C, each 1–5.
The eui JSON replies of the first 13 requests are the ones from earlier runs
(the eui prompt did not change); only the Express arm was run again.

### AXC-20 requests (seen while tuning)

| Request | Express (compact prompt) | eui JSON (catalogPrompt) | Judge |
|---|---|---|---|
| Login form | ✅ · 6 types · 206 tok · F4 C4 | ✅ · 7 types · 355 tok · F5 C5 | eui JSON |
| Dashboard | ✅ · 4 types · 284 tok · F4 C4 | ✅ · 6 types · 411 tok · F4 C5 | eui JSON |
| Settings page | ✅ · 7 types · 291 tok · F5 C5 | ✅ · 7 types · 367 tok · F5 C5 | tie |
| Chat composer | ✅ · 4 types · 195 tok · F4 C4 | ❌ composerBox: style.borderTop: not in the Box style whitelist · 7 types · 411 tok · F5 C5 | eui JSON |
| Product card grid | ✅ · 9 types · 316 tok · F5 C5 | ✅ · 10 types · 490 tok · F5 C5 | tie |
| Empty state | ✅ · 2 types · 75 tok · F5 C5 | ✅ · 2 types · 134 tok · F5 C5 | tie |
| Checkout form | ✅ · 11 types · 492 tok · F5 C5 | ✅ · 11 types · 937 tok · F5 C5 | eui JSON |
| Issue detail | ✅ · 9 types · 362 tok · F5 C5 | ✅ · 9 types · 551 tok · F5 C4 | Express |
| **Total** | 8/8 validate · 6.50 types · 277 tok/reply · F4.63 C4.63 | 7/8 validate · 7.38 types · 457 tok/reply · F4.88 C4.88 | Express 1, eui 4, tie 3 |

### AXC-21 requests (run once on the previous AXC-21 prompt, then seen)

| Request | Express (compact prompt) | eui JSON (catalogPrompt) | Judge |
|---|---|---|---|
| Sign-up form | ✅ · 5 types · 266 tok · F4 C5 | ✅ · 6 types · 548 tok · F5 C5 | eui JSON |
| Notification inbox | ✅ · 7 types · 276 tok · F4 C4 | ✅ · 6 types · 413 tok · F4 C5 | eui JSON |
| Pricing plans | ✅ · 9 types · 616 tok · F5 C4 | ✅ · 11 types · 1086 tok · F5 C5 | eui JSON |
| Edit profile | ✅ · 9 types · 355 tok · F4 C4 | ✅ · 9 types · 673 tok · F5 C5 | eui JSON |
| To-do list | ✅ · 8 types · 292 tok · F4 C4 | ✅ · 9 types · 455 tok · F4 C5 | eui JSON |
| **Total** | 5/5 validate · 7.60 types · 361 tok/reply · F4.20 C4.20 | 5/5 validate · 8.20 types · 635 tok/reply · F4.60 C5.00 | Express 0, eui 5, tie 0 |

### Held out (run once, on the final prompt)

| Request | Express (compact prompt) | eui JSON (catalogPrompt) | Judge |
|---|---|---|---|
| Kanban board | ✅ · 9 types · 413 tok · F5 C5 | ✅ · 8 types · 952 tok · F5 C3 | Express |
| Contact form | ✅ · 6 types · 218 tok · F4 C4 | ✅ · 7 types · 457 tok · F5 C5 | eui JSON |
| File browser | ✅ · 7 types · 417 tok · F5 C5 | ✅ · 8 types · 510 tok · F5 C5 | tie |
| Music player | ❌ compile: Property 'max' of component 'Progress' does not support dynamic data bindings (paths). · 0 types · 449 tok · F1 C1 | ✅ · 10 types · 713 tok · F5 C5 | eui JSON |
| **Total** | 3/4 validate · 5.50 types · 374 tok/reply · F3.75 C3.75 | 4/4 validate · 8.25 types · 658 tok/reply · F5.00 C4.50 | Express 1, eui 2, tie 1 |

### All 17 requests

| | Express (compact) | eui JSON |
|---|---|---|
| Validate in eui | 16/17 | 16/17 |
| Fidelity (mean) | 4.29 (4.50 without the reply that did not compile) | 4.82 |
| Component use (mean) | 4.29 | 4.82 |
| Reply size | 325 tok | 556 tok |
| Judge wins | 2 | 11 (4 ties) |

## Before / after on the AXC-20 requests

| | AXC-20 prompt | AXC-21 prompt |
|---|---|---|
| Express validates | 8/8 (2 of them checking their own message) | 8/8 (every check reads a bound value) |
| Express fidelity | 3.88 | 4.63 |
| Judge (Express / eui / tie) | 0 / 6 / 2 | 1 / 4 / 3 |

Different judge runs, so the scores of the two columns are not strictly
comparable (the eui replies, which did not change, went from F4.50 to F4.88).

## What the numbers say

- **Data binding is mostly fixed.** Every form input in the Express replies
  is now bound (`value=$/form/…`), and repeated content is a template over
  the data model in 6 of 8 replies that have any (products, issue timeline,
  inbox, to-do, kanban, music queue) plus two bound Tables (dashboard, files);
  AXC-20's 8 replies used none. The chat bubbles and the pricing
  cards are still written out one by one: both vary a literal-only property
  per item (`MessageBubble.side`, the highlighted plan's variant), which a
  template cannot bind, so the model chose copies over breaking rule 12's
  last sentence. The judge no longer flags unbound inputs on the Express
  side; on the kanban board it marks the *JSON* reply down for hard-coding.
- **Checks are fixed.** No Express reply checks its own message any more; the
  compiler would now refuse one (it refuses both AXC-20 replies that did).
- **Validity is level overall (16/17 each) but not on the held-out set
  (3/4 vs 4/4).** The failure is a binding on a literal-only property
  (`Progress(max=$duration)`): the push to bind made the model bind where the
  catalog does not allow it. Rule 12's last sentence was added for this after
  the AXC-21 requests' first run (2 of 13 replies failed that way,
  `rounds/4/`); it cut that to 1 of 17 but did not remove it. The compiler catches it with a clear
  error, so a repair turn would fix it; this eval allows none.
- **The judge gap narrowed but is not closed.** What the judge still prefers
  in the JSON replies is mostly layout polish: a Page or Card frame around a
  form, a titled Card around a table, a List of feature rows instead of
  stacked Text, labels on checkboxes. That comes from eui's prompt, which
  spends its budget on this catalog's idioms; the compact prompt is
  generated for any catalog and carries only the catalog's own instructions.
- **Two judge notes are outline artifacts, not reply faults.** The contact
  reply's check is `length(0, 500, …)`, but the outline cuts each check at 80
  characters, so the judge saw only `min: 0`. The sign-up note doubts that
  `required` rejects an unchecked box; eui's `required` treats `false` as
  missing, so it does. Scores were left as judged.
- **Output cost.** Express replies stay about 40% smaller (325 vs 556 tokens).

## Iterations (Express arm)

| Prompt version | Requests | Valid | Failures |
|---|---|---|---|
| AXC-20 1–3 | see `rounds/1`, `rounds/2`, `rounds/3` | 8/8 at v3 | 2 replies check their own message |
| AXC-21 4: rule 7 binding sentence + rule 12 Data | 13 (8 + 5 new) | 10/13 (`rounds/4/`) | `MessageBubble(side=$side)`, `Card(title=$name)`: bindings on literal-only properties in a template; `?length(min=8)`: named check argument (parse error) |
| AXC-21 5 (final): + "only `~` properties take a binding", "check arguments are positional" | 13 + 4 held out | 16/17 | held out: `Progress(max=$duration)` |

The 4 held-out requests were written after version 5 was fixed and run once.

## Caveats

- One sample per (arm, request), one generator model, one judge run. A smoke
  test, not a benchmark.
- The judge reads an outline of the rendered component tree, not pixels, and
  the outline truncates long values (see the contact note).
- The 5 AXC-21 requests were held out for version 4 and are tuning data for
  version 5; only the last 4 are held out for the final prompt.
