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

## Fallback stance

If the anchor-cut blocks fight onion's semantics anywhere (the
`_dup_N`/occurrence corners, empty-sid front-matter blocks, the
pair-loose coalescing keys), the fallback is PORT-AS-IS: lift
`DiffableToken` + `derive_canonical_sids` verbatim over a thin token
adapter (our Token exposes marker name + designator payload already)
and keep cloned-vec units internally, exposing only the byte-range +
SpliceEdit surface. Trigger: any fixture divergence traceable to block
construction rather than alignment. The surface stays identical either
way, so the fallback is an internal retreat, not an API change.

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

## Open questions

1. `SpliceEdit` into `edit.rs` (recommended) vs a diff-local type —
   Will's call; it sets whether "the crate's edit vocabulary" means
   one type or a small family.
2. Take `similar` as a dependency (recommended: boring, maintained,
   wasm-clean, and its word-diff replaces a second port) vs porting
   Myers by hand. `rustc_hash`: skip (std hasher or the anchor keys
   are already integers).
3. Does the decisions/merge surface (interactive merge UI contract:
   UnitId strings, MergeSide, default-side) port now or wait for the
   consumer? Lean: port with the algorithm — it is ~100 lines and the
   fixtures exercise it; deferring it forks the fixture suite.
4. Chapter-parallel diffing: keep onion's ≥20k-token gate with rayon,
   or serial-first per go-slow and parallelize on measurement?
