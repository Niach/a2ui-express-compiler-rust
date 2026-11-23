# Prompt eval: compact Express prompt vs eui's `catalogPrompt` (AXC-20, AXC-21, AXC-22)

Does a model given the compact Express prompt (`PromptStyle::Compact`) on
eui's core catalog write UIs as well as one given eui's own hand-written
`catalogPrompt` (which asks for A2UI v0.9 JSON)? The results are in
[`report.md`](report.md).

## Files

| Path | What |
|---|---|
| `requests.json` | The UI requests; `set` says which were tuning data (`axc20`, `axc21`, `heldout`: #AXC-21's held-out 4, seen since) and which were held out for the AXC-22 prompt (`fresh`, written after it was final) |
| `prompts/express-compact.txt` | `generate_prompt(eui core, PromptStyle::Compact)` |
| `prompts/eui-catalogPrompt.txt` | eui's `catalogPrompt()` at github.com/Niach/exponential `503b944` (`packages/exponential-ui`, 0.21.0) |
| `outputs/<arm>/<request>.txt` | What the model replied, verbatim |
| `score.ts` | Validates and measures every reply; writes `results.json` and, with `--blind`, the judge packets |
| `judge-brief.md` | What the blind judge is told |
| `judge/<request>.md`, `judge/key.json`, `judge/verdict.json` | The blind judge's input, the label → arm key it never saw, its verdict (`verdict-fresh.json`: the fresh requests, judged in their own run) |
| `report.ts` | Builds the result tables in `report.md` (one per request set) from `results.json` and the verdict |
| `rounds/` | Express replies (and prompt, results) from earlier versions of the compact prompt: 1–3 #AXC-20, 4 #AXC-21's first version, 5 #AXC-21's final one (with that run's eui replies; `5/rejudged/`: its Express replies judged again against the AXC-22 eui replies with the fixed outline) |

## Method

1. **Generate.** One fresh subagent per (arm, request), Claude Sonnet,
   given only the prompt file and the request (#AXC-22: both arms re-run
   on every request) ("write a single surface whose
   id is `main`"; Express arm: reply with the `<a2ui>` block; eui arm: reply
   with the messages as one JSON array). No tools besides reading the prompt
   and writing the reply; no feedback.
2. **Validate** (`score.ts`). Both arms go through the same eui pipeline:
   Express is compiled by this crate (`examples/express.rs compile`, a
   compile error fails the sample); JSON is parsed. The messages are fed to
   eui's `surfaceUpdates` (version check, the 0.9 bridge, the per-entry
   check) and the surface's components to `reduceSurface` with prop
   validation on. A sample **validates** when eui refuses nothing and
   reports no issue.
   - #AXC-19: the compiler writes the v1.0 check mixin as `@checks`, as eui's
     catalog declares it, but eui's runtime still refuses `@checks`. The
     score renames it `checks` before eui sees it and reports the count
     (`renamedChecks`); without that, every Express sample with checks fails.
3. **Variety**: distinct component types in the authored tree (macros not
   expanded, templates included).
4. **Judge.** A blind Claude Opus subagent ([`judge-brief.md`](judge-brief.md))
   reads, per request, the two rendered outlines under shuffled labels and
   scores fidelity and component use 1–5 and picks a winner. The outline
   shows every prop (bindings as `$/path`, calls as `name(arg=…)`, values
   cut at 240 characters), checks whole, actions (`on=[press→event …]`),
   style keys, a node's `visible` condition (#AXC-27) and which list a
   template repeats over.
5. **Held out.** Requests in the `fresh` set are written after the prompt
   is final and run once; the report lists them apart. `--keep-key` adds
   them without re-shuffling the labels of requests already judged.

## Reproduce

```sh
git clone --filter=blob:none --sparse https://github.com/Niach/exponential /tmp/eui && \
  git -C /tmp/eui sparse-checkout set packages/exponential-ui
cargo run -q -p a2ui-express --example express -- prompt vendor/eui/catalog/core.schema.json --compact \
  > eval/prompt/prompts/express-compact.txt
# (eui prompt: `catalogPrompt()` from /tmp/eui/packages/exponential-ui/src/entries/prompt.ts)
# generate outputs/<arm>/<request>.txt with a model, then:
bun eval/prompt/score.ts /tmp/eui/packages/exponential-ui --blind   # --keep-key to keep judged labels
# judge judge/*.md into judge/verdict.json (judge-brief.md), then:
bun eval/prompt/report.ts
```
