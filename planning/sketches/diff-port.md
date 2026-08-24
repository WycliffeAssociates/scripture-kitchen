# The diff port (roadmap item 4)

Onion's diff is TRUSTED — expect a nearly wholesale port of the
algorithm; the design work is the BOUNDARY (this repo speaks offsets,
onion speaks owned token streams). Sources read:
../usfm_onion/src/diff/{mod,skeleton,text_diff}.rs (~6,500 lines with
fixtures/proptests).

## What onion's diff actually is (inventoried 2026-08-20)

Three layers, cleanly separable:

1. **Identity — canonical SIDs.** `derive_canonical_sids(tokens, book)`
   walks the stream carrying `"BOOK c:v"` state: `\c`+number sets
   chapter (dup chapters get `_cdup_N`), `\v`+number sets verse (ranges
   render `c:12-14`; duplicate sids in one chapter get `_dup_N`);
   every token gets the current sid. Never trusts a carried sid in the
   native convention. Then `partition_by_sid` cuts the stream into
   gapless contiguous same-sid `SidBlock`s (block id = sid, `#N` for
   non-contiguous reuse), each carrying its concatenated `text_full`.
2. **Alignment — Myers over BLOCK IDS.** `similar::capture_diff_slices
   (Algorithm::Myers, baseline_ids, current_ids)` finds the LCS of
   block ids; the interleave (baseline-only run, current-only run,
   shared block) becomes `slots`. One-sided blocks with equal pairing
   keys (book+chapter+verse-start, packed in a Copy `Sid`) coalesce
   into MOVE pairs occupying two slots bound to one decision. Output =
   `DiffSkeleton { slots: Vec<Slot>, units: Vec<DecisionUnit> }`, where
   a `DecisionUnit` carries kind (Shared/Added/Deleted/Coalesced),
   status (Unchanged/Modified/Added/Deleted/Moved), both token vecs,
   dup context, `is_whitespace_change` / `is_usfm_structure_change`
   (whitespace-stripped and marker-stripped equality classifiers), and
   bridge-coverage narration (a one-sided verse covered by the other
   side's range).
3. **Presentation + merge.** `unit_text_diff` (similar's UAX-29 word /
   grapheme diff over reader-visible text only) rides BESIDE units.
   `merge_skeleton(skeleton, decisions, default_side)` is a PURE
   PROJECTION: walk slots, emit each unit's chosen side's tokens;
   unknown decision ids are hard errors (no fuzzy stale-id fallback);
   revert = a one-decision merge. Per-chapter batching
   (`diff_skeleton_by_chapter`) groups by book/chapter and diffs cells
   independently (rayon over ~20k+ tokens, ordered merge).

Dependencies it brings: `similar` (Myers + word/grapheme diffs; pure
Rust, maintained, wasm-clean — passes Will's boring-crate bar),
`rustc_hash` (trivial, replaceable), `serde` (NOT needed if we emit
flat arrays / let the session serialize).

## What ports wholesale vs what this repo already owns

| onion piece | port verdict |
|---|---|
| `derive_canonical_sids` | REPLACED — the TOC's runs + verse anchors ARE this fact, already computed (toc-vref-slab.md); block = the byte range between consecutive anchors. Dup/`_dup_N` semantics re-derived from anchor numbers + occurrence. |
| `partition_by_sid` | replaced by the same anchor cut (blocks become byte ranges, not token clones) |
| Myers over block ids | PORT AS-IS (via `similar`) — ids become pairing keys derived from anchors |
| coalesced moves, dup context, covered-by | PORT AS-IS — pure logic over the alignment |
| ws/structure classifiers | port; `whitespace_stripped_eq` over byte slices instead of token text |
| `unit_text_diff` | port as-is (similar's word diff over the masked text — the VerseText mask gives "reader-visible text" for free) |
| `merge_skeleton` / revert | port the PROJECTION; output changes shape (below) |
| by-chapter batching | replaced by the TOC's runs (the grouping artifact already exists) |

## The boundary re-speak (the actual design work)

Onion's unit carries CLONED TOKEN VECS and merge emits tokens. Here
nothing owns text: a unit references BYTE RANGES of the two inputs.

```rust
pub struct DiffInput<'a> { pub source: &'a [u8], pub toc: &'a ParseHeader }

pub struct DecisionUnit {
    pub kind: UnitKind, pub status: Status,
    pub baseline: Range<u32>,   // bytes of baseline, empty for Added
    pub current: Range<u32>,    // bytes of current,  empty for Deleted
    // dup_context, covered_by, ws/structure flags as in onion
}
// addresses render on demand: toc.address(unit.baseline.start) etc.
// unit identity for decisions: the content-derived sid STRING rendered
// from the address (+_dup/_cdup suffixes) — same stable id onion used,
// derived not stored (no-minted-identity law).
```

**The Edit question — where the ruled "hunks as crate::edit::Edit"
meets a wall, and the resolution (DESIGN CALL for Will):**
`edit::Edit.insert` is a `FixStr` — 15 inline bytes, ASCII-asserted,
ENGINE-GENERATED text. Diff inserts are arbitrary document text (whole
Devanagari verses); they cannot ride FixStr and should not: a diff
never authors text, it MOVES it. So the diff's replay artifact is a
sibling, not a reuse:

```rust
/// Replay baseline -> current: splice `insert` (a range of CURRENT's
/// bytes) over `from..to` of BASELINE. Zero owned text — lossless by
/// construction, both sides are the user's own bytes.
pub struct SpliceEdit { pub from: u32, pub to: u32, pub insert: Range<u32> }
pub fn to_edits(skeleton, decisions) -> Vec<SpliceEdit>   // merge as edits
pub fn apply_splices(baseline, current, edits) -> Vec<u8> // right-to-left
```

`edit.rs` grows this second type (it was created to be the crate's
proposed-text-change vocabulary; a copy-range edit is squarely that).
The CM session applies a `SpliceEdit` exactly like a fix — from/to
through the UTF-16 index, insert read out of current's string.

**The round-trip law (the diff's `check_fixes`):**

    apply_splices(A, B, to_edits(diff(A, B), all_current)) == B     // exactly
    apply_splices(A, B, to_edits(diff(A, B), all_baseline)) == A    // identity
    mixed decisions == onion's merge_skeleton output on the same inputs

Run over corpus PAIRS (see tests below). This is stronger than onion's
own guarantee (its merge emits token clones; byte equality was implied,
here it is asserted on bytes).

## Divergence stance (REVISED by Will 2026-08-24: pause, don't retreat)

If the anchor-cut blocks fight onion's semantics anywhere (the
`_dup_N`/occurrence corners, empty-sid front-matter blocks, the
pair-loose coalescing keys): STOP and present the divergence to Will —
which fixture, which layer, what the two behaviors are — rather than
silently falling back to a port-as-is of `derive_canonical_sids`.
The old auto-fallback stance is OVERRULED: assess first; a retreat to
onion's token machinery is a decision Will makes on the evidence, not
a pre-authorized escape hatch.

## What it needs that exists / doesn't

- Verse alignment → TOC verse anchors: EXISTS (toc sketch).
- Reader-visible text for word diffs → VerseText mask: EXISTS (same).
- Word segmentation → `similar::from_unicode_words` (UAX-29): comes
  with the dep; nothing to build.
- Two-document sessions: diff is the FIRST two-input API in the crate —
  entry takes two full sources, runs the pipeline per side (lex + toc;
  CST only if the mask/text-diff is requested), no shared state.

## Tests (plain english)

- **Port fixtures first**: onion's `skeleton_fixtures.rs` (815 lines)
  and `text_diff_fixtures.rs` encode the trusted behavior — carry them
  over as byte-level cases and require identical unit/slot/status
  output before any new tests. Its proptests port where the input
  generator translates.
- **Round-trip law** over real pairs: en_ulb vs en_ult same-book
  (different translations of MRK — heavy Modified), a book vs itself
  (all Unchanged, zero edits), a book vs itself with one verse deleted
  / duplicated / reordered (the ordering-lint fixtures reused), bdf_reg
  ROM 3's real doubled `\v 10` (dup context on genuine data).
- **Move detection**: swap two verses, expect one Coalesced pair, Moved
  status, two slots one decision; revert restores byte-identical A.
- **ws/structure classifiers**: reformat-only pair (newline churn) →
  every unit `is_whitespace_change`; markup-only pair (`\add` added) →
  `is_usfm_structure_change`, reader text equal.
- **Address rendering**: a Modified unit in MRK 6 renders "MRK 6:3"
  via the TOC on BOTH sides even when verse numbers moved.
- **Perf**: onion diffs a book in ~ms; ours should match or beat (no
  per-token String sids at all — the anchor cut removes onion's own
  measured #1 hot spot). Measure per the go-slow law.

## Resolved (Will, 2026-08-24)

1. `SpliceEdit` lives in `edit.rs` — the crate's edit vocabulary is a
   small family: authored micro-text (Edit/FixStr) + moved document
   text (SpliceEdit).
2. `similar` taken as a dependency (covers both Myers-over-blocks and
   the UAX-29 word/grapheme layer). `rustc_hash` skipped.
3. Merge/decisions surface ports WITH the algorithm (~100 lines of
   projection; the fixture catalog exercises it; consumer contract
   already fixed: opaque unitId strings, {unitId: side} map, loud
   unknown-id rejection).
4. Parallelism: NOT in this crate. Serial only; chunking/parallel
   concerns defer to the higher galley/braid layer with the other
   stateful-handle/monoid-shape questions. (Onion's rayon gate
   amortized its String-sid allocation storm; the anchor-cut units
   remove that cost class entirely.)
5. Alignment strategy per Will's read of the port table: partition via
   the TOC yes, NO derive_canonical_sids port; Myers over the aligned
   sections as-is; coalescence yes; covered_by labels yes (might never
   be used, cheap to know); ws/structure classifiers yes; unit_text_diff
   yes (words/chars within a block); merge_skeleton projection yes.
6. dup_context: Will asked "isn't that the TOC's job?" — the TOC
   detects duplicates WITHIN one document (occurrence indices on
   anchors); dup_context is the cross-document narration ("this verse
   number appears 2x baseline / 1x current"), derived at coalescing
   time from those same occurrence counts. Nearly free — keep it as a
   derived field, sourced from the TOC facts, nothing new stored.

## What do we diff — CST or raw text? (answered)

Neither, exactly: **byte ranges partitioned by the TOC** (token-level
anchors), which is onion's own grain re-spoken. The CST is not needed
for alignment — blocks are the bytes between consecutive verse/chapter
anchors, and unit equality is raw-byte equality of those ranges (so a
markup-only change reads Modified, then the classifiers label it
is_usfm_structure_change). The CST enters exactly once: the
unit_text_diff layer needs reader-visible text, which is the mask
(text + newline + optBreak kinds), and the mask walks the CST. A
structural tree-diff of the CST is neither needed nor wanted — the
verse grain IS the semantic alignment.

**How the ws-only / usfm-only classifiers work over byte ranges**
(Will's question 2026-08-24): the unit's byte range maps to a TOKEN
slice for free — blocks are cut at token-anchored offsets, so
`tokens.partition_point` recovers the slice on each side. Then:
- `is_whitespace_change`: compare the two ranges' raw bytes with ws
  bytes skipped (a two-cursor walk, zero alloc — onion's
  whitespace_stripped_eq re-spoken over slices).
- `is_usfm_structure_change`: compare only the bytes of TEXT-kind
  tokens within each slice (marker/designator/caller tokens skipped by
  kind), ws-normalized — "reader text equal, markup differs". Token
  kinds are all it needs; the CST never enters (onion also classified
  at token granularity, not tree).
Both short-circuit on first divergence, same as onion's
one-pass-zero-alloc claim.

## The n-way question (OPEN — Will leaning, needs a ruling)

Will 2026-08-24: the sid alignment exists for UI labels + resiliency
(add 500 words to a verse and the diff isn't muddied; chapters can be
delegated to workers and merged without conflicts). Should we START
at diffN, with 2-way as the N=2 minimum?

Recommendation: **2-way surface, N-ready primitives** — don't build
the N-ary unit model speculatively.

- What is ALREADY N-safe by construction (the expensive-to-change
  part): identity/partitioning. Pairing keys (book, chapter,
  verse-start + occurrence) come from each document's OWN Toc,
  document-independently — N documents each bring their anchors, and
  blocks bucket by the same Copy Sid key across all N. This was
  onion's recorded blocker for n-way (String sids per token per way)
  and the anchor cut deletes it.
- Does the TOC support an LCS spine against a master? Yes — and
  mostly without LCS: with semantic keys the spine is the canonical
  ordering of Sid keys (a master versification vref if one is
  supplied, else the union of anchors in canonical order). Each
  document aligns against the SPINE independently (N pairwise
  alignments to one reference, not N-choose-2); Myers/LCS is only
  needed to resolve ordering among duplicates, moves, and non-verse
  content per document.
- What 2-way hard-codes (the genuinely N-ary redesign, why not now):
  DecisionUnit's two Range fields become per-side presence sets; the
  status vocabulary (Added/Deleted/Modified/Moved) is 2-way-relative
  and would need re-speaking as per-side presence + a chosen
  reference; decisions become choose-among-N. Real design weight with
  no consumer pulling yet.
- Borrows are a non-issue for N-way splicing: SpliceEdit generalizes
  to (side_index, range), apply takes `&[&[u8]]` — all sources are
  immutable borrows, output is a fresh Vec. No headache.

## Proptests: keep a small core, not the full port (proposed)

Will wavered on whether proptests are needed. Lean: port the three
that check the diff's ALGEBRAIC laws — partition totality (every
input byte in exactly one bearing slot), the byte round-trip law over
generated pairs (this IS onion's P3 re-spoken), and unknown-id
rejection with zero side effects — over a simple byte-level document
generator. Skip the other proptests: the 23 fixtures + corpus pairs
cover the directed behaviors, and onion's token-level generator
doesn't translate cleanly to byte-world anyway. Rationale: those
three are exactly the "invariant stated in a checkable form" layer;
a property test is the cheapest checker for a total law. Determinism
rides along free (double-run equality inside the same tests).

## Addendum 2026-08-24 (research pass, for Will's reaction)

Second read of the old repos (subagent inventory of
../usfm_onion/src/diff + ../usfm_onion-spike + editor consumers +
onion's plans/memory). Corrections and additions to the sketch above.

**Source of truth: `usfm_onion`, not the spike.** The spike
(2026-07-21) is strictly older — no text_diff.rs at all, per-token
`format!` sid strings, no `_cdup_N` handling. Port from usfm_onion
(2026-08-05).

**"diffN" resolved: there is nothing to find or port.** No diffN code
exists anywhere under ~/Code. It's the working name for the PLANNED
n-way generalization of the 2-way skeleton — it appears only in
onion's plan docs (consolidation-epic-charter.md:35 "diff n-way
keying", perf-vref-alloc-spike.md:23) and the diff-perf memory note:
"the interleave-skeleton/slots/decision-unit bones generalize fine
(Myers is cheap, so N ways is cheap algorithmically); the design
choice that matters is partitioning on the compact Copy Sid so you pay
zero format!/alloc per token per way." That precondition is exactly
what this port's anchor cut does by construction (no String sids
anywhere), so: port 2-way now, and n-way stays a straightforward later
generalization (sides become a small vec; pairing keys unchanged) —
its own sketch when a consumer wants it. Likely the half-remembered
"separate folder": the JS prototype `scripture-editor-proto-2/
agent-tmp/prototypes/merge-interleave/` (source of the 23-case fixture
catalog) — referenced by onion's fixtures header, since deleted from
disk. The Rust fixtures preserve everything it encoded.

**Invariants to pin in the port (from onion's tests + Will's own
rulings recorded in its memory):**

- Pairing NEVER crosses a verse number: a renumber typo (`\v 11`
  content moved to `\v 1`) is delete+add, never a coalesced move —
  "do not improve this with cross-key content similarity" (case_23).
- Coalescing is two-tier: byte-identical text matches first (stream
  order among ties), then positional leftovers first-with-first.
- Moved status = coalesced AND byte-equal AND displaced (current slot
  precedes baseline slot, or a Shared slot sits strictly between).
- covered_by narrates only a SINGULAR one-sided verse overlapped by a
  true bridge on the paired opposite side — UI narration, never merge.
- Text diff is status-gated: Unchanged/Moved → None (no false
  highlight on a pure move); Added/Deleted → one unbroken run; only
  Modified gets split runs.
- Text diff's "reader-visible text" = kinds text + newline + optBreak
  (NOT markers/numbers): the mask filter for this must include
  vertical whitespace and optional breaks, not bare verse text — a
  char-marker wrap glues `\add ed\add*` into "…worded" with no
  synthetic space. Note prose rides in undifferentiated (v1 choice,
  kept).
- pairing_key's UNKNOWN fallback bucket silently pools unparsable
  sids; onion has a dedicated regression test that `_cdup_N` placement
  keeps repeated-chapter sids parseable through it — port that test.

**Consumer contract confirmed** (scripture-editor-proto-2 + Dovetail):
units are addressed by opaque `unitId` STRINGS only (never index or
sid), decisions travel as a `{unitId: side}` map with a defaultSide,
and stale/unknown ids must reject loudly ("unknown decision unit id:
…") — the editors rely on the strict error, no fuzzy fallback. The
whole skeleton round-trips as camelCase JSON. diffCalculationRunner is
a generic debounce hook, not a shape constraint.

**One convention retires here:** onion kept an interim "trust the
token's carried sid" calling path (build_sid_blocks over FormatToken)
awaiting a consumer migration. This repo's Toc-derived addressing IS
the always-derive convention; the interim path does not port.

**Test port inventory:** the 23-case fixture catalog (byte-exact
merges + revert-as-one-decision-merge, both token shapes);
skeleton proptests P1–P7 over the edited_doc_strategy generator
(partition totality, reparse fixed point, purity, determinism,
unknown-id rejection with zero side effects) plus 9 directed
proptests (bridge shapes, duplicate-sid delete-first, transposition,
CRLF/LF never coalescing); text-diff proptests (run concatenation
reproduces plain text exactly; requesting text diff never perturbs
skeleton or merge). If/when rayon: onion's serial-reference equality
over a 263k-token book is the stated correctness gate — keep it.

**Perf note supporting the byte-range units:** onion's own profiling
put the 2-way diff at ~100% allocation/String-formatting cost (Myers
itself ~62 of ~40k samples); its remaining to_vec/text_full clones
were flagged as deferred. The sketch's Range<u32> units eliminate that
whole class, so matching onion's ms-per-book should be the floor, not
the target.
