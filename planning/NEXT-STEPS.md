# Next steps

Slow on purpose. Plain language so this can be picked up cold after a break.
Vocabulary lives in GLOSSARY.md; the lint and braid layer designs in
ideas/committed/. This file is the open queue, "what code to write next, in
what order", and the settled design the code doesn't yet embody.

## Where we actually are (2026-08-12)

- **Done**: token/scanner split (8-byte row, boundary finders own the
  cursor, `classify_marker` position-free) · perf experiments banked in
  `src/experiments/` (see Benchmarks under Later) · the marker table
  (`src/tables/rows.rs`, 151 rows, idx 0 = empty row) · codegen
  (`src/tables/emit.rs` → `generated.rs`, checked in, freshness-tested;
  one u128/row) · partition oracle (`tests/partition_oracle.rs`:
  contiguous spans, `concat(spans) == source`, 226 corpus books) ·
  spec-diff (`planning/spec_contexts_diff.py`, needs a tcdocs clone).
- **The next CODE step is 4.1.** Nothing in the scanner reads the table yet.
- The design rulings ([A]–[P], Q16, the axis split, adjacency) are ENCODED —
  in schema.rs's types and doc comments, rows.rs values, and the sections
  below. The code is the record; there is no separate ledger to consult.

## Open queue

Delete items as they're ruled; rulings land here / in the schema.

- **R1 — Read emit.rs, then commit the tree (Will, in progress).** 13
  modified + 4 new files from the 2026-08-12 sitting, uncommitted.
  Suggested split: table promotion / generator / row fixes / planning
  docs. Everything else waits on this baseline.
- Parked (not open, just not lost): Q-A7/`aid` with U25002; anchors.

## Standing laws

- **Partition oracle is not negotiable.** If a change wants to break
  `concat(spans) == source`, that's a design event — stop and log it.
- **Rows change only through the spec-diff.** 6 of 7 row edits made by
  inference (2026-08-12) were wrong; the diff caught all of them. The
  MARKER PAGE is the referee — the doc index, the RNG grammar (it
  describes USX), and the pages themselves disagree in spots; spec
  fuzziness is absorbed by LINT SEVERITY, never by inventing table values.
- **Never synthesize tokens.** usfmtc fabricates an implicit `\p` when
  `\v` follows `\c`; we cannot — no span to give it. Flag, never repair.
  Every place the reference implementation normalizes is a place we lint.
- **Go slow** (Will, 2026-08-10): one behavior at a time, each behind the
  oracle + playground verify, each with its perf delta read before the next.

## 4. Wire into the lexer — SLOWLY, in this order (**START HERE**)

1. **marker_idx assignment only — DONE 2026-08-12.** Markers stamp their
   row via `resolve_marker_idx`; spans/kinds verified byte-identical vs
   the frozen variants. Perf: 1354–1377 → 962–975 MiB/s (−29%): ~5ns per
   marker resolve against a ~5ns/token budget. Accepted; step 4's
   `common_marker_checks` is the designed claw-back (suspects if it ever
   needs profiling: the sparse-u64 `by_name` compare tree + `digits_ok`'s
   second PACKED read).
2. **Per-class ws fold — DONE 2026-08-12** (killed the ScanMode TODO;
   `marker_arm` reads `ws_after_name` off the row). Perf: no measurable
   cost over 4.1 (980–988 MiB/s). DESIGN EVENT, logged as expected:
   token boundaries changed — closers (`\w*`) and `\*` never absorb
   their trailing space (content, whatever their shared row says; kind
   gates it), and unresolved markers (row 0 = NotRequired) absorb
   nothing — their following space is content and OPENS the next text
   run (one Text token; the stream never emits Text + Text). The
   partition oracle held throughout (boundaries moved, bytes didn't).
   The frozen `src/experiments/` variants now legitimately differ from
   `crate::lex`; the playground verify was downgraded to asserting each
   variant stream is itself a lossless partition.
3. **Payload mode, narrow — DONE 2026-08-12.** `TokenKind::Designator`
   (9th shape; NESTED_BIT slid to bit 4), `pending_designator` mode flag
   set from the row's payload column, consumed by the text arm as ONE
   span (happy digits, ranges, junk alike — the interpreter judges).
   A designator-less `\c`/`\v` emits nothing extra. Perf: 985 →
   1145–1192 MiB/s — a GAIN (verse numbers stopped paying the SIMD
   text-run setup for a 1–3 byte token). NoteCaller rides this same
   machinery next.
4. **`common_marker_checks` — ALL 9 ARMS DONE 2026-08-12** (v q p s f b
   ft fr xt, the measured cut). `lex` is `lex_impl::<FAST>`;
   `lex_general_path_only` (fast checks compiled out) is the definition,
   and `tests/fast_path_identity.rs` pins every arm token-identical to
   it over all 226 corpus books. Rows + caps + fold classes resolved
   ONCE per lex (`HotIdx`); every arm is conservative — off-shape
   (`\+q`, `\q1a`, `\s5`, `\v  1`, `\f*`, `\fq`) falls to the general
   path. Arms fuse: name + delimiter run (fold read off the row —
   the identity test caught `\b`'s non-folding class on the first run),
   `\v`'s digit designator, and the marker's own line ending (`\n` and
   `\r\n` alike). Digit sweep is a scalar loop — 1–3 digits, SWAR has
   nothing to chew. Perf: 968 (post-4.1 low) → **~1730 MiB/s** prose,
   26%% ABOVE the pre-table baseline (1354–1377); aligned (en_ult,
   zaln-dominated, arms rarely hit) unchanged ~1150.
5. **USV escapes** (`\uXXXX`/`\UXXXXXXXX`, U25004: a text-arm escape
   fold — today a literal backslash-u escape in source lexes as an unknown
   marker and triggers
   pop-all recovery) **+ the region-start escape dispatch fix** (the
   pre-existing `\~`-at-region-start bug). Small, self-contained, can
   land anywhere after 4.1.

## 5. Walker + attributes (LAST — the complexity we deliberately deferred)

The walker design is SETTLED (distilled here from the retired
TRANSITIONS.md, 2026-08-12); what remains is writing it, in the same
go-slow order: stack first (no consumers), then AttrList, then the
context lane. Milestone sid/eid, note callers' full behavior, and the
lint listener live HERE, not in step 4.

### The walker

- **One generic loop**: frames are stamped with their context at push
  time (`frame.ctx = row.contributes_context() or inherited` — Character
  frames are transparent). On a marker:
  `pop_while(top frame's stamped context ∉ row's context_mask)`, then
  push if `opens_scope`. That single predicate reproduces every
  displacement: `\c` unwinding a note and a paragraph (its
  `[ChapterContent]` forbids both), `\p` closing the previous `\p`,
  `\pb` leaving its paragraph alone, nested `\add`.
- **Dead ends, do not re-derive** (tombstones in schema.rs): a PARENTS
  table and a rank/`precedence()` value were both built and deleted —
  the stamped-context predicate already does their job.
- **Two hand rules** (~10 lines each): `\X*` searches the stack for the
  Note|Character frame with the matching `marker_idx`, pops through;
  `\*`/`\esbe` pops the topmost frame of the kind it closes.
- **Two data-keyed clauses** (predicate arms reading existing columns,
  not marker lists): a `ScopeKind::Sidebar` frame is a POP BARRIER —
  only `\esbe` closes it, a `\c` inside `\esb` stops and lints; an
  incoming `closing == OptionalExplicitUntilNoteEnd` marker first pops
  an open frame of that same class (note peers — `\ft` then `\fq` are
  siblings, and the column marks exactly the 19 real peers).
- **Only rows that DISPLACE run the pop predicate** — a derived boolean
  (`opens_scope.is_some() || kind ∈ {Chapter, Verse}`), not a rank. The
  mask serves legality AND displacement, and they diverge exactly on the
  empty-mask adjacency rows (`ca` must never pop the stack).
- **Milestone spelling overrides the table**: any token the lexer shaped
  as `\name-s/-e` is scope-kind Milestone whatever the row says — that's
  what pairs an unknown `\zaln-s` with its `\*`.
- **Displacement pops are LINT EVENTS**, not silent ("let them know we
  closed the footnote — it's supposed to close explicitly").

### Positional context (the other half of legality)

- The positional band (`Scripture → … → ChapterContent`,
  `SpecContext::is_positional`) is MONOTONIC and needs no new data: the
  ordering is the enum declaration order, the transitions are
  `allowed_contexts`. Rule: stay if current is allowed, else advance to
  the lowest allowed context above current, else it's behind us → lint,
  don't move. Two instructions: `mask & !((1 << (cur+1)) - 1)`, then
  `trailing_zeros()`. No marker "enables" regions — skipping forward is
  what optionality means, so `\id` needs no special case.
- Markers listing TWO positional contexts are the spec saying "I appear
  at two points" (`mt#`, `cl`, `ip`); lowest-above-current resolves all
  three correctly. `cl`'s dual semantics (book-wide label before ch.1,
  that chapter's label after) falls out.
- `ca`/`cp`/`va`/`vp` are NOT context questions: adjacency lint rules of
  shape (lastMarker, token) — see linter.md. Their rows carry an empty
  context slice; the context machine abstains.

### Attributes (3.2, from the retired attributes doc)

- BOTH forms, one `AttrList` token kind: node-initial (U25001, both
  pipes, zero scope state) and legacy trailing (needs the stack).
  Disambiguation is the three-rung pipe ladder: closing pipe →
  node-initial; closing marker → legacy trailing + deprecation lint;
  neither → content + lint hint. A pipe terminates a marker name
  universally.
- Defined attributes + defaults are table columns; `x-`/`z-` attributes
  are legal on ANY character marker (kind-level fact). Conditional
  cardinality (sid/eid pairing) is lint's business — linter.md.
- `\z` customs are CONFIG-provided (markers.ext shape); zero behavior
  unconfigured. Unknown/illegal markers: recovery = pop all, start fresh.
  Two 3.2 facts the config shape must handle when designed: attribute
  PATTERNS (`a-*` wildcards, so config attrs aren't a plain name list) and
  the `standalone` category (maps onto no-scope + no-payload + no-ws; no
  [`Category`] variant needed).

## Later (when we're actually writing code again)

- **Verse designator interpreter** — the spec `VERSE` pattern
  (`/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/`); pure text
  rules, zero table. Writing its doc-comment IS writing the comparison
  rules lint/vref need.
- **Exports / render surfaces** (from the retired html-elements doc):
  three surfaces only — the marker-keyed `html_element` column, the
  token-KIND-keyed NoteCaller rendering, and SCOPE-DERIVED containers
  (list/table/chapter wrappers are synthesized around scopes, unreachable
  from any row). Heading base levels are an authored aux table; data
  attributes always verbose.
- **Structure events → editor tree.** Measure the wasm route FIRST before
  building any JS twin.
- **Observation (lint finding) shape**: `{ code, anchor token, optional
  second token }` — severity/category/template in a rules table; message
  rendering on the consumer's side. Audit onion's messageParams first.
- **Benchmarks**: playground timing until there are two real alternatives;
  criterion then. Banked (2026-08, src/experiments/): serial 1.3 GiB/s
  prose / 1.15 aligned; scalar floor 441 MiB/s; chapter-par hits the
  machine ceiling on big books, loses on small ones — measured size
  threshold, whole-project-open tool only. The `--chunked` verify
  (token-identical split at `\c`) is the slot model's independence proof.
  Two-stage indexing (the simdjson trick): tried, a wash — cost is token
  pushes + arm logic, so speed comes from EMITTING FEWER TOKENS
  (AttrList). The stop-cost ladder (`experiments/sweeps.rs`, 2026-08-12)
  measured the granularity price directly: the full stop set alone runs
  ~1.7 GiB/s vs a 16+ GiB/s one-needle ceiling — stop DENSITY is the
  wall. Post-4.4, prose full-lex BEATS its own scan skeleton (fused arms
  skip memchr restarts), so prose is done; aligned still leaves ~30%
  inside the stops, recoverable only by fewer tokens (AttrList, `\z`).
- **Feature-gate exports to shrink the wasm bundle** — wasm doesn't
  tree-shake; combos = separate build artifacts, JS wrapper
  dynamic-imports the right blob.
- **Codegen USJ types** as the product of Token × marker table. MUST use
  official USJ type names (`para`, `char`, `note`, `ms` …) — check the
  USJ JSON schema in usfm-grammar before naming.

- **Spike candidate: one-load marker path** (2026-08-12, unbuilt). The
  general path walks the same ≤8 bytes three times (`marker_end`,
  `classify_marker`, `resolve_marker_idx`'s stem scan + u64 key build).
  One 8-byte load + one SWAR alnum mask + ctz could feed all three:
  end position, suffix byte (`*`/`-s`/`-e`), and the zero-padded name
  key from a single register. NOT a straight SWAR win — the alnum walk
  is 1 iteration for the Zipf-common names, so fixed-cost masking
  (~12-16 ops) loses to the loop there; the win, if any, is
  DE-DUPLICATING the three walks. Only worth a spike AFTER aligned's
  fewer-tokens work (AttrList, `\z` arm) lands, since the hot arms
  already bypass all three walks on prose. Spike first, per standing law.

## Parked (do not start)

Editing/session layer (design: ideas/committed/braidv2.md), multi-book
project file format (toc, checksums, caches), diff/merge, publish. Each
gets designed against this base once it holds.
