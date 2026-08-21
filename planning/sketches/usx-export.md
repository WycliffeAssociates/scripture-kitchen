# USX export sketch (roadmap item 2b)

STATE 2026-08-20: **USX BUILT** (src/usx.rs, feature `usx`, in `default`).
The oracle (tests/usx_corpus.rs) reads 195/195 of the claimable testData
fixtures; 13 of the 208 validated-pass cases with an `origin.xml` are
excluded, each with its one-line reason in the test and its story in
"Divergence record" below. Plateau trace: 137 → (BOM in the test reader)
190 → (sidebars are their own scope) 194 → (table `vid` read off the
row's first CELL, not its first child) 195. HTML lives in its own
doc: sketches/html-export.md (split 2026-08-21).

Shared with usj.rs: `src/export.rs` — the whitespace canonicalizer, the
trim helpers, `marker_name`, the span readers, and the note-peer sets
that the F3 graft keys on. NOT shared: the walker. Both folds weave their
writer through the driver (a JSON array's comma bookkeeping and an XML
element's rewind-to-self-closing are different state), so a shared driver
would be a trait with two implementors and no third fact in common —
settled-facts' net-deletion rule says wait. usj.rs's only edit was
deleting the moved helpers; its oracle stayed at 187/187 and its zoo at
25 green.

Written 2026-08-20. Companion to sketches/usj-export.md — read that
first: the fold skeleton (`Visit` trait over the CST), the mapping
table, the content→attribute lift table, the attribute splatting, and
the whitespace policy are ALL SHARED; this file records only what USX
adds. Probes: scratchpad `usx_shapes.py` (usfmtc USX with
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
| unknown marker | **CORRECTED**: `<para style="s5" />` — x-bare is DEAD here too (the oracle pivot to testData dissolved it); `\zms\*` is `<ms style="zms" />` by its `\*` spelling |
| `\ref`, `\periph` | **ADDED**: `<ref loc>` and `<periph alt id>` carry NO `style` — exactly the two USJ elements with no `marker`, plus the synthesized `<table>` |
| a para/table INSIDE an open verse | **ADDED**: `vid="<the verse's sid>"` (see below) |

Root: `<usx version="3.0">` — **CORRECTED 2026-08-20, fixture-forced**: NOT
a const. 205 of the 208 fixtures say "3.0", and the three that declare
`\usfm 3.1` say "3.1". So `\usfm` is not dropped the way USJ drops it: its
payload IS the root version, and "3.0" is the default for a document that
declares none.

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
2. **CORRECTED 2026-08-20, fixture-forced.** A verse does NOT close at
   the close of its own paragraph. It runs to the next `\v`, the next
   `\c`, or the end of the document; every BLOCK it crosses carries
   `vid="<its sid>"`; and the `eid` lands at the end of the LAST block
   it reaches. Three sub-corrections, each measured:
   - **Trailing headings and empty blocks are TRIMMED.** `\v 2 …\n\s1
     A heading\n\p \v 3 …` closes verse 2 in its own paragraph and the
     `\s1` gets no vid (basic/section, advanced/list) — while a heading
     in the MIDDLE of a verse DOES get one
     (advanced/custom-attributes' `\s`). So it is a backwards trim from
     the boundary, not a forward stop. "Heading" is the spec's own
     `ParaTitlesSections` / `ParaIdentification` / `ParaIntroductions`
     grouping; "empty" catches `\b` and `\s5`
     (paratextTests/NoErrorsNesting).
   - **A CELL is a block and the `<table>` hoists the vid.**
     specExamples/table writes `<table vid="…">` and puts the eid
     inside the last `<cell>`; cells themselves never carry a vid.
   - **A SIDEBAR is its own scope.** `\esb`'s own paragraphs carry NO
     vid (specExamples/attributes, extended/sidebars,
     extended/contentCatogories2) but the verse SURVIVES the sidebar and
     resumes in the paragraph after `\esbe` (usfmjsTests/esb).
   Because the close point depends on what comes NEXT, the export is
   TWO-PASS: `decorate()` scans the token stream and files where every
   eid goes and which block nodes carry a vid; the fold then writes.
3. An open CHAPTER closes (emit `<chapter eid/>`) AFTER the close of
   its last block, i.e. immediately before the next `\c` element or
   at document end. (Held as written — confirmed by the fixtures,
   including that a trailing empty `\b` still precedes the eid.)
4. **CORRECTED**: two Option slots are not enough for the verse, which
   is why pass one exists. The chapter IS one Option slot.
5. **ADDED, fixture-forced**: a content→attribute LIFT only happens
   when the content is PLAIN TEXT. An XML attribute cannot hold markup,
   so `\vp \+it \+wj 21\+wj*\+it* \vp*` stays an ordinary `<char
   style="vp">` with its nesting intact — which is what
   biblica/PublishingVersesWithFormatting writes in its `origin.xml`
   AND its `origin.json`. **usj.rs has the same latent divergence and
   was NOT changed** (that case is excluded from the USJ pin for an
   unrelated erratum, so the USJ oracle cannot see it). One line for
   Will: make usj.rs match, or rule the lift unconditional there.
6. **ADDED, fixture-forced**: whitespace rule 4 does NOT transfer. In
   USX a WHITESPACE-ONLY run IS content — `\f …\f*\n\v 4` writes
   `</note> <verse eid="MAT 1:3" />` (7972 whitespace-only text nodes
   across the corpus), and `<char style="ft"><char
   style="xt">ref</char> </char>` keeps one too. Rules 1–3 carry over
   unchanged: measured, no fixture has whitespace in front of a
   `<para>`/`<chapter>` open tag and no `<para>` ends with one, so
   block seams and EOF still drop.

### Divergence record (2026-08-20, the build)

The oracle's first full run read 137/208. Two of the seventy-one were the
TEST's fault (a BOM ahead of the root element; nothing else) and three
were rule corrections landed above (sidebar scope, table-vid off the
first cell, the non-liftable `\vp`). The rest is here, and
`tests/usx_corpus.rs`'s `EXCLUDED` is the enforced copy — 13 cases, one
line each, two categories.

**Thirteen against USJ's twenty, and the delta is the interesting part.**
Seven cases the USJ pin cannot claim have an `origin.xml` that agrees
with US against their own `origin.json`: the six seam-space fixtures
(usfmjsTests isa_verse_span / misc_footnotes / pro_quotes /
tit_1_12_footnote / isa_footnote, and specExamples/footnote's `\fv*\ft`
seam) plus biblica/CrossRefWithPipe's phantom EOF space.
biblica/PublishingVersesWithFormatting's XML both spells `code="MAT"`
correctly (its JSON says `XXA`) and confirms the non-liftable-`\vp` rule.
So the USJ exclusion list's "FIXTURE ERRATA" reading is now corroborated
from a second direction for eight cases — exactly the prior art the USJ
sketch predicted for usfmBodyTestD and specExamples/footnote.

**FIXTURE ERRATA (10 cases).** Three are NEW and worth naming:
paratextTests/NoErrorsPartiallyEmptyBook swallows `\h` and `\mt1` into
the preceding `\rem`'s content where its OWN origin.json keeps three
paras; usfmjsTests/isa_inline_quotes spends the `\fqa seventy men. \f*`
trailing space TWICE (once inside the char, once as a phantom `" "`
before `</note>`); usfmjsTests/usfmBodyTestD puts `vid="TIT 1:3"` on the
very paragraph that STARTS verse 3, which no other fixture does. The
other seven are the literal-newline family (advanced/footnote-structures
at a `\f*`/`\v` seam, specExamples/table's cell,
extended/contentCatogories1's note), the phantom-space family
(special-cases/empty-attributes, WordlistMarkerMissingFromGlossary…),
advanced/complex omitting every sid AND eid, and
special-cases/figure_with_quotes_in_desc inventing an escape dialect.

**PURPOSEFUL (3 cases).** The same unknown-marker pop-all recovery the
USJ pin documents, same three fixtures (luk_quotes, usfm-body-testF,
specExamples/milestone), same reasoning: `\s5` occurs 299 times in 21
validated-pass fixtures and 19 of them read our way.

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
- **BUILT as tests/usx_corpus.rs, not as a python script.** The oracle
  is testData (usj-export.md's ruling), a committed cargo test, rayon
  par_iter over the 208 validated-pass cases with an `origin.xml`. The
  comparison is STRUCTURAL — element name, attribute MAP, child
  sequence, text — with a first-divergence path reporter
  (`/usx/para[2]/char[0]@style`). Exactly two freedoms are forgiven,
  both properties of XML and not of USX: attribute ORDER (the fixtures
  themselves use `<book code style>` but `<char style lemma>`) and
  pretty-print INDENTATION (a whitespace-only text node THAT CONTAINS A
  NEWLINE; a whitespace-only run of SPACES is kept, because that is
  content). The XML READER is hand-rolled in the test, ~130 lines, no
  dependency: the corpus has no namespaces, no DTD, no comments, no
  CDATA and only the five predefined entities (measured). `src/` stays
  a writer with no XML parser in it.

## HTML

Split to its own doc 2026-08-21 (Will, for reading clarity):
sketches/html-export.md — the view-export sketch, the milestone/
text-identity rulings, and the html-tables.md authoring pass it waits
on.

## Open questions
1-6. Inherit usj-export.md's, all CLOSED 2026-08-20 for USX: hand-rolled
   writer (yes, and the oracle's XML READER is hand-rolled in the test
   too — no dependency); x-bare DEAD; lift table confirmed, plus the
   plain-text-only precondition; whitespace = testData's, with rule 4
   INVERTED; oracle = a committed cargo test over testData; sids AND
   eids always on.
7. RESOLVED 2026-08-21 (Will: "you can adjust that usj yeah"):
   `usj.rs` now carries the same `liftable` guard as `usx.rs` — a lift
   whose content is not plain text does not happen at all (both of
   biblica/PublishingVersesWithFormatting's fixtures agree). Zoo case
   `a_markup_bearing_lift_stays_an_ordinary_char_element` pins it; the
   usj oracle held at 187/187.
