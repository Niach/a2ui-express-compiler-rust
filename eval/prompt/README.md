# Prompt eval: compact Express prompt vs eui's `catalogPrompt` (AXC-20, AXC-21)

Does a model given the compact Express prompt (`PromptStyle::Compact`) on
eui's core catalog write UIs as well as one given eui's own hand-written
`catalogPrompt` (which asks for A2UI v0.9 JSON)? The results are in
[`report.md`](report.md).

## Files

| Path | What |
|---|---|
| `requests.json` | The UI requests; `set` says which were tuning data (`axc20`, `axc21`) and which were held out for the final prompt (`heldout`) |
| `prompts/express-compact.txt` | `generate_prompt(eui core, PromptStyle::Compact)` |
| `prompts/eui-catalogPrompt.txt` | eui's `catalogPrompt()` at github.com/Niach/exponential `503b944` (`packages/exponential-ui`, 0.21.0) |
| `outputs/<arm>/<request>.txt` | What the model replied, verbatim |
| `score.ts` | Validates and measures every reply; writes `results.json` and, with `--blind`, the judge packets |
| `judge/<request>.md`, `judge/key.json`, `judge/verdict.json` | The blind judge's input, the label → arm key it never saw, its verdict |
| `report.ts` | Builds the result tables in `report.md` (one per request set) from `results.json` and the verdict |
| `rounds/` | Express replies (and prompt, results) from earlier versions of the compact prompt: 1–3 #AXC-20, 4 #AXC-21's first version |

## Method

1. **Generate.** One fresh subagent per (arm, request), Claude Sonnet,
   given only the prompt file and the request ("write a single surface whose
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
4. **Judge.** A blind Claude Opus subagent reads, per request, the two
   rendered outlines under shuffled labels and scores fidelity and
   component use 1–5 and picks a winner.
5. **Held out.** Requests in the `heldout` set are written after the prompt
   is final and run once; the report lists them apart.

## Reproduce

```sh
git clone --filter=blob:none --sparse https://github.com/Niach/exponential /tmp/eui && \
  git -C /tmp/eui sparse-checkout set packages/exponential-ui
cargo run -q -p a2ui-express --example express -- prompt vendor/eui/catalog/core.schema.json --compact \
  > eval/prompt/prompts/express-compact.txt
# (eui prompt: `catalogPrompt()` from /tmp/eui/packages/exponential-ui/src/entries/prompt.ts)
# generate outputs/<arm>/<request>.txt with a model, then:
bun eval/prompt/score.ts /tmp/eui/packages/exponential-ui --blind
# judge judge/*.md into judge/verdict.json, then:
bun eval/prompt/report.ts
```
