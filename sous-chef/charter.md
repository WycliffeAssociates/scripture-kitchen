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
Galley is resident in its derived caches, judging config, and suppression
workflow, not in the editor's canonical text. Each analysis invocation takes
ownership of the complete target and optional source strings for its duration,
and returns numbers and offsets relative to exactly those strings. It does not
retain a second rope or accept splices between calls. This avoids a distributed
mutation protocol whose revisions or coordinates could diverge from the
editor. Galley may execute independent maps in parallel, but it may not change
reduction order or rule semantics.

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
   invocation's matching producer projection maps those offsets back to its
   source; its detached data may have been reused only after an exact raw-book
   checksum match. Onion resolves raw-USFM/editor spans, while a vref
   projection resolves the source line and numeric verse designator. Sous
   carries no scripture-key strings or parallel coordinate systems in every
   finding.
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
- A raw-book checksum is the identity boundary for reusable coordinate data.
  Galley may retain detached producer-map and UTF-16 index data for an
  unchanged book, then bind it to the byte-identical string supplied by a later
  invocation. Per-invocation borrow-bearing cursors are recreated; the source
  string itself is not part of the resident cache.
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
    fn key(&self) -> BookKey;
    fn text(&self) -> &str;
    fn chapters(&self) -> impl Iterator<Item = Chapter>;
    fn verses(&self) -> impl Iterator<Item = Verse>;
}

struct Corpus<'a, B> {
    books: &'a [B],
}
```

`Corpus` validates the caller's immutable book table before analysis: it
accepts at most 65,536 books, rejects duplicate `BookKey` values, and applies
the projected-book validation to every entry. `BookKey` is scripture identity
for matching books across corpora; the checked `BookIndex` is only the
position in this caller-provided array. Book order may therefore vary between
inputs without changing keyed semantics, while chapter and verse order within
each book remains significant. No carry crosses a book boundary.

The core alignment operation returns paired `AlignedUnit` rows separately
from `AlignmentFact` rows. Missing keys, duplicate ambiguity, and partial
range overlap remain structural facts for the host or rule to interpret; they
do not become findings or cause a shorter duplicate prefix to pair silently.

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

Every applicable analysis invocation supplies the complete target and optional
source strings. Galley owns them only while that invocation builds or rebinds
their projections and composes the verse iterators into aligned pairs for
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
- an Onion anchor with a missing or malformed numeric designator yields no
  aligned verse unit. Onion retains and lints the structural error; its text
  remains in the chapter projection for ordinary content analysis;
- units arrive grouped by book and chapter; the caller's book-table order may
  vary, so cross-corpus matching uses `BookKey` rather than array position;
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
derived state recomputed from the current invocation after any corpus change.

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
| 0..4 | `from: u32` | start in the book coordinate space declared by the owning container |
| 4..8 | `to: u32` | end in that coordinate space, exclusive |
| 8..10 | `book_idx: u16` | index into the snapshot's ordered book table |
| 10 | `code: u8` | union tag into the v1 code table below |
| 11 | `flags: u8` | representation flags; only `SATURATED` is currently valid |
| 12..14 | code-specific `i16` lane | meaning owned by the rule code |
| 14..16 | code-specific `i16` lane | meaning owned by the rule code |

The v1 code table and each code's lanes:

| code | kind | lane 12..14 | lane 14..16 | `SATURATED` means |
| --- | --- | --- | --- | --- |
| `0` | `LengthProportionality` | signed Q8.8 book-scope standardized deviation; `i16::MIN` unavailable | signed Q8.8 project/corpus-scope deviation; `i16::MIN` unavailable | a deviation was clamped |
| `1` | `Hygiene` | `HygieneClass` discriminant | run length in code points, `1..=i16::MAX` | the run exceeds `i16::MAX`; the lane reads exactly `i16::MAX` |

The 6-bit rule-id proposal does not save a byte in this layout and makes
decoding and evolution harder, so the charter chooses an explicit `u8`.
Each wire version has one dense table of active, hand-assigned codes. It has no
reserved ranges and no retired entries. Removing or renumbering a code requires
a new wire version rather than leaving tombstones in the current table.
Codes append within a wire version: `Hygiene` joined as `1` when the first
deterministic rule landed. Each code's lane codec lives in its own
`codec/<rule>.rs` beside the shared record, so the record module never
becomes a dumping ground for payload shapes.

`book_idx` is the caller's snapshot-local book-array index. During one
invocation, each exact input string travels with its ordered projection,
caller-owned navigation identity, chapter/verse rows, and source map. Before
publication, a Sous finding's range is projected-book UTF-8. Galley resolves
`book_idx`, maps that range through the invocation's producer projection to the
raw book, then converts the raw UTF-8 boundaries to the destination coordinate
space. A checksum-validated detached map or UTF-16 index may satisfy that work;
no borrowed slice or retained source string crosses the invocation boundary.
The ordinary JS publication uses raw-book UTF-16, just as Onion's wire does.
This mapping is deliberately outside `sous-core`: an Onion book uses its
`Mask`, while another producer may supply a different locator.
`book_idx` remains positional rather than a canonical `BookId`; the caller owns
the addressing space, and a USFM file or a vref corpus may both provide the
book. `u16` is ample without spending four bytes per finding.

The semantic record is a discriminated union: `PackedFinding` carries a
`FindingKind` — `LengthProportionality(ProportionalityDigest)` or
`Hygiene(HygieneDigest)` — and the wire code, lanes, and `SATURATED` flag are
derived from that kind. `i16::MIN` is proportionality's unavailable sentinel
and cannot be constructed as a `QuantizedDeviation`; a hygiene run of zero
cannot be constructed either, and decoders reject a `SATURATED` hygiene row
whose lane is not exactly `i16::MAX`. These lanes are compact
representations, not the analysis truth. A packed row cannot reconstruct rich
rule evidence, counts, or arguments.

An immutable snapshot row index plus an analysis identity forms an out-of-band
`FindingHandle`. Galley/Sous uses that handle to request typed detail, after
validating snapshot identity and currentness; stale detail is recomputed or
refused. `from`, `to`, and `book_idx` are navigation coordinates, not a
universal detail key.

### Complete corpus publication

Chapter observations are the reusable cache. Book and project rule summaries,
judgments, and packed findings are cheap products of an ordered whole-corpus
reduction. A change in one chapter may alter a project denominator and thereby
add, remove, or change findings in an otherwise untouched book, especially at
small support counts. Publication therefore replaces one complete corpus
findings snapshot; it does not promise independently reusable per-book finding
buffers or finding patches.

The corpus buffer follows Onion's assembly model one level higher: a versioned
header and book directory precede aligned `PackedFinding` sections. Directory
position is `BookIndex`; each entry carries `BookKey`, the published book
length, section offset, and finding count. Consumers can seek directly by
index or key and lazily decode only one book:

```ts
const snapshot = FindingsSnapshot.open(buffer);
const mark = snapshot.book("MRK");
mark.length;
mark.at(0);
```

The landed v1 envelope has a 40-byte little-endian header: `SOUS` magic,
format version, checked coordinate flags, book count, 16-byte record stride,
total finding count, and an opaque 16-byte `SnapshotId`. Its caller-ordered
directory uses one 16-byte row per book: three `BookKey` bytes plus a zero
terminator, published length, absolute section offset, and finding count.
Record sections follow contiguously with no incidental padding. The snapshot
identity is carried now; Galley's canonical identity calculation remains a
separate lifecycle decision.

The envelope declares the record coordinate space. Sous analysis emits
projected-book UTF-8 ranges; Galley publishes raw-book UTF-16 ranges for JS by
composing the producer locator with UTF-8-to-UTF-16 conversion before corpus
encoding. Split-mask and astral cases prove this composition. A projected
range that maps to discontinuous raw spans publishes the declared BOUNDING
range — first retained raw byte through last — as its navigation span. This is
the documented contract, not a silent contiguity pretense: `from`/`to` are
navigation coordinates, one logical finding stays one row, and the exact
retained run set remains reachable through the typed-detail path.

This buffer is a hot, derived findings publication, not serialization of the
resident `AnalysisSnapshot`. Chapter observations and rule inventories remain
inside Galley. Typed detail, rule-summary, inventory, and site-search calls may
initially use ordinary serde responses; a separate packed form is justified
only by measurement. A future disk cache may wrap the same publication, but
persistence does not shape this first envelope.

A Rust-owned schema generates the checked-in TypeScript `DataView` reader, and
a freshness test rejects drift. Per-book producer versions live with the
matching book table, not in every finding.
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
