# Next steps

Slow on purpose. Plain language so this can be picked up cold after a break.
Design state and open questions live in QUESTIONS.md / GLOSSARY.md; this file
is only "what code to write next, in what order."

## Done (so cold pickup doesn't re-plan it)

- Core structs stubbed and split: `src/token.rs` (8-byte row, kind_bits
  codec), `src/scanner.rs` (arms split: boundary finders own the cursor,
  `classify_marker` is position-free). 10 tests green.
- Perf story priced and banked in `src/experiments/` (see "Benchmarks"
  under Later). Real fix shipped: prebuilt memmem Finder (+14% prose).
- Table scaffolding: `src/tables/{schema,rows,unaudited}.rs` +
  `src/bin/codegen.rs`. Mechanical translation DONE (2026-08-10): 238
  onion spellings → 162 canonical rows in `unaudited.rs`, 22 families
  collapsed, 14 flags (printed by `cargo run --bin codegen`).
  `rows.rs` is the audited-only file and is still EMPTY.
- `planning/TRANSITIONS.md`: the context-machine analysis. Verdict being
  refined (see step 2 rulings below).
- **Partition oracle** (2026-08-12): `tests/partition_oracle.rs` —
  standing test that every token starts where the previous span ended
  and the last ends at EOF (implies `concat(spans) == source`), over
  every `*.usfm` under example-corpora (226 books, all green; skips
  loudly if corpora absent — they're gitignored). If a change wants to
  break it, that's a design event — stop and log it in QUESTIONS.md.

## 2. Table audit + schema rework — DESIGN DONE, audit pending

STATUS (2026-08-10, after ten agent rounds): every ruling below is
APPLIED to schema.rs/unaudited.rs and every later flag is ruled — see
the category ledger (`cargo run --bin codegen`), TRANSITIONS.md's
ruling ledger, planning/attributes-3.2.md (USV + node-initial
attributes, incl. the Q-A6 three-rung pipe ladder), and
planning/html-elements.md (render classes, all closed). 154 canonical
rows sit in unaudited.rs awaiting the audit; the only open items
anywhere are spec-side residues and parked proposals. What remains of
this step is H: move rows into rows.rs category-by-category
(paragraphs → characters → notes → milestones → meta/periph), verified
against the 3.2 docs; delete unaudited.rs when the last row moves.
The rulings are kept below as the record:

- **[A] `opens_scope: Option<ScopeKind>` column** on marker rows; the
  precedence machine keys on scope kind in a small AUXILIARY table
  (~13 rows), not on marker rows. Spec source pasted in
  planning/scratch.md (usfm 3.1 doc index).
- **[B] `closes_scope` column** (retires onion's `\esbe` phantom frame).
- **[C] Category restructure**: replace the four accreted fields
  (paragraph_category, note_family, note_subkind, inline_context) with
  the spec's own two levels — `kind` (Para/Char/Milestone/Note/Sidebar/
  Periph) × `category` (Para → Identification·Introductions·
  TitlesSections·Body·Poetry·Lists·Tables; Char → TextFeatures·
  Formatting·Breaks·Intro·Poetry·Lists·Tables·Notes; Milestone →
  list·table·qt·ts·vid; Note → Footnote·CrossRef). Fine category is
  load-bearing (`\pb` is Char but Breaks → opens no scope). Stub for
  eyeballing.
- **[D] `s#` ws ruling: TagEndDelimiter** — s is a paragraph marker;
  takes space-or-TAGEND like any para. (Spec patterns for TAGEND/ws/hs/
  HS/Hs/nl recorded in scratch.md.) Also: `s` IS numbered; bare form
  legal only when a single level exists in the text — that
  bare-vs-numbered rule is spec-wide and is LINT's business, not the
  matcher's (bare is always a valid spelling).
- **[E] Payload column settled**, plus: `\usfm` gets a Version payload;
  the NumberRange grammar is pinned to the spec pattern
  `/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/` — that
  pattern belongs to the verse-designator INTERPRETER (Q2); the scanner
  fast path only ever handles the pure-digit happy shape.
- **[F] `\z` customs**: extension definitions are CONFIG-provided (the
  markers.ext shape, never read from a file); zero behavior when
  unconfigured. Unknown/illegal markers (`\s5`): recovery = pop all the
  way out, start fresh.
- **[G] Matcher strips `-s`/`-e` BEFORE digits** (`qt3-s` → `qt3` →
  `qt`); milestone side is read off the span, rows stay collapsed.
- **[H — OPEN, delegated]** precedence encoding: generic
  `pop_while(predicate)` driver vs onion's fixed 3-pass. Suspicion: the
  3-pass is a fossil; verify against `apply_open_precedence` and present
  options.
- **[I] Note recovery folds into the generic driver**: stamp
  `effective_context` on each stack frame at push time
  (`frame.ctx = row.contributes_context.unwrap_or(parent.ctx)`); then
  "may `\q2` appear here?" is top-of-stack + row mask, and recovery is
  `pop_while(frame's context forbids this marker)`. No stack walk ever.
- **[J] Two hand rules accepted**: `\X*` name-matched pop and `\*`
  kind-matched pop (bounded stack SEARCH through possibly-unclosed inner
  frames — genuinely not a per-row value, ~10 lines each).
- **[K] NoteCaller becomes a TokenKind** (10th shape; kind_bits has room
  after the nested-bit slide). The `+`/`-`/`?`/custom caller after
  `\f `/`\x ` is payload-shaped — same pending-payload machinery as
  NumberRange; note kinds get a caller payload in the table.
- **[L — OPEN]** lint/context modeling. Recorded constraint: lint must
  be correct over `Vec<Token>` alone — anything context-shaped arrives
  as a token-stream property or explicitly attached emission, never a
  reach into live parser state. Editor contract: Token {id, kind, text}
  is ALL an editor supplies; no round-trip through usfm text to answer
  context questions.
- **[M] Single-pass vision stands**: the fused pass produces a
  valid/recovered/diagnostics-aware result in ONE traversal. Layering
  survives INSIDE the pass: arms own position, walker owns policy,
  walker is listener-gated so the plain-tokens path remains the oracle.
- **[N] `takes_attributes` grounded in spec**: defined attributes on
  jmp(href,title,id) · rb(gloss) · w(lemma,strong,srcloc) ·
  ref(loc,gen) · fig(alt,src,size,loc,copy,ref); milestones may define
  them (qt: who). User-defined `x-`/`z-` attributes are legal on ANY
  character marker (kind-level fact, not a column) and are
  non-canonical.
- **[O] Table cells (`\tc1-2` spans)**: match the alpha stem (`tc`),
  hand the digits/span details to that marker's interpreter over the
  span. Numbering doesn't model spans; the matcher doesn't parse them.
- **[P] `//`**: stays TokenKind::OptBreak, never resolved by name — the
  row leaves the table.
- **Numbering caps per spec**: lim 1–4 · sd 1–4 · ph 1–3 · h 1–3 but the
  numbered `h#` syntax itself is DEPRECATED (keep cap + deprecated note).

Then the audit itself: move rows `unaudited.rs` → `rows.rs`
category-by-category, verified against spec docs (tcdocs/ + scratch.md);
delete `unaudited.rs` when the last row moves.

## 3. Codegen emissions (NEXT — the immediate next coding step)

`cargo run --bin codegen` reads **rows.rs only** (the audited file) and
emits `src/tables/generated.rs`, checked in:

1. The packed row table (u128-in-spirit; u64+u32 lanes vs u128 vs
   windows is the GENERATOR's choice) + typed accessor fns — no
   consumer ever touches bits.
2. `marker_idx(name)`: strip `-s`/`-e`, strip digits, load ≤8 bytes as
   a u64, integer match; digits validated against Numbering;
   SpellingShape picks between the two `qt` rows; first-byte-`z` /
   no-match → 0.
3. Side arrays: idx→name (the reverse lookup — this is what makes the
   table double as the MARKER CATALOG), per-marker defined_attributes
   + AttrStatus, default-attribute, html element ids.
4. Derived aux: context bitmasks from the authored slices, baked
   contributes_context values, V_FORBIDDEN as indices. The
   hand-authored aux tables (PARENTS scope masks, heading base levels)
   stay authored; codegen may re-emit them packed.
5. Freshness test: regenerate to a buffer, compare to the checked-in
   file, fail if stale.

Because codegen reads rows.rs, emissions GROW as the audit moves rows;
an unmoved marker resolves to idx 0 (same graceful path as a custom) —
partial is never wrong, only incomplete. NOT in this pass: the JS/TS
registry and USJ types (they come with the wasm/JS boundary work) and
`common_marker_checks` (step 4.4 — measured, never speculative).

## 4. Wire into the lexer — SLOWLY, in this order

The go-slow rule, stated as law (Will, 2026-08-10): do NOT turn on
attrs + all markers + milestones + every ruled behavior at once. The
architecture is "given a marker, ask the table the right questions" —
but the questions get ASKED one at a time, each behind the partition
oracle and the playground verify, each with its perf delta read off
the playground before the next lands.

1. **marker_idx assignment only.** Tokens gain real indices; nothing
   else changes. Verify output vs current lexer (only marker_idx
   differs), bench.
2. **Per-class ws fold** (kills the ScanMode TODO — reads ws_after_name
   off the row). This CHANGES token boundaries for closing markers;
   design event per the oracle rule, expected and logged.
3. **Payload mode, narrow**: pending-payload flag + NumberRange token
   kind for the `\c`/`\v` family ONLY (slide NESTED_BIT to bit 4 here).
   NoteCaller rides the same machinery next once NumberRange holds.
4. **`common_marker_checks`** (renamed from "prelude"): the fused
   fast checks that collapse a hot marker's several arm passes + token
   pushes into one masked-compare hit — `\v `+SWAR-digits first, then
   one arm at a time in measured-priority order
   (planning/marker-frequencies.md), keep an arm only if its delta
   clears run-to-run noise. General path stays the definition;
   oracle-verified per arm.
5. **USV escapes + the region-start escape dispatch fix** (the ruled
   [A] behavior + the pre-existing `\~`-at-region-start bug) — small,
   self-contained, can land anywhere after 4.1.

## 5. Walker + attributes (LAST — the complexity we deliberately deferred)

Everything ruled but nothing built: the structure driver (open-marker
stack + opens/closes_scope columns + the pop_while driver + the two
hand rules), fused into the pass, listener-gated. Then AttrList — both
forms, the Q-A6 three-rung pipe ladder, one token kind, deprecation
lint on trailing. Then ParseHeader emission (book span + chapter run
table) rides along. Same go-slow discipline: stack first (scopes
open/close, no consumers), then AttrList on top of it, then the context
lane. Milestone sid/eid, note callers' full behavior, and the lint
listener all live HERE, not in step 4.

## Later (when we're actually writing code again)

- **Verse designator interpreter** — the [E] spec pattern; pure text
  rules (digits, ranges, suffixes, junk → "not cleanly numeric" flag).
  Zero table. Writing its doc-comment IS writing the comparison rules
  lint/vref need.
- **Structure events → editor tree.** Measure the wasm route FIRST before
  building any JS twin; consumer-sufficient kinds may have shrunk this
  problem a lot.
- **Observation (lint finding) shape**: `{ code, anchor token, optional
  second token }` — severity/category/template live in a rules table;
  message rendering and localization happen on the consumer's side. Audit
  onion's messageParams first to confirm nothing non-derivable is lost.
- **Benchmarks**: playground timing until there are two real alternatives
  to compare; criterion then. Baselines banked (2026-08, src/experiments/):
  serial 1.3 GiB/s prose / 1.15 GiB/s aligned; scalar floor 441 MiB/s (no
  SIMD is still fine — wasm worst case covered); chapter-par hits the
  machine ceiling (~4 GiB/s) on big books, LOSES on small ones — if ever
  shipped it's a measured size threshold, and only a whole-project-open
  tool. The `--chunked` verify (token-identical split at `\c`) is standing
  evidence that no scan state crosses a chapter — the slot model's
  independence claim, proven on both corpora.
- **Two-stage structural indexing (the simdjson trick): TRIED, a wash**
  (+6% prose, −4% aligned; the position tape's memory traffic eats the
  savings at our thin per-token work). Conclusion kept: cost is token
  pushes + arm logic, so speed comes from EMITTING FEWER TOKENS
  (AttrList), which is the consumer-sufficiency direction anyway.
  ~1.1–1.3 GiB/s/core is what this granularity costs; accept it.
- **Feature-gate exports to shrink the wasm bundle** (e.g. no USJ/HTML/XML
  export unless built with it). Cargo features work the same for wasm
  targets; compiled-out code never enters the .wasm. A wasm blob does NOT
  tree-shake like JS, so features matter MORE here; combos = separate
  build artifacts, JS wrapper dynamic-imports the right blob.
- **Codegen USJ types** as the product of Token × marker table: the
  discriminated union + serializer fall out of the same generator as the
  JS registry. MUST use official USJ type names (`para`, `char`, `note`,
  `ms`…) — check the USJ JSON schema in usfm-grammar before naming.

## Parked (do not start)

Editing/session layer (live ids, re-scan on edit, lint anchoring while
editing — design state lives in QUESTIONS.md Q9), multi-book project file
format (toc, checksums, caches), diff/merge, publish. Each gets designed
against this base once it holds.
