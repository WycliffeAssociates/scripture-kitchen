# Plan: custom `\z` markers behave as their category

Engine at v0.1.2 (`eca6635`). Ships in the next minor after whatever lands
first from `planning/plans/`. Nothing here is implemented.

Spec: https://docs.usfm.bible/usfm/3.2/extensions.html

## 0. Spirit

A user marker is a spec marker the table has not met. The spec says so in one
field: `\category`. So a registered `\zfoot` is not "an unknown marker we are
lenient about"; it is a footnote with a different name, and it gets EVERY
behaviour a footnote has — the caller payload, the note scope, the note
context for its children, the closing rules, the lint contexts, the mask
treatment, the export shape. The same for every other category.

Three invariants this plan serves:

1. **A row is the behaviour.** Nothing anywhere branches on "is this an
   extension" to decide what a marker does. Extensions resolve to rows, and
   the rows already carry the behaviour. The one thing an extension row cannot
   carry is its NAME, so the one new predicate in the engine is "read this
   marker's name off the span, not the row".
2. **The engine never reads `markers.ext`.** It takes a normalized list —
   name, category, attributes. A `markers.ext` reader is one translator into
   that list; a `custom.sty` reader later is another. Neither touches the
   lexer.
3. **The wire does not move.** The dish stays format 4, `MarkerIdx` stays a
   `u8`, and the TS marker table stays generated from the same rows. A host
   that was never told about extensions still decodes every buffer.

## 1. What the spec gives, and what we do with each category

`markers.ext` is USFM-shaped. Per marker: `\marker <name>`, `\category
<word>`, `\description <text>`, zero or more `\attribute <name>`. Names must
start with `z`. Processors may ignore extensions entirely. The categories,
with the spec row each one COPIES to become a template row (section 2):

| spec category | copies | kind | fine category | payload | ws after name | closing | contexts |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `header` | `h` | Paragraph | ParaIdentification | None | AtLeastOneHorizontalWhitespace | None | BookHeaders |
| `title` | `mt` | Paragraph | ParaTitlesSections | None | AtLeastOneHorizontalWhitespace | None | as `mt` |
| `introduction` | `ip` | Paragraph | ParaIntroductions | None | TagEndDelimiter | None | BookIntroduction, ChapterContent |
| `sectionpara` | `s` | Paragraph | ParaTitlesSections | None | TagEndDelimiter | None | ChapterContent |
| `versepara` | `p` | Paragraph | ParaBody | None | TagEndDelimiter | None | as `p` |
| `list` | `li` | Paragraph | ParaLists | None | TagEndDelimiter | None | as `li` |
| `otherpara` | `lit` | Paragraph | ParaBody | None | TagEndDelimiter | None | as `lit` |
| `footnote` | `f` | Note | NoteFootnote | NoteCaller | TagEndDelimiter | as `f` | as `f` |
| `crossreference` | `x` | Note | NoteCrossReference | NoteCaller | TagEndDelimiter | as `x` | as `x` |
| `char` | `add` | Character | CharTextFeatures | None | TagEndDelimiter | as `add` | as `add` |
| `introchar` | `ior` | Character | CharIntroductions | None | TagEndDelimiter | as `ior` | as `ior` |
| `listchar` | `lik` | Character | CharLists | None | TagEndDelimiter | as `lik` | as `lik` |
| `footnotechar` | `ft` | Character | CharNotesFootnote | None | TagEndDelimiter | OptionalExplicitUntilNoteEnd | as `ft` |
| `crossreferencechar` | `xt` | Character | CharNotesCrossReference | None | TagEndDelimiter | OptionalExplicitUntilNoteEnd | as `xt` |
| `milestone` | `qt` (MilestoneOnly row) | Milestone | MilestoneQt | None | OptionalHorizontalWhitespace | as `qt-s` | as `qt-s` |
| `standalone` | `ts` | Milestone | MilestoneTs | None | OptionalHorizontalWhitespace | as `ts` | as `ts` |
| `cell` | `tc` | TableCell | CharTables | None | TagEndDelimiter | as `tc` | Table |
| `attribute` | — | — | — | — | — | — | accepted, registers nothing |
| `internal` | — | — | — | — | — | — | accepted, registers nothing |

Copy means every column of the source row EXCEPT `marker`, `shape` and
`numbered_max`, which the template sets itself (section 2). `opens_scope` and
`closes_scope` are authored from kind × category and come along for free.

Notes on individual rows:

- **`otherpara`.** The spec means "non verse text paragraph". The engine
  draws the verse-text line at the verse extent (`TextRule::VerseExtent`), not
  at the paragraph category, so today there is no engine difference between
  `versepara` and `otherpara`. `lit` is the closest spec row that is body-ish
  and not a heading. It is a distinct template anyway so the difference has a
  place to live if the mask ever grows one.
- **`standalone`.** The walker already treats a KNOWN milestone row in its
  bare spelling (`\ts \*`) as a point: no attributes, no pairing. That is
  exactly the spec's "bare milestone with no attributes or delimiter", and
  `\zms\*` from the spec examples already appears in `planning/quirks.md` as
  the row-0 shape of it. Copying `ts` is the whole implementation.
- **`milestone`.** Copies the MilestoneOnly `qt` row so `-s`/`-e` pairing,
  `sid`/`eid`, and attributes all behave as for `\qt-s`. This is the row the
  uW alignment markers `\zaln-s`/`\zaln-e` register against.
- **`cell`.** A cell without a column index is meaningless, so the template
  keeps `Numbering::TableColumns` and `\ztc1` is legal. Every other template
  is `Unnumbered` and matches the whole name exactly; the spec shows no
  numbered extension.
- **`header`.** `h` is `UpTo(3)`; the template is `Unnumbered` like the rest.
- **`attribute` and `internal`** describe USX internals (`cp`, `vp`, `ca`,
  `va`, `usfm`, `cat` are `attribute`); they have no USFM behaviour. The
  translator accepts them so a valid `markers.ext` never errors, and the
  registry drops them so the marker stays row 0 as today.

## 2. Template rows

Seventeen new rows appended to `ROWS` in `onion/src/tables/rows.rs`, one per
behavioural category, AFTER every spec row so no existing index moves. 153 →
170, well inside the `u8`.

- `marker`: a short lowercase name that starts with `z`, at most 8 bytes so
  `name_key` still packs it: `zheader`, `ztitle`, `zintro`, `zsect`, `zpara`,
  `zlist`, `zother`, `zfoot`, `zxref`, `zchar`, `zichar`, `zlchar`, `zfchar`,
  `zxchar`, `zms`, `zmsbare`, `zcell`. The leading `z` is load-bearing:
  `marker_idx` short-circuits every `z` lexeme before `by_name`, so a
  template row is UNREACHABLE from source text. `\zpara` in a file resolves
  through the registry like any other extension, or to row 0 if unregistered;
  it never lands on the template by name. `emit::by_name_arms` skips
  template rows so the generated match does not even carry them.
- `shape`: `Any` for every template except `zms` (`MilestoneOnly`) and
  `zmsbare` (`Any`, as `ts`).
- A new `pub const FIRST_EXTENSION_ROW: MarkerIdx` in `generated.rs`, and
  `pub fn is_extension(idx) -> bool` = `idx >= FIRST_EXTENSION_ROW`. This is
  the ONLY "is this an extension" predicate, and it exists for one purpose:
  section 5's name rule.
- A `template_for(category: ExtensionCategory) -> MarkerIdx` total function
  in `generated.rs`, emitted from the rows.

Table tests (`onion/src/tables/`): every template's kind × category is
coherent; every template shares every copied column with its source row (a
test that DIFFS the two rows so a curated tweak to `p` reaches `zpara`); no
template name resolves through `marker_idx`; `FIRST_EXTENSION_ROW` equals the
count of spec rows; `ROW_COUNT <= 256`.

## 3. The registry

`onion/src/extensions.rs`, new module.

```rust
pub enum ExtensionCategory { Header, Title, Introduction, SectionPara, VersePara, List, OtherPara,
    Footnote, CrossReference, Char, IntroChar, ListChar, FootnoteChar, CrossReferenceChar,
    Milestone, Standalone, Cell, Attribute, Internal }

pub struct Extension { pub name: String, pub category: ExtensionCategory, pub attributes: Vec<String>, pub description: String }

pub struct Extensions { /* name bytes → MarkerIdx, plus the declared list */ }

impl Extensions {
    pub fn new(list: &[Extension]) -> (Self, Vec<ExtensionReport>);
    pub fn resolve(&self, name: &[u8], shape: SpellingShape) -> MarkerIdx;
    pub fn declared(&self) -> &[Extension];
}
```

- `new` validates: the name starts with `z`, is ASCII alphanumeric, and is at
  most whatever the scanner's name run allows; the category is one of the
  nineteen. `Attribute` and `Internal` register nothing. A duplicate name
  keeps the FIRST and reports the second. Reports are values, not errors: a
  bad `markers.ext` line costs one entry, never the whole file.
- `ExtensionReport` is ONE flat shape: `{ name: Option<String>, reason:
  String }` — "malformed extension" plus a sentence. No error taxonomy, no
  codes, no positions. A host shows the sentence.
- `resolve` is the one hot call: only reached for lexemes starting with `z`,
  which are rare in ordinary text and a per-word event in aligned corpora
  (461,352 `\zaln-s` in en_ult per `quirks.md`). An `FxHashMap<Box<[u8]>,
  MarkerIdx>` is fine; measure against a sorted `Vec` with a linear scan
  since the map will usually hold under twenty names.
- **Shape agrees or it is row 0.** `resolve` takes the shape the scanner
  classified, exactly as `by_name` does for `qt`. A name registered as
  `milestone` spelled `\zfoo` (plain) resolves to row 0; a name registered as
  `char` spelled `\zfoo-s` resolves to row 0. The spelling has already decided
  the token kind, and a row that disagrees with the spelling would give the
  walker a milestone token on a character row. Row 0 keeps today's behaviour
  for exactly the cases that are wrong today anyway.
- `Cell` is the one category where `resolve` strips trailing digits before the
  name match and re-checks them with `digits_ok` on the template row. Every
  other category matches the whole lexeme.

### 3.1 Where it lives at runtime

`lex(source)` is a plain function called from every door (parse, diff, merge,
format, mask, toc, lint, find). Threading an `&Extensions` through each is a
signature change on the whole public surface for one value that changes once
per project. So:

- A process-wide registry: `static EXTENSIONS: ArcSwap<Extensions>` (or an
  `RwLock<Arc<_>>`; pick whichever is already a dependency), read once per
  `lex` call into a local `Arc` so the scanner never takes the lock per
  lexeme. `lex` and every door read it. `pub fn set_extensions(list) ->
  Vec<ExtensionReport>` replaces it and bumps a `generation: u64`.
- `pub fn lex_with(source, &Extensions)` is the pure form, for tests and for
  any future host that wants two projects in one process. `lex` is
  `lex_with(source, &current())`.
- Sous-chef and the corpus binaries run books in parallel; an `ArcSwap` read
  is a pointer load, so this costs them nothing.

### 3.2 The cache

The Pantry's `ChunkStore` keys chunks on content alone. After the registry
changes, the same bytes parse differently, so every cached product is stale.
`Pantry` records the `generation` it derived under and flushes itself when the
generation it sees on the next call differs — a compare on every `update`,
`parse`, `mask`, `lint`, `toc`, `find`. No ordering hazard between "set
extensions" and "which handle": whichever handle touches a stale cache clears
it first. Test: `update`, `set_extensions`, `update` with identical text
reports misses, and the dish reflects the new rows.

## 4. The lexer change

One branch in `generated::marker_idx` (`generated.rs.tmpl`, since it is the
template's static code):

```rust
if lexeme.first() == Some(&b'z') {
    return extensions::current().resolve(lexeme, shape);
}
```

That is the whole lexer change. `classify_marker` already names the spelling
before the table is consulted, `marker_level` reads numbering off the row (so
`zcell` gets column digits and nothing else does), and ruling [F]'s comment
becomes true as written: an UNCONFIGURED `z` extension never needs a name
match. Update the comment; it currently says "never" and now means
"unregistered".

Downstream, nothing branches:

- `cst.rs`: the `UNRESOLVED` recovery no longer fires for registered markers;
  `opens_scope`/`closes_scope`/`closing` come off the template row. The
  `name == "li"`/`"w"`/`"esb"`/`"c"` comparisons keep working because template
  names never equal spec names — a `list`-category extension is NOT `li` for
  the list-container rules, which is correct: the spec gave us a category,
  not a claim to be `\li`.
- `lint`: `unknown-marker` stops on registered names because they are not row
  0. Context rules (`allowed_contexts`) apply per template. Attribute rules:
  `x-`/`z-` names already pass on any character row; declared `\attribute`
  names are stored on `Extension` and NOT consulted this pass (section 8).
- `mask`: rows land in their kind's `kinds[]` slot, so `\zaln-s` moves from
  the `Unknown` action to the `Milestone` action in every recipe.
- `format`: `MarkerKind::Unknown` arms stop matching registered markers; the
  paragraph/character formatting rules apply by kind.
- `diff`/`merge`: decision units are cut on rows, so a `versepara` extension
  now cuts like `\p`.

## 5. The name rule

A template row's `name()` is the template's name (`zpara`), never the marker's
spelling (`zmyp`). Every place that shows a name to a human or writes one to
an export must read the spelling off the token span when `is_extension(idx)`.
`usj.rs` already has `marker_name(source, token)`; lift it to `token.rs` (or
`scanner::spelled_name` made `pub`) as the one helper, then audit every
`generated::name(` call site:

| site | today | change |
| --- | --- | --- |
| `html.rs:292` `align`, `html.rs:265`, `html.rs:1084` family | row name as class/family | class = spelled name; family stays the template's (a `zpara` renders in the `p` family) |
| `usj.rs:660`, `usx.rs:935` cell align | `ends_with('r')` on the row name | read the spelling; `\ztcr2` aligns end |
| `usj.rs` / `usx.rs` style attribute | `marker_name` (source) | already correct; add a test |
| `lint/fix.rs:146` `rename` | row name LENGTH for the splice | spelled name length |
| `lint/structure.rs:356` `container_end_text` | row name bytes | spelled name bytes (only reachable if a template is a container; today none is, assert it) |
| `lint/flat.rs:450` `version_row` | row name | unchanged: extensions have no version row |
| `cst.rs` name comparisons | row name | unchanged (see section 4) |
| `wire/emit.rs:266` | row name into `MARKERS` | unchanged: the TS table wants the template name |

TS side, generated `MARKERS` gains the seventeen rows and `onion-reader.ts`
grows `FIRST_EXTENSION_ROW` and `isExtension(idx)`. Sefer's three
name-reading sites read the spelling from the doc span when
`isExtension(markerIdx)`: `src/editor/core/blockTable.ts:79`,
`src/editor/core/docStructure.ts:189`, `src/editor/core/cst.ts:210`. Every
kind- and category-keyed consumer (`mapping.ts`, `analysis.ts:classWordOf`,
the mask filters, the findings feed) needs no change.

## 6. The translator and the doors — NOT this pass

This pass ends at the registry. No file is read and no wasm door is added:
`onion/src/bin/playground.rs` grows a flag (say `--extensions
zaln=milestone,zmyp=versepara,…`) that builds an `Extensions` value in code
and installs it with `set_extensions` before lexing, which is all step 8 and
the tests need. What follows is the shape the later pass plugs into, kept
here so the registry is built for it.

**Done, in `mise/src/extensions.rs`**: `parse_markers_ext(text) ->
MarkersExt { markers: Vec<CustomMarker>, malformed: Vec<Malformed> }`, with
`ExtensionCategory` as the nineteen spec words and `Malformed { line, name,
reason }` as the one flat report. Line-delimited, one field per line;
`\description` optional, `\attribute` repeatable; a bad entry costs only
itself. The registry (section 3) takes `&[CustomMarker]` and maps
`ExtensionCategory` to a template row; onion's `Extension`/`ExtensionReport`
names above are these mise types.

Doors, on the free-function surface of **`galley` first** — galley is the
superset of onion and is the ONLY build Sefer vendors, so anything onion can
do at the wall galley must do too, if only as a re-export — and mirrored in
`onion-wasm` for its own package:

- `extensionsFromMarkersExt(text: string): string` — the normalized list as
  JSON, plus reports, for a host that wants to show them.
- `setExtensions(json: string): string` — takes the normalized list (NOT the
  file), installs it, returns the reports as JSON. Passing `[]` clears.

Two doors, not one, so a host with a `custom.sty` (or a UI that lets a user
add a marker) feeds `setExtensions` directly. The JSON shape is the
`Extension` struct: `{ name, category, attributes, description }`.

## 7. Sefer

Outside this plan's repo and AFTER the doors pass; listed so the engine work
is shaped for it:

- `src/core/galley/galley.ts` grows `setExtensions(list)` on the service and
  `extensionsFromMarkersExt(text)` as a pure door.
- First step, before any file IO: Sefer HARDCODES the one registration the
  aligned corpora need, `zaln` as `milestone`, and installs it through
  `setExtensions` at composition. That alone takes en_ult from 922,704
  unresolved marker tokens to zero.
- Later, project open reads `markers.ext` from the project root through the
  `FileSystem` port; absent means an empty list. Before the first `analyze`
  of a project, and again on project switch, the list is installed. The
  reports stay a value on the galley service and show once at open where
  import problems already surface on the landing screens. Not a toast (it
  vanishes) and not a `Finding` (that shape carries a book and a source
  stamp, and `markers.ext` is neither).
- `vendor/galley/` is re-vendored from the tagged build; `manifest.json`
  hashes move, wire versions do not.
- Will's build-out rule holds: no new Sefer tests in this pass.

## 8. Deferred, deliberately

- **Declared attributes as the default attribute.** `\zmyc text|value\zmyc*`
  wants `value` to resolve to the first `\attribute`. The template cannot
  carry a per-marker default without the registry growing a second lookup
  consulted from `attributes::resolve`. The list is stored now so this is a
  registry-only change later.
- **`attribute` and `internal`.** USX-only; accepted and ignored.
- **The `markers.ext` translator and the two wasm doors** (section 6). The
  registry and `lex_with` are the seam; the playground exercises them.
- **A `custom.sty` translator.** Paratext projects carry custom markers there,
  not in `markers.ext`, and `markers.ext` has not been seen in the wild. The
  `setExtensions` door is the seam it plugs into.
- **Numbered extensions** beyond cells.
- **Per-handle registries.** `lex_with` exists; a host that needs two
  projects in one process threads it. Not this pass.
- **`\s5`.** 13,636 in en_ulb alone and NOT a `z` marker; this plan does not
  touch it, and the unknown-marker count on that corpus will not move.

## 9. Order of work

Each step leaves `cargo nextest run` and `cargo clippy --all-targets` green,
and the tree uncommitted for review.

1. **Template rows.** `rows.rs` + `emit.rs` (skip templates in `by_name_arms`,
   emit `FIRST_EXTENSION_ROW`, `is_extension`, `template_for`); regenerate;
   table tests from section 2. Nothing resolves to them yet; behaviour is
   unchanged and the wire is unchanged.
2. **Registry + lexer.** `extensions.rs`, the global, `lex_with`, the one
   branch in `marker_idx`, ruling [F]'s comment. Lexer tests: one fixture per
   category resolving to its template; shape mismatch → row 0; `zcell`
   digits; unregistered → row 0; duplicates and bad names reported.
3. **CST and lint.** Tests that a registered `footnote` inside a paragraph
   leaves the paragraph open and closes at `\zfoot*` and by displacement; a
   `char` extension pairs with its `*`; `footnotechar` peers sit as siblings
   inside a registered footnote; `standalone` is a point; `milestone` pairs
   `-s`/`-e`; `unknown-marker` is silent for registered names and still fires
   for unregistered ones and for shape mismatches; context rules fire per
   template (a `sectionpara` in book headers).
4. **Name rule.** Section 5's audit; export round-trip tests: USJ, USX and
   HTML carry the spelled name for every category; `rename` fixes splice the
   right length.
5. **Mask, format, diff.** One test each that a registered `char` extension
   is removed from verse text like `\add`, a `versepara` cuts a unit like
   `\p`, and format treats a registered paragraph as a paragraph.
6. **Cache.** Pantry generation check; the test in section 3.2.
7. **Playground flag.** `--extensions name=category,…` builds the
   `Extensions` value in code and installs it before lexing. Regenerate the
   readers (`cargo run --bin codegen`) so `MARKERS` carries the template rows;
   assert wire versions unchanged. The wasm build waits for the doors pass.
8. **Measure.** With `--extensions zaln=milestone`, run lint over
   `testData/stressCorpora/en_ult`: report the unknown-marker count before
   and after, and `lex` throughput on the same books, in the pass summary.
   Add both to `planning/quirks.md` if anything surprises.
9. **Ledger.** `planning/choices.md` entry: templates over runtime rows, the
   global registry, the shape rule, the name rule. `GLOSSARY.md`: the
   "Marker" entry currently says custom markers "fall back to their span";
   rewrite it.

## 10. Settled

- **Template names**: the seventeen above. The leading `z` is the property
  that matters; eight bytes keeps them inside the packed name key. Test
  fixtures register USER names (`zmyp`, `zmyc`, `zfoot`…), never a template
  name, so nobody reads `\zpara` in a fixture as a spec marker.
- **`otherpara` → `lit`**: a heading row would set the HEADING class in
  Sefer and render as a title; `rem` is identification in the headers
  context; `pc`/`pr` carry alignment the extension did not ask for. `lit` is
  a body paragraph with no verse-text meaning of its own, which is the spec's
  description word for word.
- **Reports**: on the service, shown once at open beside import problems.
  See section 7.
