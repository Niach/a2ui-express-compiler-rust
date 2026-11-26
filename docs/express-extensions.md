# Express extensions (`at_exponential_express`)

One DSL for theme, layout and motion, plus patches, so a model writes fewer
tokens for an eUI screen. Everything here is on only when a catalog declares
the extension; with no catalog declaring it, the compiler, decompiler, prompt
generator and response reader stay upstream Express, byte for byte (the
upstream conformance suite stays 282/282). The wire stays plain A2UI 1.0:
every form is expanded at compile time.

The code is in `crates/a2ui-express/src/ext/`. The conformance fixtures are in
`conformance/extensions/`, one file per statement, and are replayed by Rust
(`tests/extensions.rs`), wasm (`packages/npm/test/extensions.test.js`, both
flavours) and Python (`crates/a2ui-express-py/tests/test_extensions.py`).

## Turning it on

```json
"metadata": {"extensions": {"at_exponential_express": {
  "version": 1,
  "themeFunction": "setTheme",
  "motions": {"pulse-dot": {"size": [24, 24], "duration": 1200, "layers": [ … ]}}
}}}
```

`true` turns it on with the defaults. `motions` is the `Motion(ref=…)`
registry: name → Motion props (eUI does not ship a registry; the eUI catalogue
batch agreed on 2026-11-26 that it lives here). `themeFunction` is the renderer
function a live theme change calls (default `setTheme`).
`conformance/extensions/catalog.json` is a small catalog that declares it.

## `theme(...)`

```
theme(base="dark", primary=#eef800, bg=#0e0e0e, card=#161616, radius=0, font="Inter",
      scale=1, density="compact", shadow="none", button="pill")
```

Seeds: `base` (`mode`): `light` | `dark`, the mode the colour seeds describe;
`bg` (`background`), `fg` (`foreground`), `card`, `primary`, `secondary`,
`muted`, `accent`, `border`, `charts=[…]` (up to 8, cycled); `radius` (px, =
`radius.md`); `font` (`type.family.sans`); `display` (headings: set as
`type.family.serif`, with a Heading recipe); `scale` (type scale); `density`
(`compact` | `default` | `comfortable`, spacing and control heights); `shadow`
(`none` | `soft` | `strong`); `button` (`rounded` | `pill` | `square`);
`stroke` (hairline width). Plus `id`, `name`, `extends` (default `neutral`).

Any full token may be given too: a colour token (`cardForeground=#111111`) sets
the base mode, `light={…}` / `dark={…}` a mode, `tokens={…}`, `fonts={…}`,
`recipes={…}` merge over what the seeds produced. `theme("orange")` picks a
built-in theme by id.

**The expander** (`ext/theme.rs`) is deterministic and contrast-checked. It
writes every shadcn colour token in both modes. The base mode comes from the
seeds, which it uses as given. The other mode is derived: surfaces mirror in
OKLab lightness, hues keep their hue and chroma, and both are lifted until they
contrast enough. Every `xForeground` gets 4.5:1 on `x`; `mutedForeground` 4.5:1
on `background` and `muted`; `link` 4.5:1, and `ring` and the charts 3:1, on
`background`. It started from the pilot's `expand` (genui-bench
`docs/round4/pilot/pilot.py`), which gave both modes the same colours. The
arithmetic (`ext/color.rs`) uses only `+ - * /`: the sRGB transfer function is
a table, and the OKLab cube root is a fixed number of Newton steps. That keeps
the output identical natively, in wasm and in Python, and it is pinned as golden
JSON (`theme.json`, `expand_theme` cases). The result `extends` `neutral`, so
the tokens it does not set fall back the way eUI's theme chain does.

**The wire** (#VAPP-289): on a new surface the theme goes into
`createSurface.metadata.extensions.at_exponential_theme` (the full theme
JSON, or the id). Write `theme(...)` before `root`. On a surface that already
exists (an `update=true` scope, a scope without `root`, a stream that already
created the surface, a patch), it compiles to:

```json
{"version": "1.0", "callRendererFunction": {"surfaceId": "main", "functionCallId": "call_1",
  "callFunction": {"catalogId": "…", "@call": "setTheme", "args": {"theme": { … }}}}}
```

**Decision, 2026-11-26**, agreed with #VAPP-289 (session b7237fe9): a theme
change on a live surface is plain 1.0 `callRendererFunction` to the core
function `setTheme`, args `{theme: <built-in id | full theme JSON>}`. The
call's `surfaceId` names the target, and the function returns void with
`allowedCallers: rendererOrAgent`, so a theme toggle can call it too. An unknown
id or an invalid theme is `INVALID_FUNCTION_CALL`, and the surface keeps its
theme. The new theme replaces `at_exponential_theme` for the rest of the
surface's life. eUI implements it in #VAPP-304. Until then, compiling a live
change against a catalog without `setTheme` is a validation error that says to
write `theme(...)` before `root`.

**Decompile** writes a theme back as `theme(...)` seeds when some seeds expand
to exactly that theme (`theme::recover`): it reads the seeds from one mode,
drops the ones the theme does not need, and adds overrides for what the seeds
do not reach. It only returns seeds that expand back to the same JSON, so the
round trip is exact. Any other theme stays in `surface(extensions=…)`.

## `#rrggbb` / `#rrggbbaa` literals

Six or eight hex digits after `#`, then a character that cannot continue a word.
They are read as colours (`ExprKind::Color`, lowercased) only in the
extension's dialect (`syntax::Dialect::EXTENDED`). Every scanner (the source
repairs, the lexer, the stream's statement splitter) asks the same question
(`syntax::lexer::hex_color_at`). Anything else after `#` is still a comment. A
colour literal outside `theme(...)` is a validation error: a style colour is a
`$color.*` token.

## Style shorthand: `s=` and `style(name, "…")`

```
style(card, "col g12 p16 bg-card r-lg sh-sm")
root = Column([a, b], s="col g16 p24")
a = Card(t, s=card)
b = Card(u, s="card hover:bg-accent")
```

`s=` takes words over eUI's Box style whitelist (`catalog/style.json`), which
the compiler expands into the plain `style` map. It works on any component that
does not declare an `s` property itself. A word is:

- a bare word (`col`, `flex`, `wrap`, `relative`, `solid`, `uppercase`,
  `grow` = `flexGrow: 1`, `shrink` = `flexShrink: 1`);
- a prefix (the word's leading letters) and a value. The value is a number
  (`p16`, `op0.5`, `mt-8`); for lengths `N%` / `Npx` (the strings the whitelist
  takes) or `-auto`; `-<name>` for a token in the key's default group
  (`bg-card` = `$color.card`), or `-<group>.<name>` for another group
  (`h-control.input`); `-<value>` for an enum (`justify-between`);
- a state prefix: `hover:`, `focus:` (`:focus-visible`), `pressed:`;
- a class name from `style(name, "…")`. A class is defined once, before its
  first use (so a stream reads it the same way), and its name cannot be a word.

Later words win. A `style={…}` map beside `s=` wins over the words and holds
what they cannot say: gradients, `backgroundImage`, transforms, grid templates,
`@media` blocks.

**Canonical decompile**: every entry the shorthand can spell becomes the one
word that reads back as exactly that entry, in table order. The rest stays a
map. Strings of three or more words used twice, or of two words used three
times, become classes `k1`, `k2`, … in order of first use. Children the
compiler hoisted out of a parent (ids `<parent>_<prop>[_<i>]`, named nowhere
else) are written inline again. All of it is decided by the messages alone, so
`decompile(compile(x))` is a fixed point. `style::table_json()` (FFI
`style_table`) exports the table for the grammar generators.

| key | words | default token group |
|---|---|---|
| `display` | `flex`, `grid`, `block`, `d-none` | — |
| `flexDirection` | `row`, `col`, `fd-row-reverse`, `fd-col-reverse` | — |
| `flexWrap` | `wrap`, `nowrap`, `wrap-reverse` | — |
| `justifyContent` | `justify-flex-start`, `justify-flex-end`, `justify-start`, `justify-end`, `justify-center`, `justify-stretch`, `justify-between`, `justify-around`, `justify-evenly` | — |
| `alignItems` | `items-flex-start`, `items-flex-end`, `items-start`, `items-end`, `items-center`, `items-stretch`, `items-baseline` | — |
| `alignContent` | `content-flex-start`, `content-flex-end`, `content-start`, `content-end`, `content-center`, `content-stretch`, `content-between`, `content-around`, `content-evenly` | — |
| `alignSelf` | `self-flex-start`, `self-flex-end`, `self-start`, `self-end`, `self-center`, `self-stretch`, `self-baseline` | — |
| `justifySelf` | `jself-flex-start`, `jself-flex-end`, `jself-start`, `jself-end`, `jself-center`, `jself-stretch`, `jself-baseline` | — |
| `flexGrow` | `growN`, `grow-<name>` | — |
| `flexShrink` | `shrinkN`, `shrink-<name>` | — |
| `flexBasis` | `basisN`, `basisN%`, `basisNpx`, `basis-auto`, `basis-<name>` | `spacing` |
| `gap` | `gN`, `gN%`, `gNpx`, `g-auto`, `g-<name>` | `spacing` |
| `rowGap` | `gyN`, `gyN%`, `gyNpx`, `gy-auto`, `gy-<name>` | `spacing` |
| `columnGap` | `gxN`, `gxN%`, `gxNpx`, `gx-auto`, `gx-<name>` | `spacing` |
| `direction` | `dir-ltr`, `dir-rtl` | — |
| `overflow` | `overflow-visible`, `overflow-hidden`, `overflow-clip`, `overflow-scroll`, `overflow-auto` | — |
| `overflowX` | `overflowx-visible`, `overflowx-hidden`, `overflowx-clip`, `overflowx-scroll`, `overflowx-auto` | — |
| `overflowY` | `overflowy-visible`, `overflowy-hidden`, `overflowy-clip`, `overflowy-scroll`, `overflowy-auto` | — |
| `width` | `wN`, `wN%`, `wNpx`, `w-auto`, `w-<name>` | `spacing` |
| `height` | `hN`, `hN%`, `hNpx`, `h-auto`, `h-<name>` | `spacing` |
| `minWidth` | `minwN`, `minwN%`, `minwNpx`, `minw-auto`, `minw-<name>` | `spacing` |
| `minHeight` | `minhN`, `minhN%`, `minhNpx`, `minh-auto`, `minh-<name>` | `spacing` |
| `maxWidth` | `maxwN`, `maxwN%`, `maxwNpx`, `maxw-auto`, `maxw-<name>` | `spacing` |
| `maxHeight` | `maxhN`, `maxhN%`, `maxhNpx`, `maxh-auto`, `maxh-<name>` | `spacing` |
| `aspectRatio` | `aspectN`, `aspect-<name>` | — |
| `position` | `relative`, `absolute`, `sticky` | — |
| `top` | `topN`, `topN%`, `topNpx`, `top-auto`, `top-<name>` | `spacing` |
| `right` | `rightN`, `rightN%`, `rightNpx`, `right-auto`, `right-<name>` | `spacing` |
| `bottom` | `bottomN`, `bottomN%`, `bottomNpx`, `bottom-auto`, `bottom-<name>` | `spacing` |
| `left` | `leftN`, `leftN%`, `leftNpx`, `left-auto`, `left-<name>` | `spacing` |
| `inset` | `insetN`, `insetN%`, `insetNpx`, `inset-auto`, `inset-<name>` | `spacing` |
| `insetInlineStart` | `insetsN`, `insetsN%`, `insetsNpx`, `insets-auto`, `insets-<name>` | `spacing` |
| `insetInlineEnd` | `inseteN`, `inseteN%`, `inseteNpx`, `insete-auto`, `insete-<name>` | `spacing` |
| `insetBlockStart` | `insettN`, `insettN%`, `insettNpx`, `insett-auto`, `insett-<name>` | `spacing` |
| `insetBlockEnd` | `insetbN`, `insetbN%`, `insetbNpx`, `insetb-auto`, `insetb-<name>` | `spacing` |
| `padding` | `pN`, `pN%`, `pNpx`, `p-auto`, `p-<name>` | `spacing` |
| `paddingTop` | `ptN`, `ptN%`, `ptNpx`, `pt-auto`, `pt-<name>` | `spacing` |
| `paddingRight` | `prN`, `prN%`, `prNpx`, `pr-auto`, `pr-<name>` | `spacing` |
| `paddingBottom` | `pbN`, `pbN%`, `pbNpx`, `pb-auto`, `pb-<name>` | `spacing` |
| `paddingLeft` | `plN`, `plN%`, `plNpx`, `pl-auto`, `pl-<name>` | `spacing` |
| `paddingHorizontal` | `pxN`, `pxN%`, `pxNpx`, `px-auto`, `px-<name>` | `spacing` |
| `paddingVertical` | `pyN`, `pyN%`, `pyNpx`, `py-auto`, `py-<name>` | `spacing` |
| `paddingInlineStart` | `psN`, `psN%`, `psNpx`, `ps-auto`, `ps-<name>` | `spacing` |
| `paddingInlineEnd` | `peN`, `peN%`, `peNpx`, `pe-auto`, `pe-<name>` | `spacing` |
| `margin` | `mN`, `mN%`, `mNpx`, `m-auto`, `m-<name>` | `spacing` |
| `marginTop` | `mtN`, `mtN%`, `mtNpx`, `mt-auto`, `mt-<name>` | `spacing` |
| `marginRight` | `mrN`, `mrN%`, `mrNpx`, `mr-auto`, `mr-<name>` | `spacing` |
| `marginBottom` | `mbN`, `mbN%`, `mbNpx`, `mb-auto`, `mb-<name>` | `spacing` |
| `marginLeft` | `mlN`, `mlN%`, `mlNpx`, `ml-auto`, `ml-<name>` | `spacing` |
| `marginHorizontal` | `mxN`, `mxN%`, `mxNpx`, `mx-auto`, `mx-<name>` | `spacing` |
| `marginVertical` | `myN`, `myN%`, `myNpx`, `my-auto`, `my-<name>` | `spacing` |
| `marginInlineStart` | `msN`, `msN%`, `msNpx`, `ms-auto`, `ms-<name>` | `spacing` |
| `marginInlineEnd` | `meN`, `meN%`, `meNpx`, `me-auto`, `me-<name>` | `spacing` |
| `backgroundColor` | `bg-<name>` | `color` |
| `color` | `fg-<name>` | `color` |
| `borderWidth` | `bN`, `b-<name>` | `border` |
| `borderTopWidth` | `btN`, `bt-<name>` | `border` |
| `borderRightWidth` | `brN`, `br-<name>` | `border` |
| `borderBottomWidth` | `bbN`, `bb-<name>` | `border` |
| `borderLeftWidth` | `blN`, `bl-<name>` | `border` |
| `borderColor` | `bc-<name>` | `color` |
| `borderStyle` | `solid`, `dashed`, `dotted` | — |
| `borderRadius` | `rN`, `rN%`, `rNpx`, `r-auto`, `r-<name>` | `radius` |
| `borderTopLeftRadius` | `rtlN`, `rtlN%`, `rtlNpx`, `rtl-auto`, `rtl-<name>` | `radius` |
| `borderTopRightRadius` | `rtrN`, `rtrN%`, `rtrNpx`, `rtr-auto`, `rtr-<name>` | `radius` |
| `borderBottomRightRadius` | `rbrN`, `rbrN%`, `rbrNpx`, `rbr-auto`, `rbr-<name>` | `radius` |
| `borderBottomLeftRadius` | `rblN`, `rblN%`, `rblNpx`, `rbl-auto`, `rbl-<name>` | `radius` |
| `opacity` | `opN`, `op-<name>` | `opacity` |
| `boxShadow` | `sh-<name>` | `shadow` |
| `fontSize` | `fsN`, `fsN%`, `fsNpx`, `fs-auto`, `fs-<name>` | `type.size` |
| `fontWeight` | `fwN`, `fw-<name>` | `type.weight` |
| `lineHeight` | `lhN`, `lhN%`, `lhNpx`, `lh-auto`, `lh-<name>` | `type.lineHeight` |
| `letterSpacing` | `lsN`, `lsN%`, `lsNpx`, `ls-auto`, `ls-<name>` | — |
| `textAlign` | `text-left`, `text-right`, `text-center`, `text-start`, `text-end`, `text-justify` | — |
| `textDecoration` | `no-underline`, `underline`, `line-through` | — |
| `textTransform` | `normal-case`, `uppercase`, `lowercase`, `capitalize` | — |
| `fontStyle` | `not-italic`, `italic` | — |
| `fontVariantNumeric` | `tabular-nums` | — |
| `fontFamily` | `ff-<name>` | `type.family` |
| `transition` | `tr-<name>` | `motion` |
| `transitionEasing` | `ease-<name>` | `ease` |
| `visibility` | `visible`, `invisible` | — |
| `pointerEvents` | `events-auto`, `events-none` | — |
| `userSelect` | `select-auto`, `select-none`, `select-text` | — |
| `cursor` | `cursor-auto`, `cursor-default`, `cursor-pointer`, `cursor-text`, `cursor-not-allowed`, `cursor-grab`, `cursor-grabbing`, `cursor-move`, `cursor-col-resize`, `cursor-row-resize` | — |
| `backdropBlur` | `blur-<name>` | `blur` |
| `tail` | `tail-none`, `tail-start`, `tail-end`, `tail-down` | — |
| `animationDuration` | `dur-<name>` | `motion` |

## Motion

| sugar | style key |
|---|---|
| `enter="slide-in-up"` | `animation`: `pulse`, `spin`, `fade-in`, `slide-in-up`, `slide-in-down`, `slide-in-left`, `slide-in-right`, `shimmer` |
| `exit="fade-out"` | `exitAnimation`: `fade-out`, `slide-out-up`, `slide-out-down`, `slide-out-left`, `slide-out-right` |
| `stagger=60` | `animationStagger` (ms, on a container: child or template item i starts i × N ms late) |
| `delay=120` | `animationDelay` (ms, added to the stagger offset) |

The eUI catalogue batch for genUI round 4 (session 118e5768) agreed these names
on 2026-11-26; they land in eUI's style whitelist in a later renderer PR.

`Motion(ref="pulse-dot")` expands to the registry entry's props; a prop
written on the call wins (`Motion(ref="pulse-dot", duration=2400)`). The
decompiler writes `ref=` for a Motion whose `layers` equal an entry's (the
entry's differing props stay written).

`Motion(kf="…")` is a compact keyframe form compiled into the eui-motion
dialect's `layers` (`apps/desktop/crates/eui-motion`,
`packages/exponential-ui/eval/motion/guide.md`):

```
circle CX CY R COLOR | ring CX CY R WIDTH COLOR | rect CX CY W H RADIUS COLOR | star CX CY POINTS OUTER INNER COLOR
| scale T:V … [EASE] | opacity T:V … | rotate T:V … | move T:DX,DY …
```

Layers are split by `;`. `T` is in ms, `COLOR` is `$color.<token>` or
`#rrggbb[aa]`, and `EASE` is `linear`, `ease`, `ease-in`, `ease-out`,
`ease-in-out` or `hold`, applied to every segment. The decompiler writes `kf=`
for layers that are exactly what some keyframe text compiles to.

## Patches: `compile_patch(screen, patch)`

`screen` is the Express of a surface as it stands. `patch` holds:

- `name = …`: a reassignment, which replaces the statement in place, or a new
  variable;
- `$/path = …`;
- `style(name, "…")`;
- `theme(...)`;
- `del(name)`: removes the component and its entry in every child list that
  names it (naming it anywhere else is an error);
- `del($/path)`.

The result is `{messages, screen}`. The messages are the difference between
compiling the old screen and the new one: `setTheme` when the theme changed,
one `updateComponents` with exactly the new and changed components (untouched
ones are never re-sent), and one `updateDataModel` per changed data path (a
removed one as `null`). A2UI 1.0 has no message that deletes a component; one
that `root` no longer reaches is simply not drawn.
`tests/extensions.rs::patches_apply_like_the_new_screen_compiles` checks
`apply(patch, screen) == compile(new screen)` over random patches of eUI's
sample surfaces. It is in Rust (`Compiler::compile_patch`), FFI, wasm
(`compilePatch`) and Python (`a2ui_express.compile_patch`,
`Catalogs.compile_patch`).

## The prompt

With the extension declared, `generate_prompt` adds an "Express Extensions"
section (`ext::prompt_section`), built from the same tables.

## Measurements

Model-written screens from genUI round 4 (genui-bench round-4 `carbon` outputs, 175
screens that compiled; Qwen3.5 tokens, medians; canonical = plain top-down
`decompile`, sugared = with the extension):

| set | n | model's Express | canonical | sugared | sugared / canonical | compress.py estimate (+classes) |
|---|---|---|---|---|---|---|
| sy (synthetic, dense) | 58 | 6107 | 6380 | 2835 | 0.47 | 2926 |
| ws (websight) | 117 | 1009 | 764 | 564 | 0.74 | 590 |

The sugared text compiles back to the same messages for 173 of the 175. The
other two are stored messages from an older catalog, which the plain decompile
does not round-trip either. On eUI's own sample surfaces, which are mostly data,
the saving is 2–7 % (`tests/extensions.rs::the_sugar_is_shorter`).

Reproduce: `cargo run -q -p a2ui-express --example express -- sugar <catalog.json> < message-lists.json`
prints `{plain, sugared}` per message list.
