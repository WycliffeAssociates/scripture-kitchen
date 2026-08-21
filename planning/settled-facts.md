# Settled facts that constrain later work

Rulings that shape phases we have NOT started (exports, wasm, the
editor session, sous-chef, braid). Extracted from NEXT-STEPS 2026-08-19
so that file stays the open queue. These are decisions, not sketches —
reversing one is a design event, logged here with a date like the
originals below.

- **Editor direction is CodeMirror 6** (spike live and promising): the
  text buffer is truth — the engine's own model. No Lezer grammar EVER
  (a second parser = drift), no LSP for the app (in-process wasm), no CM
  `Language` (nothing consumes `syntaxTree()`; escape = implement
  lezer's `Parser` interface over the session if that ever changes).
- **wasm surface is STATELESS-FIRST** (ruled 2026-08-18, after the CM
  prototype leaned pure-functions): one call, `analyze(text) -> reads`,
  full re-lex + build + lint inside, UTF-16 index computed fresh per
  call (its version-pairing invariant evaporates by never outliving the
  call). Priced: the re-lex was never the cost (~60 µs prose book, ~2 ms
  heaviest aligned, ×2-3 wasm, behind the probe's 150 ms debounce); the
  only thing state would save is the whole-string boundary crossing
  (~1-2 ms, ALIGNED books only). Stateless is race-free by construction
  and survives any future async boundary trivially (result stamps its
  doc version; stale ⇒ discard). Reads stay typed arrays of UTF-16
  numbers (`token_spans`, `blocks`, `chapters`, `note_extents`,
  `text_runs`, `diagnostics`); JS never holds a token. The stateful
  `BookSession` (owns bytes + splices via `apply_edits`) is DEMOTED to a
  measured optimization — additive over the same pure core if a real
  device/corpus measurement ever demands it; do not build speculatively.
  Sync main thread first; worker doesn't even save the string copy;
  Rust-over-IPC parked behind both.
  IF incrementality is ever needed, its ruled shape is memoization
  INSIDE the stateless call, never the splice — full design (chunk
  hashing, chunk-relative coordinates, rebase-rides-the-emit-loop, no
  invalidation machinery, seam caveat, and the standalone
  partial-loading rationale for chapter-relative offsets) lives in
  ideas/committed/chunk-memoization.md. The irreducible floor is the
  string/ArrayBuffer crossing itself.
- **UTF-16 is a boundary index** (byte↔UTF-16 drift breakpoints, binary
  search both ways; rust-analyzer's `line-index` is the precedent) —
  never an eager conversion, never in the core. SHAPE AMENDED by
  measurement (2026-08-19, Will's Hindi-IRV question;
  experiments/utf16.rs is the record): per-drift-change anchors are
  pathological on dense scripts — 236% of the source on Devanagari, 49%
  on aligned en_ult — so the shape is FIXED-STRIDE anchors (256 B) +
  SWAR remainder count (`len − continuations + count(≥0xF0)`, which
  composes across a mid-character stride cut with zero special-casing):
  1.6% of source on every script, builds 6-13× faster, byte→utf16
  equal-or-faster where it matters (the bulk emit direction);
  utf16→byte pays ~100 ns/query, invisible at cursor-position rates.
  Hand-rolled SWAR (~10 lines), no dependency; simdutf/str_indices are
  the graduation path if true SIMD ever earns it. Production adoption
  happens when the wasm session is built.
- **No JS table**: the table's JUDGMENTS cross stamped on ranges —
  category + `payload_kind` (2 bits) + `closing` (2 bits) per marker
  range. NOTE `Category` has 26 variants (> 4 bits): either two bytes,
  or — likely right — ship a COARSER class axis (para/char/note/
  milestone/chapter-verse/sidebar, 3 bits); the app's shape inference
  needs only the class, and per-marker CSS comes from the marker NAME in
  the document itself. Decide at session-build time; no table/codegen
  change either way. Command pick-lists cross as codegen'd constants
  from the rows.
- **The editing model is the APP's** (cmWysiwyg's TEXT/FIELD/ATOM/CHROME/
  BREAK vocabulary never enters the library). The library ships neutral
  judgments + node CONTENT EXTENTS; the app infers intent around hidden
  markers from them. Sufficiency is proven (segment kinds ≈ TokenKind
  rename; its four shapes = table columns; anchors = adjacency + node
  extents). App-side wrinkle: segments may be FINER than tokens (a
  delimiter space split off a Text span) — views may subdivide, tokens
  stay truth.
- **No minted identity, ever**: per-build array indices internally,
  content-derived addresses (book code, chapter ordinal, verse
  reference) for anything durable, `ChangeDesc.mapPos` in-session.
  Nothing can drift from the bytes, so there is nothing an id would
  protect.
- **Ownership**: no owned twin — offsets only; the bundle
  (`Document/BookSession { source, tokens, cst }`) answers who holds the
  buffer; owned strings exist only where serialization allocates anyway.
- **Version data lives in LINT's rules table**
  (`severity(code, declared_version)`, fed by `\usfm`), and per-FORM
  facts (trailing lists deprecated 3.2 / removed 4) can only live
  there — no row owns a form's lifecycle. SOFTENED 2026-08-20 (Will):
  per-MARKER version facts MAY ride `MarkerRow` when that is simpler —
  it was never a hard rule; current lean keeps the small authored
  VersionRow in lint (zero codegen touch) and migrates onto MarkerRow
  only if the set grows. Escalation is a SLICE
  (`&[(UsfmVersion, Severity)]`, ruled 2026-08-20) so whole
  none→Warning→Error ladders are data, not hand gates.
- **Exports are folds over the CST** (USJ/USX/HTML): codegen'd
  name-mapping (official USJ names — check the schema in usfm-grammar),
  the attribute interpreter for k/v splatting (the lossy step), plus
  per-format quirks that never feed back (content→attribute markers
  project onto the ENCLOSING element; USX `eid`s are derived during
  iteration — never-synthesize governs tokens, not projections; HTML
  wrappers + kind-keyed NoteCaller rendering; heading base levels are an
  authored aux table). usx.md holds the six content→attribute target
  names still to be read off their marker pages. vref is the ordering
  lane's sibling, not a fold.
- **The sous-chef slab (TOC file) is an EXPORT** (2026-08-18, contract in
  scripture-sous-chef-2/documentation/design.md): four flat u32 arrays —
  book header, chapter rows {file span, mask range, anchor range,
  checksum}, mask spans, verse anchors. Decomposes onto existing
  artifacts: chapter rows = ParseHeader runs, anchors = Designator token
  starts, mask spans = text_runs ∩ runs with Note-frame extents
  subtracted (CST). TWO corrections carried back to that design: rows
  are keyed by ORDINAL (label is a span in the row — the producer never
  refuses a messy file; duplicate/out-of-order `\c` is a LINT finding
  beside the TOC, and refusal is sous's own policy gate), and the
  CHECKSUM (xxh3-128 over masked content) is computed by the slab
  exporter, not the core — no hash dependency in the engine. Sous's
  INCREMENTALITY is memoization, not mutation: the stateless producer
  emits the whole slab per call (the cheap pass); sous's content-
  addressed stats store skips its own expensive walk on checksum hits.
  No session, no invalidation protocol — the checksum IS the protocol,
  and it only works BECAUSE the producer is pure and total per call.
  Consumption pattern: walk chapter rows IN TOC ORDER, reuse by hash —
  order gives position, hash gives identity; the row's ordinal + label
  span + verse anchors are the human sid, derived at read time. The
  vref export is this same artifact at the degenerate mask granularity
  (one span per chapter). (Why memoization can be architecture for sous
  while staying an escape hatch for the engine:
  ideas/committed/chunk-memoization.md.)
- **The attribute interpreter** (k/v view over an `AttrList` span: quoted
  values, default attribute, comma/colon splits) is the designator
  interpreter's sibling — pure span → judgment, shared with exports/lint.
- **`\z` custom markers are CONFIG-provided** (markers.ext shape; row 0
  already gives zero-behavior-unconfigured). Config must handle `a-*`
  prefix wildcards and the `standalone` category (no new Category
  variant needed).
- **`assign_marker_indices` is PARKED, unbuilt** — only caller would be
  a persistence path deserializing rows across a table-version change;
  ~10 lines on the scanner's resolver if that day comes.
- **Perf rules**: stop density is the wall — speed = emitting fewer
  tokens; arms stay `#[inline(always)]`; hand a found needle forward.
  A second pass over token rows is cheap (~1 ns/token floor, measured on
  ParseHeader; `playground --parse-header-only` prices any pass), so the
  CST and lint never need to fuse into the scan. Chapter-par only pays
  on big books. Criterion only when two real alternatives exist.
  One-load SWAR marker-path spike: see investigate-later.md.

## Parked (do not start)

Braid — collapsed twice: first from identity-tracking to a sync layer
(id-stability-across-edits stopped existing as a problem in the CM
world), then (2026-08-18) possibly to NOTHING but "call the stateless
`analyze`, debounced" — the buffer-sync machinery is the demoted
BookSession optimization, and chapter-scoped re-lex is a
measured-someday, not a design input. What remains genuinely braid's if
anything: multi-book session concerns. Multi-book project format,
diff/merge (onion's diff is trusted — expect a nearly wholesale port),
publish, anchors/U25002/`aid`.

## Whitespace tokenization: two rules, no exceptions (2026-08-21)

The grammar's delimiter class is any reducible-WS run ([\t\n\r ]+); we
honor it under two orthogonal rules rather than one uniform costume:
1. A NEWLINE IS ALWAYS ITS OWN TOKEN — block seams, lint line
   discipline, \n\c chunking, and the editor's line model all key on it.
2. A HORIZONTAL delimiter run (space/tab) FOLDS into the token that
   grammatically requires it (marker names; and each carved payload —
   designator, note caller, book code). CR belongs to the newline
   machinery.
The delimiter always exists in the stream and always concatenates back
(partition); which rule represents it depends only on its characters.
A Roslyn-style trivia channel was considered and rejected: trivia buys
fidelity when bytes would otherwise be lost — we never lose bytes, so
it would cost a token kind in every consumer for zero information.
