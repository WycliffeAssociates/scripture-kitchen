# USJ export sketch (roadmap item 2a — NOT APPROVED, pseudocode to react to)

Written 2026-08-20 from live usfmtc probes (scratchpad `usj_shapes.py`,
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

Hand-rolled JSON writer, no serde. Rationale: USJ is a small CLOSED
shape (eight element types, known keys), the crate is no-deps, and the
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
| root | `{type:"USJ", version:"3.0", content:[…]}` | "3.0" = USJ schema version, a const |
| `\id` + BookCode + description | `{type:"book", marker:"id", code, content:[desc]}` | description text is content |
| Paragraph node (`p q1 s1 h mt1 ms …`) | `{type:"para", marker, content}` | `\b` (childless): OMIT the content key entirely |
| Character node | `{type:"char", marker, …attrs, content}` | |
| Note node (`f fe ef x ex`) | `{type:"note", marker, caller, content}` | NoteCaller token LIFTS to the `caller` key, dropped from content |
| `\c` + Designator (leaf pair) | `{type:"chapter", marker:"c", number}` | an ELEMENT in the stream, not a container; no sid (see opens) |
| `\v` + Designator | `{type:"verse", marker:"v", number}` | inline in para content |
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
| unknown row-0 marker (`\s5`, unconfigured `\z*`) | `{type:"ms", marker, "x-bare":"true", content?}` | usfmtc's shape — MIRROR IT (open question 2) |
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

CAVEAT (open question 3): these are usfmtc-observed, not read off the
3.2 marker pages as usx.md's TODO asked. Confirm against the pages or
rule usfmtc as authority. Edge: a lifted marker with NO enclosing
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

## Whitespace policy (what this export drops, exactly)

Ours to define — usfmtc's own trailing-newline behavior is quirky
(observed: keeps `"…God\n"` before a mid-para `\v`, strips before a
table, keeps before an inline `ms`). PROPOSED policy, simple and
stateable:

1. Text tokens emit their bytes verbatim.
2. Newline tokens INSIDE paragraph content emit as `"\n"` (they are
   content — line breaks inside a verse are real).
3. Newline tokens at BLOCK seams (immediately preceding a paragraph/
   sidebar/table-row node's opening marker, or trailing at a block's
   close) are dropped — they are USFM's line discipline, not content.
4. Delimiter whitespace folded into marker spans was never content.

Divergence from usfmtc's quirks is expected at seam edges; the oracle
normalizes (below) rather than chasing their exact trailing-`\n`
choices.

## The oracle (usfmtc as reference — the whole reason USJ ships first)

Methodology:
1. Ours: `usj(book)` → parse with serde_json (dev-dep) into Value.
2. Theirs: the scratchpad venv runs usfmtc over the same file → USJ.
3. Compare STRUCTURALLY: walk both trees; `type`/`marker`/every
   attribute key compare EXACTLY; text content compares per-element
   after normalization (collapse `[ \t\n]+` → one space, trim at
   element edges). Report the first divergent path.
4. Corpus scope: EXCLUDE books where lint reports any Recovery —
   usfmtc REPAIRS damage (observed: an unclosed `\f` swallows the next
   verse INSIDE the note; our tree closes it at the boundary), so
   divergence there is by design, not a bug. Today that excludes
   exactly en_ulb ISA and MRK.
5. Pins: full-corpus green minus the excluded two; en_ult is the
   stress case (zaln/ms density), bdf_reg the non-ASCII case.

Delivery: the oracle is a SCRIPT (python + cargo run), not a cargo
test — it needs the venv. Committed as planning/tools/usj_oracle.py
(or scratchpad-only, open question 5). The cargo test suite gets the
ZOO instead: hand-verified expected-JSON fixtures for every mapping
row above, no python involved.

## Test cases (plain english)

- One per mapping row (the zoo): book+description, each para kind,
  `\b` omits content, char nesting, note with caller lift, chapter/
  verse elements, milestone spelling preserved incl. bare `\ts`,
  list container emits points-not-container, consecutive `\tr` group
  into ONE table (and an intervening `\p` splits into TWO), sidebar
  with and without `\cat`, figure with `src`→`file`, ca/cp/va/vp lift,
  orphan `\cp` (no chapter) stays a char element, `\usfm` dropped,
  unknown marker x-bare shape, optbreak.
- Attribute cases: default-attr resolution, multi-attr, later-wins on
  a duplicate, milestone attrs, attr on `\fig`.
- Whitespace: intra-verse newline kept as `\n`; block-seam newline
  dropped; the delimiter space never appears.
- JSON validity: every zoo output parses; escaping (quote, backslash,
  control bytes in Text — bdf_reg has curly quotes).
- Determinism: key order fixed by the writer (type, marker, attrs in
  interpreter order, content last) so diffs are stable.

## Open questions
1. Hand-rolled writer + serde_json as DEV-dep for the oracle — ok?
2. Mirror usfmtc's `x-bare:"true"` ms shape for unknown markers? (It
   keeps the oracle clean over en_ulb's 13,636 `\s5`; the alternative
   — our own shape — diverges on every uW book.)
3. The lift table is usfmtc-observed; read the five remaining marker
   pages to confirm, or rule usfmtc as authority?
4. Whitespace policy as proposed (ours, stateable) vs chasing
   usfmtc's exact seam quirks?
5. Where does the oracle script live — planning/tools/ (committed) or
   scratchpad (ephemeral)?
6. usfmtc omits chapter/verse `sid` in USJ unless `addesids()` is
   called. PROPOSED: we omit too (match the default); USX is where
   sids/eids earn their keep (see usx-html-export.md).
