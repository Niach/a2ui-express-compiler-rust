# Judge brief (blind)

You judge generated user interfaces. Each file `judge/<request>.md` holds one
UI request and two samples, A and B: each an outline of the rendered component
tree a model wrote for that request (component, its props, `$/path` data
bindings, `name(...)` function calls, `on=[event→action]`, `checks=[...]`,
`style{keys}`, `visible=<condition>` on a component shown only when it
holds, data templates and the data model). The components come from
one design system (shadcn-like names: Page, Card, List, Row, Form, ...); both
samples use the same one. Long values are cut with `…`.

For each request, score each sample from 1 to 5 on:

- **fidelity**: does it build everything the request asks, correctly
  (content, behaviour, validation, data binding where asked)?
- **components**: does it use the design system well: fitting components,
  a clear, well-structured screen, idiomatic over hand-built?

Then pick the `winner`: "A", "B", or "tie" when neither is clearly better.
Add a one-sentence `notes` per sample. Judge each request on its own; the
order of the samples means nothing.

Read only the files `judge/*.md` named below. Do not read `judge/key.json`,
`judge/verdict.json`, `outputs/`, `results.json` or anything else.

Write `judge/verdict.json`:

{"<request>": {"A": {"fidelity": n, "components": n, "notes": "..."},
               "B": {"fidelity": n, "components": n, "notes": "..."},
               "winner": "A" | "B" | "tie"}, ...}
