# Lint sketch (rulings landing; remaining opens at bottom)

Started 2026-08-18 while Will was away; marked up 2026-08-19. Prior art:
onion's lint (its CODES were mostly right — port the list, not the
machinery). ideas/committed/linter.md is DELETED — its still-true rulings
are folded in below; its architecture (the `push_token` emit funnel,
`Scanner<L: Listener>`, lint-over-bare-tokens, the slot fold) is dead:
lint is a pass over (source, tokens, cst), full stop.

## Architecture ruling (Will, 2026-08-19): three lines, no fusion

    let tokens = lex(source);
    let cst = cst::build(&tokens);
    let report = lint(source, &tokens, &cst);

- **Lint ALWAYS gets a CST.** One entry point, not two — the
  `lint_prepared` + `lint(source)` sugar pair is dropped. The CST is
  cheap (~6 ns/token, ~40% of a lex) and without it lint would recompute
  forced closures the walker already judged. Callers that "don't want a
  tree" still build one; it's the price of never re-deriving.
- **No listener/funnel/feed.** `cst::build` is a single forward pass —
  no lookahead, no lookbehind (every decision reads the current token +
  the frame stack; the "displacing token" is derived after the fact from
  the node's last child, not peeked). Lint's own passes are equally
  linear. Three cheap passes beat one fused pass that entangles the
  scanner with observation. (A second pass over token rows floors at
  ~1 ns/token — measured.)

## Ruling reversal to record (Will, 2026-08-18)

Ordering DOES flag verse sequence problems. Earlier leaning was to
abstain; reversed: verses that are **non-contiguous, out of order, or
duplicated** are all findings — "no one of which is real Bible data."
Severity may differ per code, but all three exist.

## Shapes

```rust
// CodeMirror's exact severity ladder — Diagnostic.severity is one of
// "error" | "warning" | "info" | "hint", and the session maps 1:1.
enum Severity { Error, Warning, Info, Hint }        // RULED (2026-08-19)

enum Category {
    Structure,   // nesting, closers, barriers, recovery
    Ordering,    // chapter/verse sequence (the interpreter's lane)
    Attributes,  // attr lists: forms, terminators, unknown names
    Payload,     // designators, callers, book codes — malformed content
    Form,        // whitespace/delimiter shape, marker-not-ws-preceded
    Version,     // deprecated/removed forms keyed on \usfm
}

struct Observation {
    code: Code,      // u16 enum, one per finding kind
    anchor: u32,     // token index into the linted slice
    second: u32,     // the other party (opener for an orphan closer…);
                     //   u32::MAX = none (flat, wasm-friendly)
    aux: u32,        // per-code integer payload — see "message params"
}

struct LintReport {
    book: Option<u32>,     // BookCode token idx; None IS the missing-\id finding
    observations: Vec<Observation>,
    fixes: Vec<Fix>,       // side table; Observation→Fix by parallel lookup
}
```

**Rules table authoring (RULED 2026-08-19)**: a dedicated file
collocating the `Code` enum with its data, rows.rs-style — declaration
order IS the u16 discriminant (asserted), `name` is the only durable
identity. Codegen only if it earns it; hand-authored is fine.

```rust
pub struct LintRow {
    pub code: Code,
    pub name: &'static str,        // "unclosed-note"
    pub category: Category,
    pub severity: Severity,        // default
    // at/after this \usfm version, this severity instead; None = flat.
    // This column OWNS the version facts no MarkerRow owns.
    pub escalation: Option<(UsfmVersion, Severity)>,
    pub aux: AuxKind,              // what Observation.aux means here —
                                   //   the column that keeps aux non-opaque
    pub template: &'static str,    // default EN; {anchor} {second} {aux};
                                   //   rendering/localization = consumer's
    pub fix_label: Option<&'static str>,  // None = never offers a fix
}
pub enum AuxKind { None, ExpectedNumber, NumberingCap, Count, Version }
pub enum UsfmVersion { V3_0, V3_2, V4_0 }
```

The "aux interpreter" worry is priced: it is the `AuxKind` column plus
one match at the render site; the lib may ship a convenience
`render_message(&row, obs, source) -> String` (allocates only at the
serialization boundary, which the ownership law permits).

**Message params (RULED 2026-08-19)**: there are NO string params, ever. Everything textual a
message needs is already a SPAN in the document — reachable through
`anchor`/`second` (marker name = the anchor token's bytes; the offending
designator = its span). What's left is small integers: expected verse
number, the row's numbering cap, the declared version. That is `aux`, a
u32 whose meaning is per-code, documented in the rules table row
(discriminated union by `code`, exactly the "bits for aux" idea). Zero
allocation, crosses wasm as one flat u32 array
(`[code, anchor, second, aux] × n`). Codes with no aux leave it 0.
Onion's ICU messageParams audit becomes: confirm every onion param is
either a span or a small integer (spot-check says yes — onion's params
were marker names and numbers).

**Suppressions (RULED 2026-08-19): v1 has NONE.** No inline suppression
comments, no `{code, reference}` machinery. The only knob is per-RULE
config (off / severity override), and even that can wait for a consumer
who asks. The versification question (below) is a future customer.

**Code identity (RULED 2026-08-19)**: no stable numbers. The durable
identity is the kebab-case NAME (`unclosed-note`), shipped as an
authored/codegen'd name table exactly like marker names; the u16 is
per-build wire data, always paired with a same-build table. Config and
humans use names; no reserved numeric ranges.

## The fix model (RULED: model A, byte-splice edit lists)

Token-space transforms (onion's model B) are rejected — we don't ship
objects, and CM applies text changes anyway. Model A:

```rust
struct Fix {
    label: &'static str,      // "insert \\f*", "renumber to 12"
    edits: Range<u32>,        // into LintReport.edit_list (sorted, non-overlapping)
}
struct Edit { from: u32, to: u32, insert: FixStr }  // 24 bytes, Copy

/// Our own tiny inline string (~15 lines with `new`/`as_str`), i.e. the
/// useful part of CompactString without the dependency or heap path.
struct FixStr { len: u8, bytes: [u8; 15] }  // 0 len = pure deletion
```

(RULED 2026-08-19, per-edit-owned inline: fix text is always short
engine-generated ASCII — closers, `\p\n`, renumber digits; worst spec
case `\table-e\*` = 10 bytes — so JS decodes with `String.fromCharCode`,
no TextEncoder. Anything longer than 15 (a long custom `\z` closer)
splits into ADJACENT SAME-POSITION edits, which concatenate — fixed
width with no cap. Swapping to compact_str later is a one-type change
if this ever grates.)

- A CM `Diagnostic.action.apply` dispatches the edits (session converts
  byte→UTF-16 through the existing shim; `insert` crosses as a string —
  no offset conversion exists for it). One transaction, one undo step.
- **Offered, never applied.** Never-synthesize governs TOKENS; a fix is
  proposed TEXT — once applied, the bytes are real and re-lex honest.
- "Fix all X" = concat a code's edits, sort, dispatch once.
- Composability rule: a fix's edits are computable from (source, tokens,
  cst) alone and must leave the partition re-lexable — enforced by the
  fix ORACLE test: apply → relex → relint; the fixed finding must be
  GONE and no new finding may appear.
- Node-level reasoning still emits byte edits, three mechanical
  derivations: reasons over NODES → addresses in TOKENS (anchor = token
  index) → emits BYTES (from/to = token.start/end()). "Replace this
  whole note" spans the node's byte EXTENT — owed helper:
  `Cst::extent(node, tokens) -> Range<u32>`, shared with the app's
  content-extents read.

**Format/prettify is the same machinery**: a formatter is a rule bundle
whose findings are all Hint + auto-fixable (normalize delimiter ws,
newline-before-marker, canonical attr spacing…). "Format document" =
apply every formatter fix in one transaction. No separate subsystem.

## Severity policy where the spec is fuzzy (salvaged from linter.md)

The spec's `Valid In::` lists systematically under-report
Footnote/CrossReference for character markers, and the docs' own
examples contradict their own lists (`\jmp` inside `\ef` in Example 13
while jmp's page omits Footnote; `\dc` likewise in `\x`). Ruled
(2026-08-12, standing): **the table records what the marker page says,
fuzziness included; LINT absorbs the uncertainty as severity.** A
character marker inside a note is NOT reported, or reported at the
lowest severity — crying wolf on valid text is worse than missing a
nicety. `fm` is the deliberate live example: its row stays as the page
has it, lint stays quiet. One tunable severity value, never a fork in
the data.

## Code list, v1

### Structure (read off `Node.reason` — a linear pass)
- `unclosed-note` — Recovery on a Note frame (3 live in the corpus:
  en_ulb ISA/MRK, bsb GEN). Fix: insert `\f*`/`\x*` at the boundary.
- `unclosed-char` — Recovery on a Character frame. Fix: insert `\X*`.
- `unclosed-at-eof` — Eof on a frame whose row wants a closer.
- `unterminated-container` — Recovery on a Container (version-keyed:
  optional 3.2 / required 4). Fix: insert `\list-e\*` before displacer.
- `unterminated-milestone` — Recovery/Eof on a Point (missing `\*`).
- `orphan-closer` — ClosingMarker leaf (no matching frame). Fix: delete.
- `orphan-terminator` — `\*` leaf. Fix: delete.
- `orphan-container-end` — `-e` point with no container in reach.
- `content-outside-sidebar-rule` — `\c` (etc.) leaf inside a Sidebar
  frame (the barrier kept it; the spec says it shouldn't be there).
- `unknown-marker` — row-0 Marker (also the pop-all recovery event).
- `nested-spelling-misuse` — `\+X` where the row isn't Character, or
  nesting depth says plain form belonged.
- `missing-paragraph` — a `\v` whose parent is the chapter, no paragraph
  between (usfmtc FABRICATES an implicit `\p` here,
  usfmparser.py:891 — we flag, never repair; ruled 2026-08-12). Fix:
  insert `\p\n` before the verse.
- NotePeer/Implicit closes are SILENT — no finding.

### Ordering (tokens only; needs the verse-designator interpreter)
- `designator-malformed` — fails the spec VERSE pattern (`12text`, `?`).
- `verse-duplicate` / `verse-out-of-order` / `verse-gap` — per chapter,
  from leading integers; ranges (`12-14`) count as their span. Fix for
  duplicate/out-of-order: renumber to next expected. aux = expected.
- `chapter-duplicate` / `chapter-out-of-order` / `chapter-gap`.
- `verse-before-first-chapter`, `missing-chapter` (book has none).
- `missing-verse-one`? (chapter starts at \v 2) — same gap machinery.
- Plain contiguity only — NO versification-scheme awareness (RULED
  question 3: traditions that legitimately skip verses are a future
  per-rule-off candidate, or just ignore the warning).

### Payload
- `missing-id` — `LintReport.book == None` (real: BSB Ecclesiastes).
- `book-code-unknown` / `book-code-not-uppercase` — vs the books aux
  table (authored, membership only — copy list from onion; UNBUILT).
- `chapter-without-designator` — `\c` then no Designator token.
- `caller-shape` — note caller not in `+ - ?`-or-word set (Hint).
- `numbering-out-of-range` — `\q7` etc.; reads the row's numbering cap
  (aux = cap).
- `numbering-mix` — book mixes bare and numbered spellings of one family
  (`\q` and `\q1`); AGGREGATE rule, per-family levels-seen bitmask,
  anchors to the token that revealed the mix. Consistency observation,
  Info-tier, never an error.

### Adjacency (the `(lastMarker, token)` shape — one token of lookbehind
inside lint's own walk; these markers open no scope, so the CST can't see
their misplacement)
- `ca-cp-placement` — `\ca`/`\cp` legal only immediately after `\c` (or
  the other of the pair); `va-vp-placement` — same for `\va`/`\vp`
  after `\v`. (Chapter/Verse are NOT contexts — chapters repeat, no
  monotonic encoding can say "right after \c"; hence adjacency rules.)

### Attributes
- `attr-trailing-form-deprecated` — AttrList span not ending `|`
  (version-keyed: deprecated 3.2, removed 4). Character frames ONLY:
  `\zaln-s |attrs\*` is the milestone's normal syntax, never deprecated —
  flagging it would light up every alignment corpus.
- `attr-both-lists` — two AttrLists adjacent to one marker (legal
  back-compat; "later definition wins" is the interpreter's merge rule).
- `attr-terminator-mismatch` — span shape (opened `|…` never re-piped).
- `attr-pipe-hint` — rung-3: Text containing `|` inside an attrs-capable
  node (Hint: "did you mean an attribute list?").
- `attr-unknown-name` — k/v interpreter vs `defined_attributes` (with
  the `a-*` prefix-wildcard matcher — the ONE place that learns the
  convention).
- `attr-required-if` — conditional cardinality the rows can't express:
  `eid` required IF `sid` was used (ms/qt.html); sid/eid optional
  standalone, required when paired. The rows say Optional; lint owns
  the required-if. Plus `attr-family-cardinality` (ta's "one or more").

### Form
- `marker-not-ws-preceded` — byte before a marker token is non-ws
  (`content\s1`; the para railroad requires ws — ruled 2026-08-18).
- `delimiter-shape` — the ws_after_name derivations (NBSP-after-name
  etc.; the Unicode-hs open question lands here).
- `empty-paragraph` — paragraph node with no content children (Info).

### Version
- `deprecated-marker` — `h#`, `addpn`, … reads the row's deprecated
  flag; severity by declared `\usfm` version.

## Two subsystems, one report

```text
lint(source, tokens, cst):
    structural: for node in cst.nodes[1..]: match node.reason …   // linear
    adjacency + form + payload + attributes: single token walk
    ordering:   filter Designator/BookCode → interpreter → sequence check
    aggregate:  numbering-mix bitmasks, closed out at EOF
    fixes computed per-rule beside the finding
```

## Build phases (agreed 2026-08-19 — each behind tests + a perf read)

1. **Skeleton + structural**: Code/Category/Severity/AuxKind enums, the
   lint_rows file (structural codes only), Observation/LintReport,
   `lint(source, tokens, cst)`, the linear Node.reason pass. Tests: one
   per structural code + the three live corpus findings (en_ulb ISA/MRK,
   bsb GEN) pinned. Playground `--lint-stats`. No fixes.
2. **Ordering**: verse-designator interpreter (spec VERSE pattern; its
   doc comment = vref's future comparison rules), books aux table (port
   from onion), chapter/verse sequence codes, missing-id.
3. **Token-walk rules**: adjacency (ca/cp/va/vp), form, payload
   (caller shape, numbering cap, numbering-mix aggregate), the four
   shape-only attribute rules. NOT the k/v-interpreter attr rules
   (attr-unknown-name, attr-required-if) — those ride whenever the
   attribute interpreter is built (exports want it too).
4. **Fixes**: Fix/Edit/FixStr, `Cst::extent(node, tokens)`, per-code
   fixes, and the fix ORACLE harness (apply → relex → relint: finding
   gone, nothing new) run over every corpus fix.

## Still open (small)
- messageParams audit vs onion — mechanical, do during build: every
  onion param must be a span or fit aux.
