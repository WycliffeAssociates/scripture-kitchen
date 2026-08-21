SUPERSEDED 2026-08-21 by masks-toc.md (vref = a renderer over the Toc; the "slab" IS the Toc).

# TOC + masks + vref + sous slab (roadmap item 3)

One addressing scheme, two artifacts, three consumers. Settled in
outline across the 2026-08-19/20 conversation (settled-facts: the slab
bullet, the memoization doc, the TOC-unification exchange); this sketch
is the specifics. Sources read: src/parse_header.rs (what exists),
../usfm_onion/src/vref.rs (the format precedent),
scripture-sous-chef-2/documentation/design.md (the slab contract).

## Layering (RULED in outline)

    TOC      token-fold facts, ~1 ns/token, no CST      always cheap
      ↑ one-way: masks are GROUPED BY toc rows; toc never knows masks
    MASKS    CST-dependent filter spans, a FAMILY        built on demand
      ↑
    consumers: vref (verse-granularity mask → text),
               search (mask spans + prefix sums),
               sous slab (masks + checksums, exporter-side)

## 1. The TOC — ParseHeader grown

What `ParseHeader` already has (src/parse_header.rs):

    book: Option<(u32, u16)>            // first \id's BookCode span
    runs: Vec<ChapterRun {
        rows: Range<u32>,               // TOKEN rows, [\c row .. next \c row)
        label: (u32, u16),              // the Designator span (len 0 = none)
        occurrence: u8,                 // dup labels counted, BYTES-exact
    }>

What the TOC adds — three things, all in the same single pass:

```rust
pub struct ChapterRun {
    pub rows: Range<u32>,        // exists
    pub label: (u32, u16),       // exists
    pub occurrence: u8,          // exists
    pub bytes: Range<u32>,       // NEW: byte range, TILING the file
    pub verses: Range<u32>,      // NEW: index range into ParseHeader.verse_anchors
}
pub struct ParseHeader {
    pub book: Option<(u32, u16)>,
    pub runs: Vec<ChapterRun>,
    pub verse_anchors: Vec<VerseAnchor>,   // NEW: flat, source order
}
/// One \v: where its designator's bytes are and what the interpreter read.
pub struct VerseAnchor {
    pub designator: (u32, u16),  // span; len 0 when \v carved no designator
    pub first: u32,              // designator::verse() first, 0 = malformed
    pub last: u32,               //   … last (12-14 ⇒ 12, 14)
}
```

- **Tiling rule (PROPOSED, needs Will's nod on the boundary):** a chunk
  starts at the byte AFTER the newline preceding its `\c` marker (Will:
  "partitioned into new-line `\c` chunks" — the preceding `\n` belongs
  to the PREVIOUS chunk, so a chunk is "the line carrying `\c` through
  the end of the line before the next `\c`"). Row 0 is the PREAMBLE:
  `0..first_chunk_start` (whole file when no `\c`). Consequence — the
  tiling oracle: `concat(all byte ranges) == 0..source.len()`, the
  partition oracle's little sibling, tested the same way. NOTE the
  delta from today's `rows`: token rows still start AT the `\c` row
  (unchanged, row-space); only the new BYTE range starts at line start.
  The two disagree by exactly the `\c` line's leading newline token,
  on purpose — bytes serve chunk identity, rows serve token slicing.
- **Verse anchors** are one more match arm in the existing token loop
  (`MarkerKind::Verse` + the same `chapter_label`-style payload step
  the `\c` arm uses, incl. the AttrList step-over). `first`/`last` are
  the designator interpreter's output, computed here because ordering
  lint and address rendering both want numbers, not spans. Anchors
  land flat in source order; each run's `verses` range is stamped when
  the next `\c` arrives (same shape as `rows.end`).
- **Cost check**: ParseHeader measured ~1-1.5 ns/token; the verse arm
  fires ~31k times per corpus against 7.16M tokens. Re-measure per the
  go-slow law; expected delta ≈ noise.

## 2. `toc.address(byte)` — content-derived addressing

```rust
pub struct Address<'a> {
    pub book: Option<&'a str>,     // the BookCode span's bytes
    pub chapter: Option<&'a str>,  // the run's LABEL bytes ("6"), not ordinal
    pub verse: Option<&'a str>,    // the anchor's designator bytes ("3", "12-14")
}
pub fn address(&self, byte: u32, source: &[u8]) -> Address<'_>
```

Two binary searches: runs by `bytes` (partition_point), then that run's
anchor slice by `designator.0 <= byte`. Degenerate cases fall out as
SHORTER addresses: byte before the first anchor → chapter-only; byte in
the preamble → book-only; no `\id` → all-None. Labels render from
SPANS, never from ordinals (no-minted-identity law; `\c 01` prints
"01"). Consumers: diagnostics ("MRK 6:3"), diff addressing, search
hits, the sous sid-on-read contract.

## 3. Masks — the CST-dependent artifact family

A mask = sorted, disjoint byte spans of source, grouped by TOC row. NOT
one artifact — a family keyed by a filter:

```rust
pub enum MaskKind {
    VerseText,       // Text tokens inside verse content; notes/figures
                     //   SUBTRACTED via CST note-frame extents; markers,
                     //   designators, attrs, callers excluded
    AllText,         // every Text token (proofing wants notes too)
    // later variants are cheap: same walk, different predicate
}
pub struct Mask {
    pub kind: MaskKind,
    pub spans: Vec<(u32, u32)>,      // flat, sorted, disjoint
    pub by_run: Vec<Range<u32>>,     // runs.len()+1 entries (0 = preamble),
                                     //   index ranges into spans
}
pub fn mask(kind, source, tokens, cst, toc) -> Mask
```

- Derivation: one `cst.in_order()` walk (or the lint walk's driver
  pattern) carrying "inside a note frame" depth — a Text leaf's span
  joins the mask iff the predicate passes; adjacent accepted spans
  merge. `text_runs` as a separate artifact does NOT exist in the
  crate today and does not need to — the mask IS text_runs with a
  predicate.
- One-way dependency: masks index INTO the TOC's runs (`by_run`
  parallel to `runs`); the TOC is never touched.
- Front matter naturally lands in `by_run[0]` and is EMPTY for
  `VerseText` — the sous "front matter is outside every mask span"
  jurisdiction split falls out structurally.

## 4. vref — a RENDERER of the VerseText mask (answers Will's roadmap note)

Will's note on item 3: "maybe a mask instead to create? prob worth
spitting out a real vref, but also worth having a masked version, so
might just generate from a mask?" — RESOLVED as: vref is not a second
mechanism, it is the VerseText mask CUT AT VERSE ANCHORS and rendered.

Onion's format (vref.rs precedent): `VrefMap = BTreeMap<String,
String>` — `"GEN 1:1"` → verse text with markup stripped, a separator
inserted at structural breaks (unconditional — correctness, not a
mode), optional trim. Onion also ships a `VrefIndex` (ordered entries,
sid + projection) for order-preserving consumers.

Ours:

```rust
pub fn vref(source, toc, mask /* VerseText */) -> Vec<(Address, String)>
// map sugar on top for onion-compat; keys render from address spans
```

Mechanics: walk each run's anchor slice; verse N's text = the mask
spans clipped to `[anchor N's designator end .. anchor N+1's start)`
(last verse runs to the run's byte end), joined with the structural
separator (single space where two spans were parted by markup — the
onion `push_collected_text` behavior), optionally trimmed. This is the
ONE allocation-bearing export in the family and it allocates only at
its own boundary (ownership law). The "masked version" Will asked
about is then free: hand the same renderer a different MaskKind.

## 5. Search across markup

```rust
pub struct MaskText { /* mask + prefix sums of span lengths */ }
impl MaskText {
    /// virtual (concatenated-content) offset -> file byte
    pub fn to_file(&self, virt: u32) -> u32;      // binary search + remainder
    /// file byte -> virtual offset (None when byte is masked OUT)
    pub fn to_virtual(&self, byte: u32) -> Option<u32>;
}
```

Search = run the needle over the virtual text (materialized or chunked
through the spans), map hits back via `to_file`, render with
`toc.address`. Same two-coordinate-systems shape as the UTF-16 stride
index and the chunk rebase — sorted breakpoints, binary search, one
addition. A hit may STRADDLE a span seam ("fine linen" across
`\w …\w*`); the virtual text is what makes that findable at all.

## 6. The sous slab — a thin exporter, engine stays hash-free

Slab = the sous contract's four flat u32 arrays, assembled FROM
(toc, VerseText mask) + checksums computed IN THE EXPORTER (xxh3-128
over each run's masked bytes) — the no-hash-dep-in-engine ruling.
Corrections already carried back to sous's doc and re-asserted here:
rows keyed by ORDINAL, label is a span, the producer NEVER refuses a
messy file (sous's design.md still says "non-increasing \c is refused"
— that refusal is sous's own policy gate, not the exporter's). vref
remains the degenerate case (one span per chapter) — here that's just
`MaskKind::VerseText` aggregated per run instead of per verse.

Placement (PROPOSED): the slab exporter is sous-adjacent code — it can
live in this crate as an `export::slab` module (pure, taking the
artifacts) with the xxh3 dep feature-gated, or in sous itself consuming
our public artifacts. Lean: sous-side first; promote inward only if a
second consumer appears.

## Methodology / tests (plain english)

- **Tiling oracle**: for all 226 corpus books, the TOC byte ranges
  concatenate to exactly `0..len` — preamble + chunks, no gaps, no
  overlaps. Unit cases: no `\c` at all (one preamble chunk), `\c` as
  first byte, `\c` at EOF, duplicate `\c 3 \c 3`, CRLF line ends.
- **Anchor sanity**: every verse anchor's designator span sits inside
  its run's byte range; `verses` ranges tile `verse_anchors` in order.
- **Address round-trips**: for every anchor, `address(designator.0)`
  renders that verse; for bytes BETWEEN anchors, the address is the
  preceding verse; preamble bytes render book-only. Pin "MRK 6:3"-style
  cases from a real book.
- **Mask containment**: every span in `by_run[i]` lies inside run i's
  byte range; spans sorted/disjoint globally (oracle over the corpus).
  VerseText mask of a book with footnotes contains NO byte of any note
  extent (assert against CST note frames directly).
- **vref vs onion**: run onion's `usfm_to_vref_map` and ours over the
  same corpus book(s); compare per-key after onion's known
  normalizations (separator/trim semantics documented as we match or
  deliberately diverge — divergences listed in the test, not silent).
- **Search**: needle spanning a `\w` seam in en_ult is found in virtual
  space and maps back to the true byte range; a needle inside a
  footnote is NOT found under VerseText but IS under AllText.
- **Perf**: ParseHeader delta after the verse arm (expect noise);
  mask build cost per book (expect ~a lint-walk, measure); slab
  end-to-end on the corpus for sous's sizing.

## Open questions

1. The tiling boundary nod: chunk starts at line-start-of-`\c` (prev
   newline belongs to previous chunk) — confirm, it defines chunk
   identity for memoization forever after.
2. Does `VerseAnchor` carry interpreter output (`first`/`last`) or stay
   spans-only with ordering lint re-interpreting? Sketch says carry —
   one interpretation, two consumers — but it makes ParseHeader depend
   on `designator.rs` (it already depends on the marker table; fine?).
3. Slab exporter placement (sous-side vs `export::slab` feature-gated).
4. Does the mask walk ride lint's walk driver (shared machinery) or own
   a tiny visitor? Decide at build; the driver is `pub(crate)` already.
