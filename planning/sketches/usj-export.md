# USJ export sketch (roadmap item 2a — APPROVED 2026-08-20, rulings folded in)

RULED 2026-08-20: the ORACLE IS testData — it is the COMMITTEE'S data
(the reference prose is periodically fuzzy and every implementation
interprets it slightly differently; testData is what the committee
committed to, not just what usfm-grammar happens to do). Everything
below was re-verified against it: 261 cases, 209 `<validated>pass`,
207 of those with an `origin.json` USJ fixture. The usfmtc venv probe
that first authored this sketch demotes to a SECONDARY, uncommitted
scale check (en_ulb/en_ult aren't in testData).

Originally written from live usfmtc probes (scratchpad `usj_shapes.py`,
`usx_shapes.py` — every shape below was OBSERVED, not inferred). Style
and laws follow lint-sketch.md. Settled constraints honored
(settled-facts "Exports are folds over the CST"): the fold never feeds
back into the core; content→attribute lifting is a PROJECTION artifact
(usx.md's governing principle); the attribute interpreter
(sketches/attr-interpreter.md) is the k/v splatter; the export is the
LOSSY view, the token stream is not.

## Entry + output type (PROPOSED)

```rust
pub fn usj(source: &[u8], tokens: &[Token], cst: &Cst) -> String
```

Hand-rolled JSON writer, no serde (RULED). Feature-gated (RULED):
cargo features `usj`, `usx`, `html`, all in `default`; size-sensitive
consumers set `default-features = false`. Features are compile-time,
so wasm32 respects them like any target — gated-off code isn't in the
binary and the bundle genuinely shrinks; a published wasm artifact
bakes in whatever set its build chose. Output is a `String` (RULED):
no typed USJ tree in the crate — the CST is our model and a typed tree
would be a second model to keep in sync; tests use `serde_json::Value`,
and typed consumers have scripture-editors' `usj.model.ts` as the
target shape (checked: its `MarkerObject` keys — sid/eid/number/code/
altnumber/pubnumber/caller/align/category — are exactly this table's,
with custom attrs splatting in as extra keys).

Rationale for no serde: USJ is a small CLOSED
shape (a dozen element types, known keys), the crate is no-deps, and the
ownership law already blesses allocation at serialization boundaries.
The writer is ~40 lines (`JsonWriter { out: String }` + one string
escaper); serde would buy generality nothing here needs. The ORACLE
side does need to parse JSON — take `serde_json` as a DEV-dependency
only (precedent: rayon is already dev-only for the corpus tests).

## The fold skeleton (PROPOSED — shared by USJ/USX/HTML)

One read-only visitor over the CST, the export siblings each implement
it. Same event grammar as lint's walk (the proven driver shape):

```rust
trait Visit {                       // exports-internal, not public API
    fn node_open(&mut self, doc: &Doc, id: u32, node: &Node);
    fn leaf(&mut self, doc: &Doc, idx: u32, token: &Token);
    fn node_close(&mut self, doc: &Doc, id: u32, node: &Node);
}
fn fold(doc: &Doc, v: &mut impl Visit)   // ChildCursor stack, no recursion
```

Lint's walk stays its own (its machines and Emit are specialized and
perf-tuned); whether lint's driver later collapses onto this is a
net-deletion question for investigate-later, not this sketch.

## The mapping table (OBSERVED from usfmtc; the codegen name column)

| CST / token shape | USJ element | notes |
|---|---|---|
| root | `{type:"USJ", version:"3.1", content:[…]}` | "3.1" per testData + scripture-editors' USJ_VERSION (usfmtc said "3.0" — overruled by fixtures) |
| `\id` + BookCode + description | `{type:"book", marker:"id", code, content:[desc]}` | no description → `content: []` KEPT (fixtures keep empty arrays) |
| Paragraph node (`p q1 s1 h mt1 ms …`) | `{type:"para", marker, content}` | `\b` (childless): `content: []` kept, same rule (RULED — follow fixtures, not usfmtc's key-omit) |
| Character node | `{type:"char", marker, …attrs, content}` | |
| Note node (`f fe ef x ex`) | `{type:"note", marker, caller, content}` | NoteCaller token LIFTS to the `caller` key, dropped from content |
| `\c` + Designator (leaf pair) | `{type:"chapter", marker:"c", number, sid:"GEN 1"}` | an ELEMENT in the stream, not a container; sid EMITTED (RULED — fixtures carry sids; usfmtc's omit-by-default overruled); no content key |
| `\v` + Designator | `{type:"verse", marker:"v", number, sid:"GEN 1:1"}` | inline in para content; sid emitted; no content key |
| Milestone point node | `{type:"ms", marker:AS-SPELLED, …attrs}` | marker keeps the author's spelling (`qt-s`, `ts`, `zaln-e`); NO content key |
| Container node (list/table via `-s`) | NOT an element | the `-s`/`-e` points emit as inline `ms` right where they sit (observed); the container node is walker bookkeeping the projection ignores |
| bare `\tr` rows | `{type:"table", content:[rows]}` wrapper SYNTHESIZED around each run of CONSECUTIVE TableRow nodes | rows `{type:"table:row", marker:"tr"}`, cells `{type:"table:cell", marker:"tc1"/"tcr2"/…, align:"start"\|"end"}` — align derived from the `r` spelling |
| Sidebar node | `{type:"sidebar", marker:"esb", …category, content}` | |
| `\cat` node | its CONTENT lifts to `category` on the ENCLOSING note/sidebar; the node emits nothing | usx.md's governing principle, observed |
| `\ca`/`\cp` after `\c` | `altnumber`/`pubnumber` keys ON the chapter element | content→attribute lift |
| `\va`/`\vp` after `\v` | `altnumber`/`pubnumber` ON the verse element | |
| Figure node | `{type:"figure", marker:"fig", …attrs, content:[caption]}` | ATTR RENAME: `src`→`file` (observed; a per-format quirk map entry) |
| OptBreak token (`//`) | `{type:"optbreak"}` | |
| `\usfm` + its Text | DROPPED | observed: no element, no version echo (envelope version is USJ's own) |
| `\ref text\|loc\ref*` | `{type:"ref", loc, content:[text]}` | its own type, not char; default attr is `loc` (testData advanced/complex) |
| `\periph Title\|id="x"` | `{type:"periph", alt:Title, id, content:[paras…]}` | content TEXT lifts to `alt` — one more lift row (testData advanced/periph) |
| unknown paragraph-position marker (`\s5`) | `{type:"para", marker, content}` | RULED: testData's shape (usfmjsTests/1ch_verse_span: `{type:"para", marker:"s5", content:[]}`). usfmtc's `x-bare:"true"` ms shape is DEAD — it was only ever a mirror-usfmtc convenience, and the oracle pivot dissolved it |
| (error shape) | `{type:"unmatched", marker}` | usfm-grammar's damage shape (orphan closer etc.) — appears ONLY in validated=fail fixtures. NOT ours to emit: we lint, the export never judges |
| AttrList token | consumed by the interpreter → splatted keys | never content |
| Newline token | whitespace policy below | |

The `marker` column above is the token's own spelling; the codegen
name-mapping earns its keep only where a name CHANGES in projection —
observed set so far: `src`→`file` on fig, plus the content→attribute
lift table below. Both are tiny authored consts in the export module,
NOT row columns (usx.md's rule: "becomes an attribute in USX" is never
a property of the row).

## The content→attribute lift table (resolves usx.md's TODO)

usx.md flagged six markers whose content becomes an attribute, with
one target name confirmed. The probes observed all six:

| marker | target attribute | on |
|---|---|---|
| `ca` | `altnumber` | enclosing chapter element |
| `cp` | `pubnumber` | chapter |
| `va` | `altnumber` | verse |
| `vp` | `pubnumber` | verse |
| `cat` | `category` | enclosing note/sidebar |
| `usfm` | — dropped — | (nothing; USX root `version` is the USX schema's) |
| `periph` (its title text) | `alt` | the periph element itself |

CONFIRMED 2026-08-20 by testData (specExamples/chapter-verse shows
ca/cp → chapter altnumber/pubnumber and va/vp → verse, exactly this
table; advanced/periph adds the `alt` row). No marker-page read
needed — the committee's fixtures are the referee. Edge: a lifted
marker with NO enclosing
target (`\cp` before any `\c`) — PROPOSED: emit it as an ordinary char
element and let lint's placement finding carry the complaint; the
projection must not guess an owner.

## Attribute splatting (the lossy step)

The interpreter yields `(name, value)` pairs off the AttrList span:
- bare default value → the row's `default_attribute` name (observed:
  `\w word|grace\w*` → `lemma:"grace"`),
- unknown names pass through verbatim (`x-strong`, `link-href` —
  observed),
- duplicate name: LATER DEFINITION WINS (the interpreter's merge rule),
- key order, `=` spacing, quote style: all dropped — this is the
  documented lossy step (usx.md "Losslessness, for the record").

## Whitespace policy (RULED: adopt testData's canonicalization)

The export canonicalizes the way the fixtures do, so the oracle
compares EXACTLY — no fuzzy text normalizer between us and it. Read
off the fixtures:

1. Text tokens emit their bytes verbatim.
2. A newline INSIDE paragraph content becomes ONE SPACE (fixture:
   `"verse one "` — the line break before `\v 2` survives as the
   trailing space).
3. A newline at a BLOCK seam is dropped (fixture: `"verse two"`
   before a mid-line `\p` — no trailing space). Same for trailing
   whitespace before a closing marker (`\w gracious … \w*` →
   `"gracious"`).
4. Delimiter whitespace folded into marker spans was never content.

An earlier draft emitted intra-para newlines as `"\n"` — overruled by
the fixtures. Known wrinkle: exactly 4 of 236 fixtures still carry a
literal `\n` inside a JSON string (specExamples milestone/table,
57-TIT.greek.oldformat, contentCatogories1) — the exact edge rule
there is a TESTS-WILL-TELL item; investigate at the bytes when the
pin diverges, don't pre-guess.

usfmtc's own trailing-newline behavior is quirky and no longer chased
(it kept `"…God\n"` before a mid-para `\v`, stripped before a table,
kept before an inline `ms`).

## The oracle (RULED: testData is primary — it's the COMMITTEE'S data)

PRIMARY (committed cargo test, pure Rust, no venv): for every testData
case with `<validated>pass</validated>` and an `origin.json` (207
today), read `origin.usfm`, run our full pipeline + `usj()`, parse
both with serde_json (dev-dep) into `Value`, compare `==`. With the
canonicalization above adopted, structural-exact equality holds — key
order is the only freedom `Value ==` forgives, which is exactly right.
Report the first divergent path on failure.

- `<validated>fail` cases (52) stay OUT — that's where usfm-grammar's
  `unmatched` damage-shapes live; our answer to damage is lint.
- FAILURE MAY BE IN testData (Will, 2026-08-20: "consider carefully
  if failure is in testData"): investigate every divergence at the
  bytes FIRST; if the fixture is wrong, exclude the case with a
  one-line reason in the test — the lint precedent (ISA/MRK excluded
  for usfmtc's damage-repair). Tie-breaker when a fixture is suspect:
  jcuenod/usfm3's reading — a second interpreter, never an authority.
- No p-insertion mimicry (RULED): the validated-pass corpus never
  needs it (special-cases/empty-c: chapters with no paragraph, nothing
  synthesized) — the never-synthesize law survives the oracle for free.

SECONDARY (uncommitted scale check): the usfmtc venv script over the
full corpora (en_ulb/en_ult/bdf_reg aren't in testData) — scratchpad-
only, normalized text compare, Recovery-books excluded (usfmtc repairs
damage). Divergences there are READ, not pinned: where usfmtc
disagrees with testData conventions (version "3.0", no sids, x-bare,
key-omit for empty content), testData wins.

The cargo suite also gets the ZOO: hand-verified expected-JSON
fixtures for every mapping row above, no python involved.

## Test cases (plain english)

- One per mapping row (the zoo): book+description, each para kind,
  `\b` keeps `content: []`, char nesting, note with caller lift,
  chapter/verse elements with sids, milestone spelling preserved incl.
  bare `\ts`, list container emits points-not-container, consecutive
  `\tr` group into ONE table (and an intervening `\p` splits into
  TWO), sidebar with and without `\cat`, figure with `src`→`file`,
  ca/cp/va/vp lift, `\ref` with loc, `\periph` title→alt, orphan `\cp`
  (no chapter) stays a char element, `\usfm` dropped, unknown marker →
  para shape, optbreak.
- Attribute cases: default-attr resolution, multi-attr, later-wins on
  a duplicate, milestone attrs, attr on `\fig`.
- Whitespace: intra-verse newline kept as `\n`; block-seam newline
  dropped; the delimiter space never appears.
- JSON validity: every zoo output parses; escaping (quote, backslash,
  control bytes in Text — bdf_reg has curly quotes).
- Determinism: key order fixed by the writer (type, marker, attrs in
  interpreter order, content last) so diffs are stable.

## Rulings record (2026-08-20 — all six original opens closed)
1. Hand-rolled writer + serde_json dev-dep: YES. Plus feature gating
   (`usj`/`usx`/`html`, default all) and String output — no typed tree.
2. x-bare: DEAD — oracle pivoted to testData; unknown markers take
   usfm-grammar's para shape.
3. Lift table: CONFIRMED by testData fixtures (the referee), `alt` on
   periph added.
4. Whitespace: adopt testData's canonicalization (newline→space
   intra-para, dropped at seams) so the oracle compares exactly.
5. Oracle: committed cargo test over the 207 validated-pass fixtures;
   usfmtc venv demoted to scratchpad scale check.
6. sids: EMITTED (fixtures carry them) — usfmtc's omit-default
   overruled.

## Follow-on (own sketch)

The INVERSE converter (USJ → USFM) is wanted but is its own item:
sketches/usj-import.md. Different animal — a JSON reader plus a USFM
printer; output is canonical USFM (the lossy step can't un-lose).
