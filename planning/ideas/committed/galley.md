# Galley — the workflows crate (piece 5 of 5)

STATUS: NOT YET DISCUSSED with Will — this document organizes the raw
material for that conversation. Sources: Will's voice transcript
(2026-08-25, teased apart below), the Sefer v2 vision
(../scripture-editor-proto-2/plans/editor-v2-rewrite-vision.md), and
the braid-v2 sketch this file replaced (Lexical-era; what survives is
compressed at the bottom, the original is in git history as
ideas/committed/braidv2.md).

## The five-crate layout (ruled 2026-08-25)

1. **onion** — the Rust engine (this repo's library).
2. **onion-wasm** — bindgen + .d.ts + JS-environment utilities
   (UTF-16 walls, wrappers). This is `onion-wasm/` in this repo —
   renamed from `galley/` in pass 10 (2026-08-25), which frees the
   name for piece 5 below.
3. **sous** — the proofreading Rust engine (sibling repo).
4. **sous-wasm** — same as 2, for sous.
5. **galley** — an OPINIONATED set of higher workflows over 1/3
   (natively) or composed into one wasm binary with 2/4 (the vision's
   "composed analysis host", §4.4/§9.4). Dirty-marking, normalizing,
   checksumming, ingest recipes, find, onion↔sous coordination —
   anything that might be considered stateful lives here and NEVER in
   the engines (§9.2: "one narrow seam above the pure domain fns").

## The transcript, teased apart

### For sure (stated or strongly implied)

1. **Sous implementation under the new regime is the next major
   scope.** Rule of thumb from measurements: ~2x on wasm depending on
   the work.
2. **Find lives in galley** (confirms ideas/candidates/
   find-and-overlay.md): memmem for first literal search; the recipe
   is mask (verse_text) → search the projected plane → map hits back
   into the caller's coordinate system (utf8/16, chapter-relative or
   absolute). **aho-corasick for multi-needle sweeps** — the hygiene
   sweep / termlist case searches many needles at once, which is
   aho-corasick's exact shape (it rides on memchr, same maintainer).
3. **Sous's corpus input needs a shape proposal.** Lean from the
   transcript: an array of triples — (label, text, …) where label is
   a book identifier / file path used only for association, never
   read by the engine. Everything mask-shaped stays onion's; sous
   consumes projections.
4. **Galley probably re-exports the onion surfaces** and adds the
   higher-level ops on top — open how much is pure re-export vs
   curated.
5. **Coordinate MODES**: galley hands out a handle/mode that fixes
   the coordinate contract — ABSOLUTE (whole-file offsets) or
   PER-CHAPTER (chapter-relative offsets, the chapter assumed to be
   the whole document). The consumer picks; galley owns the adapter;
   any UI choosing per-chapter owns its chapter-swap thinking.

### Maybes (need measurement or a design ruling)

6. **Caching / checksumming / dirtiness.** Performance so far says
   retention may be unnecessary — "always run the whole book" is the
   standing default (vision rung 1). The checksum-keyed KV cache of
   derived products is exactly vision rung 2 (content-addressed
   reuse). Open: is the complexity ever bought? The checksum EXPORT
   itself (xxh3-128-v1, vision §13.4) was deferred out of onion-wasm
   and lands in galley when save/dirty machinery exists.
7. **Monoid / map-reduce shape of the engine products.** Can CST /
   lint / toc be expressed as a fold over chunks so one edited chunk
   recomputes and the rest just shift offsets (prefix sums)? Toc is
   friendly (it tiles every byte). CST and lint were the open
   question — now assessed: see "Monoid-friendliness: what it would
   take" below. NOTE: this is rung-3 research (vision: "workers, IPC,
   splice replay, monoids before measurement" is on the REJECTED
   list) — it stays a question, not a plan, until a measurement
   fails. The braid-v2 fold-over-slots sketch is the prior art.
8. **Chapter chunking.** Current speeds say pre-split/pre-scan is
   overhead for onion — but sous may WANT chapter-grain inputs, and
   chunking is a galley concern either way. Sub-question: loop
   overhead of feeding lex one chapter at a time + chapter offsets +
   a file-level prefix. The chapter-independence proof exists
   (experiments/chapter_par.rs: token-identical split at line-initial
   `\c` across both corpora).
9. **Diff granularity + wire.** Always diff the whole book, or mark
   chapter segments dirty? And: should a whole-book diff RETURN
   unchanged segments? (Partially answered already: Unchanged units
   cost ZERO replay splices — a one-verse change returns ~a few
   splices, never the book — but the SKELETON does narrate unchanged
   units; whether the wire should elide them per-chapter is a
   consumer question for the modal.)
10. **Product caching for sous and find** — the recurring tension:
    "performance has suggested we don't have to retain, and maybe
    that's true" — non-retained until a real consumer measurement
    says otherwise.

### Pain points galley exists to solve

- The editor's dirty/save machinery needs checksums and a dirty
  answer that doesn't retain a second full string (§13.4).
- Coordinate multiplicity: utf8 vs utf16 × absolute vs
  chapter-relative — one adapter, owned once (§6.2, §10.3).
- Sous needs corpus-shaped ingest + mask recipes (the "chop it,
  mask verse text, feed sous, map back" pipeline).
- Whole-project operations: analyze every target book on open,
  aggregate ProjectDiagnostics (§11.1 / Horizon 2).

## Monoid-friendliness: what it would take (assessed 2026-08-25 at 04a91df)

Answers item 7's open question by reading what each stage actually
threads across a line-initial `\c`. Two framing rules first:

- **Adoption is per-subsystem and opt-in.** Nothing here obligates
  lint or CST to BECOME folds — the minimal useful rung is `\c`
  checksum chunks in galley for SOUS reuse alone (chop, checksum,
  reuse the chunk's sous product on checksum hit), which touches no
  onion product at all. Each level below stands on its own.
- **The oracle is the law**: a product built per-chunk in isolation
  and reduced must equal the fresh whole-book product, byte for
  byte. That is the standing retained-vs-fresh equivalence test any
  chunked path must keep green, and it is what makes every level
  testable before it is trusted.

### The carry analysis, stage by stage

The flat-arena architecture (every product a vec of u32 indices /
byte offsets) makes "compose two chunk products" almost everywhere
SHIFT-BY-PREFIX-SUM + CONCAT — `chapter_par.rs`'s `rebased` is the
existing proof of the shape. The obstruction is never state size;
it is BOUNDARY LEAKAGE, captured per stage as a `Carry`:

- **Lex: zero carry, proven.** The split point sits after a newline,
  which ends every run and clears delimiter-ws mode; token-identical
  across both corpora (experiments/chapter_par.rs). Chunk products
  stored chunk-relative rebase by one add.
- **CST: carry = the open scope stack at the boundary.** Root-only
  on well-formed input (paragraphs close implicitly at `\c`), so the
  carry is empty in the overwhelming case. What dirties it is
  exactly what lint flags anyway: a sidebar straddling a chapter
  (`\esb … \c 1 … \esbe`, ancestry.rs's own example) or an unclosed
  `\f` running past a `\c` — a per-chunk build would close those
  Eof/Recovery at the chunk end and diverge. Rule: a chunk product
  records whether its boundary stack was root-only; a dirty carry
  fuses that chunk with its neighbor and recomputes the pair.
- **Lint: four machines, all small carries.**
  - *Ordering* — the one self-declared cross-token machine, but
    verse state RESETS at every `\c` by design, so the chunk
    boundary is already its reset point. Chapter state is a
    textbook semigroup: summary = (first chapter number, last
    chapter number); duplicate/out-of-order/gap judged at the merge
    seam between adjacent summaries.
  - *Ancestry* — two stack-derived depths (zero at a clean
    boundary) plus `run_reported`, a one-bit carry for a
    paragraph-less verse run straddling `\c`.
  - *Structure* — one pending token plus the empty-paragraph run
    list; a run straddling a chapter end is a small carry.
  - *Flat* — the genuinely whole-book state: the positional `band`
    (one byte, monotonic — trivial carry-in) and the numbered-family
    `levels`/`first_seen` arrays (~2KB of bitmasks composing by
    OR/min, mixed-family re-judged at merge).
  - *Emit/finish* — observations sort by (anchor, code), so
    per-chunk reports are pre-sorted and merge by concat; fix
    indices and edit_list offsets shift like everything else. The
    `finish()` rules (missing chapter, verse-before-first-chapter,
    empty-verse collapse) run once, on the merged summaries.

### The design shape, if a measurement ever promotes it

Per-chunk product + `Carry` (boundary stack state, pending runs,
Flat's masks, Ordering's chapter summary). Clean carry → compose by
shift+concat; dirty carry → fuse neighbors and recompute. Cache key:
a chunk's product is a function of (bytes, carry-in), not bytes
alone — so strictly (checksum, carry-in), degenerating to the bare
checksum whenever the carry-in is the default one, i.e. almost
always. Chunk 0 (everything before the first `\c`) is the special
one: it owns `\id`/headers/intro and establishes the version and the
positional band the chapters inherit.

### Adoption ladder (each level independently testable)

0. **Sous-only chunk checksums** (galley, no onion change): chop at
   `chapter_chunk_starts`, checksum chunks, reuse per-chunk sous
   products on hit. Test: chunked-and-reduced findings == fresh
   whole-corpus findings.
1. **Lex chunk reuse**: cache chunk-relative token vecs keyed by
   chunk checksum. Test: rebase-concat == `crate::lex` whole-book
   (the chapter_par equivalence, now as a standing corpus test).
2. **CST/lint with carry**: per-chunk build + carry + neighbor
   fusion. Test: isolation-built-and-reduced CST/LintReport ==
   fresh whole-book on both corpora, PLUS adversarial straddle
   cases (`\esb` across `\c`, unclosed `\f` across `\c`, empty-`\p`
   and verse runs ending at a chapter seam, chapter numbering
   anomalies ACROSS the seam) proving the dirty-carry fusion path
   fires and converges to the oracle.

The chunk surface can also earn its keep as pure API SHAPE (item 8:
chapter-grain ingest for sous, the PER-CHAPTER coordinate mode's
adapter) without any of the memoization machinery ever promoting.

## The one-buffer wire: what it would take (assessed 2026-08-26)

Today `analyze` crosses the wall as ~13 typed arrays + 2 scalars + 1
string (the js-sys Object path). The single-buffer alternative — one
`Vec<u8>` behind a header — matters for the SECOND host (Tauri IPC via
`tauri::ipc::Response`), not for wasm, where the Object path is fine.
Will's estimate ("90% there already") is right; the load-bearing fact
is in onion-wasm.ts itself: `AnalysisView` never touches wasm — it
consumes `RawAnalysis`, a plain bag of Uint32Arrays. The change is
purely about where that bag comes from; nothing downstream moves.

The concrete delta:

- **Rust (~100–150 lines):** `Analysis::to_bytes() -> Vec<u8>`.
  Header = magic, wire version, len_utf16, usfm_version, section
  count, then a table of (section id, byte offset, byte len), then
  the sections. One ordering trick kills padding entirely: emit all
  u32 sections first (self-aligned — the header is u32s too, so every
  section start is a multiple of 4 for free) and put the only
  byte-shaped payload, `fix_text`, LAST. Zero pad bytes anywhere.
- **TS (~40 lines):** `rawAnalysisFromBuffer(buf): RawAnalysis` —
  read the table, one `new Uint32Array(buf, off, len/4)` view per
  section, `TextDecoder` for fixText (the one real change: it crosses
  as a JS string today, as UTF-8 bytes in the buffer). Everything
  from `AnalysisView` down — stride loops, bit helpers, `blockAt` —
  is untouched: that file already IS the interpreter, and it
  interprets `RawAnalysis`, not wasm.
- **Tests:** a Rust roundtrip (`to_bytes` → parse → equals the
  struct) and one golden buffer the TS side decodes.

The real cost is not the code, it is the CONTRACT: once bytes are the
wire, the header's wire-version byte and a bump discipline become
load-bearing (the Object path gets schema drift caught by TypeScript;
the buffer path has only its version check). Sequencing: serde-JSON
over plain `invoke` FIRST for the Tauri host (derive Serialize on
`Analysis` — works today, measure it), promote to the buffer when a
measurement fails. Serialization itself is memcpy-class either way —
the products are already flat u32 planes.

## Build plan (in progress, 2026-08-26 — rulings as they land)

### The dependency graph (corrects an earlier misreading)

The five-crate layout stands EXACTLY as ruled: per-engine wasm crates
enforce the boundary, and the engines never see each other. (The wasm
sketch's pass-7 "one combined crate" note was superseded by the
five-crate ruling; do not re-derive it.)

```
onion ◄── onion-wasm            sous ◄── sous-wasm
  ▲            ▲                  ▲           ▲
  │            │ (wasm feature)   │           │ (wasm feature)
  └──────── galley ◄──────────────┘───────────┘
```

- galley deps onion + sous NATIVELY, always; it pulls onion-wasm and
  sous-wasm only behind a `wasm` cargo feature.
- `#[wasm_bindgen]` exports in a DEPENDENCY survive into the final
  cdylib: `wasm-pack build galley` (feature on) emits one binary whose
  exports are onion-wasm's tags + sous-wasm's tags + galley's own —
  the vision's composed analysis host is galley compiled, no sixth
  crate. Each engine's pkg stays standalone-buildable.
- The TS layer composes the same way: `galley.ts` re-exports
  onion-wasm.ts (the decoders consume typed arrays and do not care
  which binary produced them) and adds galley's own decoders. The
  editor installs galley's pkg; onion-wasm's remains for bare-engine
  consumers.

### Ordering

1. Workspace-ify this repo (root `[workspace]`; members onion,
   onion-wasm, galley). Sous + sous-wasm join as members when sous
   starts — greenfield, zero migration. This repo IS the crates
   monorepo.
2. galley v0: `pub use onion;` (whole crate as a module — nothing
   hidden, galley's own names are the curated layer), plus
   `chunk_checksums`. Feature-gated bindgen tags inside galley.
3. Frontend swaps pkgs: onion-wasm → galley (a superset).
4. Sous lands; galley adds `proofread` behind the same shape.

### Rulings (Will, 2026-08-26)

- **The `\c` pre-scan is an ONION primitive** — promote
  `chapter_chunk_starts` out of experiments/ into the library proper
  (same class of primitive as `lex`; sous-side chunking wants it
  without galley someday). Galley re-exports.
- **xxhash-rust** (xxh3-128) is the checksum dep.
- **Checksums cross the wall as HEX STRINGS**, not u32 quads: the
  u32-plane convention is for OFFSETS; a checksum is an identity
  whose consumer-side job is to key a JS Map, volume is dozens per
  book, and strings key Maps natively. (Revises an earlier stride-4
  lean.)
- **v0 is checksums-only** — no ingest/LF-normalize yet; the read
  documents the same LF-canonical input contract analyze already has
  (the checksum is only stable against normalized text).

### Naming (Will, 2026-08-26)

- Workspace/repo: **usfm-workspace**.
- Crates: **usfm_onion** (the `_2` dies everywhere — this is the
  successor), **scripture_sous_chef**, **galley** (or `usfm_galley` —
  open; publishing to crates.io/npm would favor the prefixed form,
  bare `galley` is likely taken).
- The wasm doorways follow their engines (onion-wasm / sous-wasm as
  directories; package names settle with the galley naming call).
- The `usfm_onion_2 → usfm_onion` rename (package name + every
  `usfm_onion_2::` path in tests/bins) lands WITH step 1's
  workspace-ification — one mechanical commit, not two.

### The (hash → products) cache shape, noted for rung 2

Content-addressed and label/order-free: `(chunk hash) → chunk-
relative products`. No book name, no position in the key — pure
reordering, relabeling, or a chapter shared verbatim between books
all hit. Retrieval cost is REBASE MATH only, and that is proven, not
hoped (chapter_par.rs `rebased()` is `start + base`): absolute mode
adds the requesting book's chunk base (prefix sums), chapter-relative
mode adds nothing. UTF-16 consumers need UTF-16 chunk bases, so the
cache stores each chunk's UTF-16 length alongside — prefix sums
again. Two standing caveats:

- The carry rule from the monoid section: hash-only keys are valid
  exactly when the `\c`-boundary carry is clean; dirty carries fuse
  neighbors and recompute.
- Memory is a MEASUREMENT to record, not an assumption: tokens run
  ~8 bytes at ~1 per ~19 source bytes (a whole Bible's lex ≈ low
  single-digit MB), and no corpus copy is retained (encodeInto is
  fast) — but rung 2 promotes on a recorded number, per the ladder.

## Recipes (the concrete v1 surface the transcript sketches)

- `ingest(text)` → LF-normalize (→ maybe checksum) → products.
- `proofread(book | corpus)` → mask verse_text (untrimmed) → sous →
  findings mapped back to source bytes → caller's coordinates.
- `find(query, scope)` → mask → memmem/regex (aho-corasick for
  needle sets) → hits mapped back; replacement = Edits at the mapped
  (possibly discontiguous) spans.
- `project(triples)` → per-book pipeline, aggregated diagnostics,
  deterministic book ordering.

## How this addresses the vision, directly

- §4.4/§9.4 one composed wasm module = galley IS that module (deps:
  onion + sous, later linked as one binary).
- §9.2's "narrow stateful seam above pure functions" = galley's
  charter; the engines stay stateless forever.
- §9.6 complexity ladder: rung 1 = today's stateless galley (all
  evidence says it suffices); rung 2 = the checksum KV of item 6;
  rung 3 = resident source + splice replay, whose prior art is the
  braid-v2 sketch below. Promotion requires a recorded failing
  measurement.
- §13.4 checksum utility: galley's, deferred until save/dirty pulls.
- §10.1's "typed common projections + a small composable selector":
  onion's Filter already is this; galley curates the recipes.
- §11.1 whole-project diagnostics = the `project()` recipe.
- §12 project-wide Find = the `find()` recipe (no engine gap —
  see ideas/candidates/find-and-overlay.md).

## Prior art kept from braid v2 (the rest is superseded)

The braid-v2 sketch designed a Lexical-era stateful store (slots at
`\c` boundaries, piece-table backing, splice-as-the-one-primitive,
prediction/ruling/reconcile against the editor tree). The CodeMirror
pivot + the vision's canonical-document model replaced its editor
half wholesale, and the vision's rung discipline demotes its store
half to a measured promotion. Still live:

- **Chapter independence is PROVEN** (experiments/chapter_par.rs) —
  the factual basis for any future chunking, memoization, or
  parallel-open work.
- **The stateless core is the oracle**: retained-vs-fresh equivalence
  as a standing test is the law any rung-2/3 galley must keep.
- **Slot/splice mechanics** as rung-3 prior art: slot-relative spans
  + run-table bases + re-partition-on-splice is a worked design if
  resident source ever promotes. (Its Lexical reconcile protocol is
  dead; CM's canonical funnel replaced it.)
- Superseded outright: incremental lint as a fold (lint is ruled a
  whole-book pass; chunk memoization is its own parked idea), store
  undo (CM canonical history owns it), resident collapsed containers
  (Lexical rendering detail).
