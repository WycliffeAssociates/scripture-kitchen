# Sous Chef 2 charter

Sous Chef finds reviewable inconsistencies in scripture text without assuming a
language model, dictionary, or writing-system-specific style guide. It learns
what a translation normally does, reports the evidence in plain counts, and
leaves the linguistic decision to a person.

This file is the durable authority for purpose, ownership, and cross-boundary
contracts. [rules.md](rules.md) defines the checks and their examples.
[roadmap.md](roadmap.md) says how to build and verify them. If an implementation
choice conflicts with this charter, stop and change the plan or amend the
charter deliberately.

## The product promise

- Findings make narrow, truthful claims. “This form occurs 1 of 1,053 times”
  is valid; “this is wrong” usually is not.
- Every corpus-relative verdict exposes its numerator, denominator, comparison
  population, and abstention condition.
- Separate reasons remain separate. One strong signal may speak by itself;
  unrelated signals are not blended into an opaque confidence number.
- Findings are patterns first and sites second: one review row may expand to
  all matching spans. A maximal bad run produces one finding.
- The engine is useful on a new language from the first corpus. It ships no
  trained language artifact and does not silently normalize itself to each
  project as the project changes.

## Ownership boundaries

### Onion owns scripture structure

`usfm_onion_2` owns parsing, marker validity, canonical book/chapter order,
content masks, verse anchors, and the mapping between raw USFM and addressable
content. It may reject duplicate or non-increasing chapter structure with a
typed error. Sous does not grow a second USFM parser or reconciliation engine.

`sous-core` does not depend directly on Onion. It accepts a small neutral,
borrowed analysis view containing addressable target text, projection rows,
and reverse source coordinates. A host adapter may depend on both crates and
construct that view from Onion. The CLI is the first such adapter; extract a
shared adapter crate only when a second consumer needs it.

Onion presents each file as raw UTF-8 bytes plus a monotone table of contents:

- book ranges in canonical order;
- chapter rows with raw-file spans and masked-content checksums;
- content spans to walk;
- verse anchors for addressing and aligned-unit iteration.

The table is derivable and disposable. Raw bytes are authoritative when the
table and file disagree.

### Sous owns content evidence

Sous walks only the content spans Onion exposes. It owns Unicode
classification, config-free observations, corpus reductions, judging,
finding serialization, and rule evidence. It does not own editor position
mapping, persistence policy, file I/O, corpus download, or user-facing
localization.

### Hosts own lifecycle and presentation

CLI, WASM, and editor adapters provide I/O and presentation. The workspace's
Galley is the resident coordinator: it holds the target corpus, optional source
corpus, chapter-observation caches, judging config, and suppression workflow.
It may execute independent maps in parallel, but it may not change reduction
order or rule semantics.

## Text, seam, and coordinate invariants

1. **Verse start is not sentence start.** Verse markers are addresses, not
   discourse boundaries. Sentence, punctuation, word, and adjacency state may
   cross a verse seam.
2. **Book is the discourse unit.** State never crosses a book boundary unless
   a rule explicitly declares a different semantic scope.
3. **Chapter is the rebuild unit.** A chapter edit invalidates that chapter's
   raw observation. Each pass declares a minimal `Carry`, reset at book
   boundaries, that ordered reduction uses to resolve chapter-seam behavior.
4. **Chapter mapping is independently executable.** A chapter observation is
   a pure function of that chapter's declared local inputs. It never reads a
   neighboring chapter. Cross-chapter facts arise only when deterministic
   ordered reduction applies the pass's `Carry`. This keeps mapping
   parallelizable without requiring threads or workers.
5. **Findings use projected book coordinates.** Core and wire findings name a
   book in the snapshot's immutable ordered book table, then carry half-open
   `u32` UTF-8 byte offsets into that book's analyzed text projection. The
   retained producer projection maps those offsets back to its source: Onion
   resolves raw-USFM/editor spans, while a vref projection resolves the source
   line and numeric verse designator. Sous carries no scripture-key strings or
   parallel coordinate systems in every finding.
6. **Findings never split a rendered grapheme.** The walk may use a proven
   fast atom rule; emitted span edges are checked against full grapheme
   boundaries.
7. **Digit pooling is mandatory.** Unicode decimal digits share one judging
   lane so number systems do not become punctuation anomalies.
8. **Glue is Mark plus grapheme extenders.** In particular ZWJ/ZWNJ and other
   extenders do not enter the nonletter inventory as punctuation.

## Observation and cache invariants

- Raw observations are config-free. Judgment config never participates in a
  chapter-observation cache key.
- Each corpus-relative rule declares: inputs, chapter observation, boundary
  state, ordered reductions, consumers, observation-affecting config, and
  judging-only config.
- Galley owns content-addressed chapter-observation reuse. A cached observation
  is a pure function of one chapter's declared local inputs and schema stamp;
  all neighboring context is represented by the pass's explicit `Carry` and
  resolved during ordered reduction.
- Every raw-source edit rebuilds or updates the producer projection and source
  map. If a pass's projected chapter inputs are unchanged, Galley may reuse its
  Sous observation and resolve the same projected offsets through the new map.
  A structural change reruns any pass that declared the changed rows as input.
- Target and source observations are cached independently. Adding, removing,
  or changing source text preserves target-only observations; Galley remaps
  only changed source chapters and reruns source pairing/proportionality
  reduction.
- `Carry`, book aggregates, paired ratios, and corpus aggregates are minimal
  disposable reduce products, not a second mutable cache, until measurement
  proves otherwise.
- Sites are re-derived from current text through first-class `sites(query)`
  and `sites_many(queries)` operations. The public query describes the
  pattern, not the search algorithm: exact literals may use `memchr`/`memmem`,
  many literals may use Aho-Corasick when justified, and semantic
  topology/run/digit patterns rescan through the classifier.
- Identical content may share stored observations, but reduction counts every
  positional occurrence.
- Deletion, insertion, edit, source replacement, and config-only re-judgment
  must converge to the same result as a cold analysis.

## The aligned-unit contract

Source-compared rules do not accept two unrelated strings and guess their
alignment. A producer supplies projected text plus ordered chapter and verse
rows. Sous consumes that semantic view; it does not require every producer to
store the same index. The production core contract is:

```rust
trait ProjectedBook {
    fn text(&self) -> &str;
    fn chapters(&self) -> impl Iterator<Item = Chapter>;
    fn verses(&self) -> impl Iterator<Item = Verse>;
}
```

`TextRange`, `Chapter`, `VerseKey`, and `Verse` make projected byte units and
bridge identity explicit. Duplicate verse keys remain duplicate rows in
producer order; pairing derives occurrence ordinals later instead of storing
or collapsing them at this boundary.

Onion already has the two authorities needed to implement the view. `Mask`
owns the kept raw-source ranges and projected-to-source mapping; `Toc` owns
book, chapter, verse, and bridge identity. Do not copy those facts into a
second `ProjectionIndex`. `Mask::project_source` converts TOC raw extents into
projected ranges. Locating a nonempty projected range composes
`Mask::to_source` with `Toc::locate` for its first and last retained bytes and
returns each intersected raw-source run, since removed markup can make one
projected range discontinuous in USFM. The numeric result is Onion's existing
`Sid`; formatting `"MRK 7:21"` is presentation work.

Onion's verse-text mask keeps actual content bytes and configured source
newlines. Marker/designator bytes take their required delimiter with them when
removed; surplus delimiter padding is a `Pad` token, rides the same removal,
and remains available to Onion's own padding lint rather than becoming Sous
content. The adapter does not synthesize separators or reinterpret Pad.

A vref loader implements the same semantics from different storage. It parses
numeric identity from the left column, uses right-column bytes as content, and
keeps each actual newline byte between records as a projected separator in its
reverse map. This is intentionally the honest projection of the lossy vref
file; it cannot recreate USFM whitespace or markup discarded during export.
Structural `<range>` rows remain part of alignment. Onion and vref are equal
producers of the Sous contract without claiming byte-identical projections.

Galley retains target and optional source projections. `update_source_text`
is the source mutation boundary; ordinary analysis does not receive the source
again. Galley composes the two borrowed verse iterators into aligned pairs for
Sous. A source may instead be an addressless vref stream such as
`MRK 7:21<TAB>text`; source units need stable keys and content, but only the
target must provide finding coordinates. The vref loader preserves structural
`<range>` rows: concrete text followed by contiguous `<range>` placeholders
becomes one interval unit beginning at the concrete row. It does not discard
the placeholders and pretend the remaining row is an ordinary single verse.
The semantic contract is:

- target and source units pair by an explicit stable key plus occurrence
  ordinal, never merely by array position;
- the ordinary scripture implementation yields verse units from Onion's TOC;
- units arrive grouped by canonical book and chapter;
- a key absent from either side is skipped by proportionality;
- exact duplicates pair by occurrence ordinal only when the pairing is
  unambiguous; otherwise that key abstains;
- matching bridges pair as one unit. When one side has a bridge and the other
  has exactly the same contiguous constituent verses, coalesce the constituent
  texts and compare the range totals as one aligned unit. Do not invent an
  equal per-verse division or duplicate the ratio into the distribution;
- incompatible or partially overlapping ranges abstain rather than silently
  realigning;
- empty units do not produce a length ratio;
- source and target are assumed to use compatible versification. Detecting or
  repairing versification differences belongs upstream; Sous may surface a
  typed refusal or a separate shear observation, but may not silently realign.

Proportionality caches target and source per-unit lengths independently, then
pairs and reduces them in book and project order. Adding or removing a source
does not throw away the target walk. Pairing, ratios, and median/MAD are cheap
derived state recomputed after `update_source_text`.

## Rule and configuration invariants

- Rules are classified as deterministic, convention-learned,
  source-compared, or census-only before scoring is designed.
- Counts, opportunities, and retained observations do not depend on review
  depth or enablement. Judging-only changes reuse observations.
- A configurable rule names its primary signal and its support floor
  separately. Weak support causes abstention or a visibly thin claim; another
  signal may not compensate for it invisibly.
- Global defaults come from reproducible fleet measurement. Per-project
  evidence determines a finding, but does not refit the meaning of a global
  sensitivity control.
- Typed rule enablement and judging knobs belong to Sous config. Additional
  suppressions are Galley workflow state: they anchor to rule plus relevant
  content/context, survive unrelated edits, expire when reviewed text changes,
  and filter publication without rewriting the corpus's observations or
  denominators.

## Finding and wire contract

The in-memory finding model may carry rich typed evidence. The hot transport
is a versioned fixed-width snapshot designed from day one, not a serialized
Rust struct and not an array of JS objects.

Recommended v1 record, 16 bytes, little-endian:

| bytes | field | contract |
| --- | --- | --- |
| 0..4 | `from: u32` | projected-book UTF-8 start |
| 4..8 | `to: u32` | projected-book UTF-8 end, exclusive |
| 8..10 | `book_idx: u16` | index into the snapshot's ordered book table |
| 10 | `code: u8` | explicit append-only rule discriminant |
| 11 | `flags: u8` | severity, evidence presence, saturation, reserved bits |
| 12..14 | `numerator: u16` | compact display digest |
| 14..16 | `denominator: u16` | compact display digest |

The 6-bit rule-id proposal does not save a byte in this layout and makes
decoding and evolution harder, so the charter chooses an explicit `u8`.
Each wire version has one dense table of active, hand-assigned codes. It has no
reserved ranges and no retired entries. Removing or renumbering a code requires
a new wire version rather than leaving tombstones in the current table.

`book_idx` is the caller's snapshot-local book-array index. Galley retains the
exact immutable ordered projections used for analysis; each projection carries
its caller-owned navigation identity, chapter/verse rows, and source map.
Selecting a finding resolves `book_idx`, then calls `locate(from..to)` on that
projection. It is deliberately positional rather than a canonical `BookId`:
the caller owns the addressing space, and a USFM file or a vref corpus may both
provide the book. `u16` is ample without spending four bytes per finding.

The two count lanes are a compact UI digest, not the analysis truth. They
saturate at `u16::MAX` and set a flag; rich `u32` counts and rule-specific
arguments remain available through a typed detail path. Rules whose useful
digest is not a count pair define a code-specific interpretation or write
zeroes. A schema table and generated consumer declarations are single-sourced
from Rust.

The buffer has a small header with magic, format version, record length,
record count, engine/schema stamp, corpus revision/context, and analysis
identity. Per-book producer versions live with the matching book table, not in
every finding.
Decoders fail closed on unknown versions, codes, flags, malformed spans, or
length mismatch. Complete snapshots replace previous snapshots; receiver-side
reconciliation preserves object identity where useful.

## Dependency and generation policy

- Prefer standard-library code and small, well-understood dependencies.
- Keep the first wire flags byte as a small checked newtype. Add `bitflags`
  only if several internal masks develop real set-algebra needs; do not add a
  dependency for one byte of fail-closed wire decoding.
- Generated Unicode and wire tables are committed, reviewable artifacts. A
  small repository command regenerates them from pinned inputs.
- Generation has exhaustive drift tests against the source predicates/UCD
  data. Do not run code generation implicitly in `build.rs`.
- Reuse the old repository's generator logic and tests selectively; do not
  import its full `u32` classification layout or dependency graph without a
  current consumer proving each bit.
- Corpora and oracle blobs are not committed and require no Git LFS. A small
  manifest pins downloadable inputs by size and checksum.

## Evidence and compatibility policy

- Synthetic examples and invariant/property tests live in the repository.
- Corpora calibrate and compare behavior; they are not ordinary test fixtures.
- The old repository is a donor and a behavioral comparator, not the v2
  architecture. Exact output parity is required for deliberately unchanged
  deterministic behavior. Redesigned statistical rules require a documented
  divergence adjudication, not a broad re-pin.
- Keep a small, script-diverse iteration tier and a full-fleet bookend. Port
  only the dump/compare machinery necessary to reproduce those gates; do not
  port the old blob/oracle subsystem wholesale.

## Non-goals until a measured gate promotes them

- workers, resident background schedulers, or mandatory Rayon execution;
- a materialized scalar tape;
- retained site lists or corpus rollup caches;
- binary diffs/tombstones between finding snapshots;
- a general versification-repair engine;
- a compatibility layer for v1 public APIs;
- trained language models or shipped per-language artifacts.
