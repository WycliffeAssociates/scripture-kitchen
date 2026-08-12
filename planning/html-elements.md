# `html_element`: draft inventory for the audit sitting

**Status: RULED, IMPLEMENTED, and CLOSED (round 6 / [C] + rounds 8–9,
2026-08-10). Nothing open.** Q-H1–Q-H7 closed; Q-H8 (`sd#`) dissolved by round 9. `HtmlElement` is
inhabited, every one of the 154 rows carries a class, and
`schema::HEADING_BASE_LEVEL` + `schema::heading_level` exist. This file is now the
record of WHY, plus the short list of what the rulings left open (§6).

What was ruled:

- **Q-H7 — the column stays in the main table.** The row is a u128 either way, so
  co-location is free and the audit sees one place. The prior-question worry (that
  this is the one column that is policy, not spec) is answered by the export
  convention below rather than by moving the column out.
- **Budget widened u4 → u5, 32 slots**, and **`Ruby` added** as a class.
- **Milestones render as a SELF-CLOSING span** with their data attributes.
- **`b` = `Span`**, not `Transparent` — we are not shipping styles, and a styled
  break would be less consistent than a plain span. The same logic carries `\pb`.
- **Q-H1 = option (b)**: a per-family heading base-level auxiliary side table,
  the same precedent as the scope-kind precedence table. Built.
- **Round 8 — Q-H2**: semantic elements where HTML has a twin (`bd`→`<b>`,
  `it`→`<i>`, `sup`→`<sup>`); `sc` and `no` keep Span-with-data-attributes because
  there is no smallcaps or "normal text" element. **Q-H4**: list/table containers
  are scope-derived, accepted as convention. **Q-H5**: two render surfaces, no
  special case.
- **Round 9**: `em`→`<em>` (its semantic twin, consistent with `bd`→`<b>`), and
  **`sd#` is an empty spacer `<div>`** — not a heading, not `<hr>`. That dissolved
  the last open question and removed the only family needing a heading clamp.
- **Export convention: ALWAYS emit verbose data attributes** — `data-marker`,
  `data-category`, `data-usfm-type` — so consumers restyle via CSS, and a
  consumer-supplied marker↔element map can override the defaults wholesale later.
  This is what makes "the element is a default" true in practice.

Budget note, superseded: the NEXT-STEPS sketch said "4 bits, ≤16 element names".
It is now 5 bits / 32 slots, 17 used.

Two framing points before the tables.

**This is new design, not a port.** Onion's HTML export declines element
semantics almost entirely. Its whole inventory is `div`, `span`, `table`, `tr`,
`td`, `figure`, `section`, `a` — verified by reading `src/html.rs`
`tag_and_type_for_marker` (~line 1095) — with meaning carried in
`data-usfm-type="para|char|ms|note|sidebar|periph|table:row|table:cell|figure|book"`
attributes instead. There is **no `p`, no `h1`–`h6`, no `li`, no `ul`/`ol`, no
`sup`, no `aside`** anywhere in it, and a `prefer_native_elements` flag whose
entire effect is upgrading `div`→`figure` for `\fig` and `div`→`section` for
`\id`. So onion is *evidence about what was sufficient*, not a source of element
choices — and the inventory sketched in NEXT-STEPS is considerably more ambitious
than anything that has shipped.

**Do not conflate this with USJ.** USJ's type names are a different, official,
fixed vocabulary — onion's `usj/mod.rs` carries `book`, `chapter`, `verse`,
`para`, `char`, `ref`, `note`, `ms`, `figure`, `sidebar`, `periph`, `table`,
`optbreak` (13). That projection is settled by the USJ schema and is not this
column's business. NEXT-STEPS already flags checking it against usfm-grammar.

---

## 1. Inventory (21 used, 11 spare — u5)

4 bits = 16 slots. Deliberately leaving one free rather than filling it.

| # | class | element(s) emitted | who it serves |
|---|---|---|---|
| 0 | `Transparent` | *none* — children flow into the parent | `\id`, `\usfm`, `\cat`, `\pb`, `\b`, milestones, `\ide`/`\h`/`\toc#` |
| 1 | `Para` | `<p>` | body, poetry, introduction, peripheral paragraphs |
| 2 | `Heading` | `<h1>`…`<h6>` | `\mt#`, `\ms#`, `\s#`, `\cl`, `\qa`, `\c` |
| 3 | `Span` | `<span>` | every ordinary character marker |
| 4 | `ListItem` | `<li>` | `\li#`, `\lim#` |
| 5 | `ListContainer` | `<ul>` | synthesized around a run of list items |
| 6 | `Table` | `<table>` | synthesized around a run of `\tr` |
| 7 | `TableRow` | `<tr>` | `\tr` |
| 8 | `TableCell` | `<td>` / `<th>` | `\tc#`, `\tcr#`, `\tcc#` / `\th#`, `\thr#`, `\thc#` |
| 9 | `Aside` | `<aside>` | `\esb`, `\f`, `\x` |
| 10 | `Sup` | `<sup>` | `\v`, `\va`, `\vp`, note callers, `\sup` |
| 11 | `Anchor` | `<a>` | `\jmp`, `\ref`, `\xt` |
| 12 | `Figure` | `<figure>` + `<figcaption>` | `\fig` |
| 13 | `Image` | `<img>` | `\fig`'s `src` attribute, inside the figure |
| 14 | `Section` | `<section>` | `\periph`, and the book/chapter containers |
| 16 | `Ruby` | `<ruby>` + `<rt>` | `\rb` — added by ruling [C] i |
| 17 | `Bold` | `<b>` | `\bd`, and the outer element of `\bdit` (round 8) |
| 18 | `Italic` | `<i>` | `\it` (round 8) |
| 19 | `Em` | `<em>` | `\em` (round 9) |
| 20 | `Div` | an EMPTY `<div>` — a vertical spacer | `\sd#` (round 9) |
| — | | *11 slots spare* | u5 gives 32 |

Notes on three of these:

- **`TableCell` covers both `<td>` and `<th>`** without a second slot: which one
  is derivable from the marker name (`th*` vs `tc*`), which export already has.
  Same trick as `-s`/`-e`: don't spend a column on something the span says.
- **`ListContainer` and `Table` are SYNTHESIZED**, not mapped from any marker.
  USFM has no list-open marker (`\li` items just occur; `\lh`/`\lf` bracket them
  loosely) and `\tr` rows have no enclosing marker. Export must group runs. The
  3.2 `list` and `table` MILESTONES exist for exactly this and would remove the
  guessing where present — worth deciding whether export prefers them.
- **`Ruby` is in** (ruling [C] i, with the widened budget), and
  **`SelfClosingSpan`** joins it for milestones (ruling [C] ii) — a milestone marks
  a point, so there is no content to wrap.

---

## 1a. The three render surfaces (recorded — Q-H4 + Q-H5)

Rendering is **not** "this column plus special cases". There are three surfaces,
and keeping them apart is what closed both questions:

| surface | keyed on | examples |
|---|---|---|
| **marker-keyed** | canonical marker → this column | `\p`→`<p>`, `\bd`→`<b>`, `\f`→`<aside>` |
| **kind-keyed** | `TokenKind` → its own render rule | `Newline`, `OptBreak`, and **`NoteCaller`→`<sup>`** |
| **scope-derived** | a walker SCOPE → a synthesized element | `<ul>` around a run of `\li#`, `<table>` around `\tr`, and every closing tag |

- **Q-H5 closed by reframing.** A footnote's `<sup>` caller is not an exception to
  the column: `\f` maps to `Aside` (the note BODY container) and the `<sup>`
  belongs to the **NoteCaller token kind**, which renders by kind exactly as
  `Newline` and `OptBreak` already do. Two different things rendering two ways.
- **Q-H4 closed as convention.** `ListContainer` and `Table` correspond to walker
  SCOPES, not to markers — USFM has no marker that opens either. Export
  synthesizes them around a scope, which is the same reason **closing tags are not
  a column**. That they are unreachable from any row is correct, not a gap.

## 2. Category-level defaults

The point of the `kind` × `category` restructure was that behaviour keys off the
fine category. This column should too — so the DEFAULT is per category and only
genuine deviations get a row in §3.

| category | default class | reasoning |
|---|---|---|
| `ParaIdentification` | `Transparent` | `\ide`, `\h`, `\toc#`, `\sts`, `\rem` are metadata, not rendered body |
| `ParaIntroductions` | `Para` | |
| `ParaTitlesSections` | `Heading` | level from the span's digits — see §4, this is the hard part |
| `ParaBody` | `Para` | |
| `ParaPoetry` | `Para` | indent level is a CSS class, not a different element |
| `ParaLists` | `ListItem` | |
| `ParaPeripheral` | `Para` | |
| `ParaTables` | `TableRow` | `\tr` only |
| `CharTextFeatures` | `Span` | |
| `CharFormatting` | `Span` | fallback for members with no semantic twin (`sc`, `no`); `bd`/`it`/`sup` override — round 8 / Q-H2 |
| `CharBreaks` | `Span` | `\pb` — ruled [C] iii, same reasoning as `b` |
| `CharIntroductions` | `Span` | |
| `CharPoetry` | `Span` | |
| `CharLists` | `Span` | |
| `CharTables` | `TableCell` | |
| `CharNotes` | `Span` | note-internal markers render inside the note body |
| `NoteFootnote` | `Aside` | |
| `NoteCrossReference` | `Aside` | |
| `MilestoneList` / `MilestoneTable` / `MilestoneQt` / `MilestoneTs` / `MilestoneVid` | `SelfClosingSpan` | ruled [C] ii |
| `ChapterVerse` | `Sup` | `\v`, `\va`, `\vp` — but see `\c`/`\cp` in §3 |
| `Sidebar` | `Aside` | |
| `Meta` | `Transparent` | `\cat` |
| `Peripheral` | `Section` | |
| `DocumentStructure` | `Transparent` | `\id`, `\usfm` |
| `Figure` | `Figure` | |

**Round 8 settled the choice that was flagged here.** The draft used `Span` for
all of `CharFormatting`, arguing that `<b>`/`<i>` bake in a presentation decision
a publisher overrides anyway. Ruled the other way *where a twin exists*: `bd`→`<b>`,
`it`→`<i>`, `sup`→`<sup>`. `sc` (smallcaps) and `no` ("normal text") have no
semantic element, so they keep `Span` plus data attributes — which is what the
always-emit convention is for.

## 3. Per-marker overrides (only where the category default is wrong)

Eyeball-sized on purpose: 16 rows out of 154.

| marker | category default | override | why |
|---|---|---|---|
| `c` | `Sup` | `Heading` | a chapter number is a heading, not a superscript — the one place `ChapterVerse` splits |
| `cp` | `Sup` | `Heading` | published chapter number displaces the chapter heading |
| `b` | `Para` | `Span` | ruled [C] iii — plain span; not shipping styles, and a styled break would be less consistent |
| `d` | `Heading` | `Para` | Hebrew psalm descriptive title — conventionally italic prose, not a heading |
| `sp` | `Heading` | `Para` | speaker attribution, prose |
| `cd` | `Heading` | `Para` | chapter description is a block of prose |
| `r` | `Heading` | `Para` | parallel-passage reference line |
| `mr` | `Heading` | `Para` | as `r` |
| `sr` | `Heading` | `Para` | as `r` |
| `qa` | `Para` | `Heading` | acrostic heading — the one Poetry row that IS a heading |
| `lh` | `ListItem` | `Para` | list header sits outside the `<ul>` |
| `lf` | `ListItem` | `Para` | list footer, ditto |
| `rb` | `Span` | `Ruby` | ruby annotation genuinely needs `<ruby>`/`<rt>` |
| `sup` | `Span` | `Sup` | it is literally superscript |
| `jmp` | `Span` | `Anchor` | |
| `ref` | `Span` | `Anchor` | |
| `xt` | `Span` | `Anchor` | but 3.2 deprecates `\xt` outside a cross-reference — see the flag |
| `bd` | `Span` | `Bold` | round 8 / Q-H2 — `<b>` is the semantic twin |
| `it` | `Span` | `Italic` | round 8 / Q-H2 — `<i>` |
| `bdit` | `Span` | `Bold` | round 8 — `<b>` outer, `<i>` templated inside (below) |
| `em` | `Span` | `Em` | round 9 — `<em>`; see the recategorisation note below |
| `sd` | `Heading` | `Div` | round 9 — an empty spacer, §6.1 |
| `sup` | `Span` | `Sup` | an override before round 8, confirmed by it |
| `imt` | `Para` | `Heading` | an introduction major title IS a heading; base 1 |
| `imte` | `Para` | `Heading` | base 1 |
| `is` | `Para` | `Heading` | introduction section heading; base 3 |
| `iot` | `Para` | `Heading` | introduction outline title; base 3 |

**`bdit` — the choice made, and why.** One marker wanting `<b><i>`. Options: (a)
name the OUTER element and template the interior, (b) a dedicated `BoldItalic`
class, (c) `Bold` plus a CSS class carrying the italic. **Chose (a)** — it reuses a
convention *already ruled* for `\fig` under Q-H3 (`<figure>` names the outer
element; `<img>`/`<figcaption>` are a fixed interior template), so it adds no new
mechanism and keeps the invariant **one marker = one class**. (b) was defensible
with 13 slots spare, but it spends a slot to avoid reusing an existing convention
and would make `bdit` the only class encoding a *combination* rather than an
element.

**`em` — a category correction made while executing this ruling.** 3.2's
`char/index.html` lists `em` ("Emphasis text") under **Text Features**, and gives
"Text Formatting" as exactly `bd`/`it`/`bdit`/`no`/`sc`/`sup`. Our row had `em` in
`CharFormatting`; it is now `CharTextFeatures` on that citation, so its element is
that category's default, `Span`. Round 9 then gave it `<em>` — its semantic
twin, consistent with `bd`→`<b>`. The recategorisation stays on the flag list not
as a question but as a record: the wrong category was a misreading on our side,
not a spec ambiguity.

Those last four are an addition to the original draft: `ParaIntroductions`
defaults to `Para`, which is right for `\ip`/`\im`/`\iq` but wrong for the four
introduction markers that are genuinely headings.

`\fig` needs both `Figure` and `Image` (a `<figure>` wrapping an `<img>` plus a
`<figcaption>` from the caption text). That is one marker producing two elements,
which no single-value column can express — noted in §5.

---

## 4. The hard part: heading level from digits

The NEXT-STEPS scheme is "numbered markers store the element CLASS, export
computes the level from the span's number". For headings that scheme is
**underspecified**, and this is the crux of the whole column:

`mt1`, `ms1`, and `s1` all have digit `1`, and they are not the same heading
level. A book main title outranks a major-section heading, which outranks a
section heading. So `class + digit` is not enough — the mapping needs a **base
level per family**:

| family | digits | plausible levels |
|---|---|---|
| `mt#` | 1–4 | `h1`–`h4` |
| `mte#` | 1–2 | `h1`–`h2` |
| `ms#` | 1–3 | `h2`–`h4` |
| `s#` | 1–4 | `h3`–`h6` |
| ~~`sd#`~~ | — | **not a heading** — an empty spacer `<div>` (round 9) |
| `cl` | — | `h2` |
| `c` | — | `h2` |
| `qa` | — | `h4` |

**RULED: option (b)** — `schema::HEADING_BASE_LEVEL`, an auxiliary side table
keyed on the heading family, with `schema::heading_level(marker, digit)` computing
`base + digit - 1`.

**No clamp any more.** Round 9 removed `sd#` from the heading set, and it was the
only family that overflowed — with `s4`→`<h6>` the deepest legal level, every
remaining family fits exactly. So `heading_level` does not clamp and an
out-of-range base fails loudly. Round 10 then **deleted the test** that asserted
the range: 12 hand-written rows whose breakage an export would show immediately
did not earn an assertion. The table stays; the test does not.

The three options as considered:

- **(a) Spend more bits.** A 3-bit base level alongside the 4-bit class. The
  NEXT-STEPS note says ~50 spare bits exist, so it fits — but it makes the column
  a pair, and "html_element" stops being one value.
- **(b) A small side table** keyed on the heading families only (~8 rows above).
  Consistent with how the precedence data was handled ([A]: kind-keyed data goes
  in an auxiliary table, not on marker rows). Probably the right shape.
- **(c) Emit flat `h1` everywhere** and let a CSS class carry the distinction.
  Cheapest, and closest to onion's actual behaviour (`div` + `data-usfm-type`),
  but it throws away document outline structure that HTML has a mechanism for.

Chosen for the precedent: kind-keyed and family-keyed data belongs beside the
marker table, not inside it — the same call made for the scope-kind precedence
table.

---

## 5. Status after the rulings

**All closed.** Q-H8 (`sd#` — dissolved by round 9: an empty spacer `<div>`),
Q-H1 (heading side table, built), Q-H2 (semantic elements where a
twin exists — round 8), Q-H3 (outer element named, interior templated), Q-H4
(scope-derived containers — round 8), Q-H5 (two render surfaces — round 8), Q-H6
(`Transparent` stays one class: `\pb` and `\b` are Spans under [C] iii, so only
genuinely-skipped markers remain on it), Q-H7 (the column stays in the main table).

**Open: none.**

## 6. Anomalies

### 6.1 `sd#` — RESOLVED (round 9), was Q-H8

Filed as `Heading` through round 8 and flagged as the one genuinely unsettled
case, on two compounding problems: it might not be a heading at all, and no base
level fitted (it sits below `s`, but `s4` already reaches `<h6>` and HTML has no
`<h7>`, so it clamped and `sd3`/`sd4` both rendered `<h6>`).

**Ruled from the spec's own layout example**: `\sd#` renders as vertical BLANK
SPACE between text blocks — a division *spacer*, not a title. So it is neither a
heading nor an `<hr>`: it is an **empty `<div>`**.

Both problems dissolve rather than trade off. No level is needed, so the hierarchy
conflict disappears; the base-level entry and the clamp are gone; and **no
distinction is lost**, because the digit rides in `data-marker` like every other
marker's, per the always-emit convention. The clearest case in this file of the
right answer making the awkwardness vanish instead of balancing it.

### 6.2 Closed by convention (kept as the record)

- **`\fig`** — `<figure>` + `<img>` + `<figcaption>`: one marker, three elements.
  Outer element named, interior templated (Q-H3). `\bdit` uses the same convention
  (`<b>` outer, `<i>` inside).
- **`\f` / `\x`** — the `<sup>` caller belongs to the `NoteCaller` token kind, not
  to the row; the row is the `<aside>` body container (Q-H5, §1a).
- **`\pb` / `\b`** — plain spans by ruling [C] iii; we are not shipping styles, and
  a styled break would be less consistent than a span carrying data attributes.
