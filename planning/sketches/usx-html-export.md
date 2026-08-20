# USX + HTML export sketch (roadmap item 2b — NOT APPROVED)

Written 2026-08-20. Companion to sketches/usj-export.md — read that
first: the fold skeleton (`Visit` trait over the CST), the mapping
table, the content→attribute lift table, the attribute splatting, and
the whitespace policy are ALL SHARED; this file records only what USX
and HTML each add. Probes: scratchpad `usx_shapes.py` (usfmtc USX with
and without `addesids()`).

## USX

```rust
pub fn usx(source: &[u8], tokens: &[Token], cst: &Cst) -> String
```

Hand-rolled XML writer (five-entity escaper; attribute + text
contexts). Same no-serde rationale as USJ.

### Element shapes (observed)

USJ's mapping table transposes almost 1:1 — `type` becomes the element
name, `marker` becomes `style`:

| USJ | USX |
|---|---|
| `{type:"para", marker:"p"}` | `<para style="p">` |
| char / note(+caller) / ms / sidebar / figure / book(+code) | `<char style>` `<note style caller>` `<ms style>` `<sidebar style category>` `<figure style … file>` `<book style="id" code>` |
| `{type:"table"}` (synthesized) | `<table>` (no style), `<row style="tr">`, `<cell style="tc1" align="start">` |
| chapter / verse | `<chapter style="c" number sid>` + EID MILESTONES (below) |
| optbreak | `<optbreak/>` |
| unknown marker | `<ms style="s5" x-bare="true"/>` (same open question as USJ) |

Root: `<usx version="3.0">` — the USX schema version, a const;
`\usfm`'s declared version is dropped here too (observed).

### The eid derivation rules (the one genuinely new mechanism)

usfmtc by DEFAULT emits neither sids nor eids; its `addesids()` opt-in
produces the full Paratext decoration. PROPOSED: we emit sids + eids
ALWAYS — they are the useful half of USX (every downstream consumer
keys on them), and "usfmtc default" is just their round-trip
minimalism. The oracle compares against usfmtc WITH `addesids()`.

Observed placement (probe, two chapters × three verses):

```
<chapter style="c" number="1" sid="GEN 1"/>
<para style="p">
  <verse style="v" number="1" sid="GEN 1:1"/>one
  <verse eid="GEN 1:1"/><verse style="v" number="2" sid="GEN 1:2"/>two
  <verse eid="GEN 1:2"/></para>
<chapter eid="GEN 1"/>
<chapter style="c" number="2" sid="GEN 2"/> …
```

Rules, derived during iteration (never-synthesize governs TOKENS; an
eid is a projection artifact exactly like the synthesized `<table>`):

1. sid = `CODE C` / `CODE C:V` — book code off `LintReport`-style
   BookCode token, numbers off the designator INTERPRETER's `first`
   (the sketch's comparison rules are already vref's; this is their
   first export consumer). A malformed designator gets `number` =
   raw span and NO sid/eid — never repair, degrade the decoration.
2. An open VERSE closes (emit `<verse eid>`) at whichever comes
   first: the next `\v`, the close of the paragraph that contains its
   marker, or the chapter close.
3. An open CHAPTER closes (emit `<chapter eid/>`) AFTER the close of
   its last block, i.e. immediately before the next `\c` element or
   at document end.
4. State is two Option slots in the visitor (open verse, open
   chapter) — the same shape as ordering lint's, minus the findings.

### What we mirror vs skip (vs usfmtc)

- Mirror: element/style names, attribute splatting, content→attribute
  lifts, x-bare unknowns (pending the open question), `src`→`file`.
- Skip: usfmtc's repair behaviors (they nest a displaced verse INSIDE
  an unclosed note; our tree closed it Recovery at the boundary — the
  oracle excludes Recovery books, same as USJ).
- Add: always-on sids/eids (their opt-in).

### USX tests (plain english)

- The zoo transposed from USJ (same fixtures, expected-XML strings).
- Eid placement: mid-para verse succession; verse ending at para
  close; chapter eid after the last para; book with front matter only
  (no eids at all); malformed designator degrades sid.
- XML escaping: `&<>"'` in text and in attribute values (bdf_reg
  quotes).
- The oracle script grows a `--usx` mode (same normalization, XML
  parsed with python's ElementTree on the usfmtc side).

## HTML

```rust
pub fn html(source: &[u8], tokens: &[Token], cst: &Cst) -> String
```

**Scope ruling to record: HTML is a VIEW export.** No round-trip
claim, no oracle (there is no reference implementation) — snapshot
tests and validity smoke only. It exists for preview/publish surfaces,
not for interchange.

### The wrapper scheme (PROPOSED)

The rows already carry the one authored fact needed:
`MarkerRow::html_element` (Span/Div/Transparent/…— shipped since the
table landed, so far unread by any consumer; this is its consumer).

- Node → its row's `html_element`, `class="usfm-<marker>"`
  (`<span class="usfm-nd">`, `<div class="usfm-p">`). Transparent →
  children emit, no wrapper.
- Text/Newline per the shared whitespace policy; HTML-escaped.
- Attributes → `data-*` (`data-lemma="grace"`); the lift table targets
  land as `data-altnumber` etc. on the enclosing wrapper.
- chapter → `<h2 class="usfm-c" data-number>`? NO — chapter/verse are
  markers, not headings: `<span class="chapter-num">1</span>` /
  `<sup class="verse-num">1</sup>` (PROPOSED; app CSS decides display).
- Milestones → `<span class="usfm-qt-s" data-…></span>` (empty,
  addressable) — or dropped entirely (open question 7).
- Tables → real `<table><tr><td class="usfm-tc1">`; align → CSS class.
- Sidebar → `<aside class="usfm-esb">`; figure →
  `<figure><img src><figcaption>`.

### Headings: the authored base-levels aux table

The one genuinely authored HTML fact (settled-facts named it): which
para families are headings and where they sit. Shape:

```rust
// family, base <hN>, numbered spellings add (level - 1), cap h6
const HEADING_BASES: &[(&str, u8)] = &[
    ("mt", 1), ("imt", 1), ("ms", 2), ("s", 3), ("sd", 4), // …audit
];
```

`mt1`→`<h1>`, `s2`→`<h4>`. Families absent from the table are plain
divs. The full list is an authoring session against the 3.2 pages
when HTML gets built — the sketch only fixes the SHAPE.

### Kind-keyed NoteCaller rendering (settled-facts named this too)

| caller | rendering |
|---|---|
| `+` | auto: `<sup class="note-caller">` numbered per note KIND (f-family and x-family count separately) |
| `-` | no visible caller (`<sup hidden>` or omitted) |
| anything else | the literal span text as the caller |

Note body: `<span class="note usfm-f" role="note">` inline (the app
decides popover/footnote-section presentation; we do not paginate).

### HTML tests (plain english)

- Snapshot per zoo fixture (tiny hand-checked HTML strings).
- Escaping: `&<>` in text, quotes in data attributes.
- Heading table: mt1/s1/s3-over-cap → h1/h3/h6.
- Caller trio: `+` numbering resets per kind, `-` hidden, custom
  literal.
- Smoke: every corpus book renders without panic; output contains no
  unescaped `<` from content (grep-able invariant).

## Open questions
1-6. Inherit usj-export.md's (writer, x-bare, lift-table authority,
   whitespace, oracle home, sid policy — the last is answered here for
   USX: always-on).
7. Milestones in HTML: empty addressable spans or dropped?
8. `html_element` column audit: it predates the CST — confirm each
   row's value against what this scheme actually wants before build.
9. Heading base levels: author the full table now or at HTML build
   time? (Sketch says build time.)
