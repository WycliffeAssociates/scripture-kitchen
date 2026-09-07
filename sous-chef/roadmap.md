# Sous Chef 2 roadmap

Sous Chef 2 is a deliberate rewrite of the small, language-agnostic content
reviewer inside `scripture-sous-chef`. The old repository is a donor and a
behavioral comparator, not an architecture to preserve. The exploratory v2
probes have already shown that a streaming counter model can cover most of the
desired checks with much less code and state.

This roadmap is capability-led. It records what to carry, what to leave
behind, the order in which contracts become real, and the gates that may still
say no. It does not authorize unreviewed implementation beyond the named
stage.

The durable authorities are [charter.md](charter.md) for purpose, ownership,
and cross-boundary contracts; [rules/](rules/) for what each lane may claim;
this file for sequencing and gates; and [evidence.md](evidence.md) for every
measurement they rest on. Implementation detail lives beside its code, in the
module READMEs the ledger below names.

## Current baseline

Landed:

- a Cargo workspace with production `core`/`cli` crate boundaries;
- checked projected-book domain types and a producer-neutral iterator
  contract;
- an Onion CLI adapter composing the existing `Mask` and `Toc`, including
  projected chapter/verse ranges and reverse location to discontinuous raw
  source spans;
- a typed `usage-rs` CLI over one file or one directory, serial by default
  with explicit `--parallel`, printing provisional projection/alignment rows
  plus optional `--stats`/`--stats-only`. Its wall-clock timing is debug
  output, not a benchmark;
- a producer-neutral aligned-unit table pairing by `BookKey`, preserving
  duplicate occurrence order, reporting structural alignment facts separately,
  and retaining bridge constituents as multiple projected ranges;
- a headerless, checked 16-byte finding record inside a checked
  complete-corpus envelope, with a generated lazy TypeScript reader;
- a generated, committed Unicode classification table from pinned UCD 17.0.0
  extracts, with its drift, `std` cross-check, agreement, and determinism
  gates, plus the grapheme atom rule and its two conformance differentials;
- deterministic byte and classifier hygiene checks over projected content,
  exposed through the CLI and published through galley;
- an explicitly disposable probes crate;
- a manifest-and-fetch corpus lane with no Git LFS dependency;
- a clean ownership direction for Onion TOC/content masks and file-relative
  findings;
- a catalog mapping the PO checks into Sous, Onion/editor, or unresolved
  lanes.

Probe types and APIs remain evidence, not a compatibility surface.

## Testing posture

The repository is exploratory overall, so ordinary feature work uses an
**alpha** tolerance: test load-bearing behavior without freezing incidental
interfaces. Three areas are **hardened from their first production commit**
because later correction would be expensive:

1. source-coordinate and chapter/book seam invariants;
2. finding wire layout, generated schema, and rejection behavior;
3. cold, rebuilt, and reordered reduction equivalence.

Synthetic tests and generated vectors are committed. Fleet runs remain local
or explicitly invoked gates.

## Donor disposition

“Port” means preserve a proven behavior or technique after restating its v2
contract. It never means copy a module wholesale by default.

| donor area | disposition | v2 decision |
| --- | --- | --- |
| corpus-relative, language-agnostic thesis | **keep** | This is the product. Narrow claims and printable evidence are charter law. |
| 20-plus PO checks | **change** | Collapse them into hygiene, shared nonletter views, word lanes, and source comparison; do not recreate one module per checklist row. |
| probe streaming substrate | **rewrite from evidence** | Preserve measured shapes and invariants; production code starts clean in `crates/core`. |
| generated Unicode classification | **migrate selectively** | Reuse the generator discipline, pinned UCD inputs, and exhaustive drift tests. Generate only bits with current consumers; do not inherit the old full `u32` layout automatically. |
| allocation-free hot walk and local interning | **keep** | Dense scalar IDs, packed short words, and rare overflow maps are the starting implementation techniques. Re-measure production code. |
| Wilson-based rule scoring | **ditch** | Use explicit fractions and support bands. Keep any old Wilson code only as a calibration comparator until the band judge is adjudicated. |
| Review Depth framework | **change** | Start with explicit per-band thresholds. Add a global master mapping only after the fixed bands and user vocabulary are proven. No per-project self-normalization. |
| proportionality median/MAD | **migrate with gates** | Preserve paired-unit ratios, book plus project scopes, and asymmetric spread as the starting model. Rebuild against the v2 aligned-unit contract and reproduce the paired survey before accepting defaults. |
| untranslated-word candidate | **evaluate later** | It is a separate source-compared observation, not an extension of proportionality. Port only after counterexamples and excusals are re-adjudicated. |
| packed 16-byte finding snapshots | **migrate concept, change layout** | Keep versioned fixed records, append-only codes, generated consumers, fail-closed decode, and complete snapshots. Change from `key_idx + u16 verse offsets` to `u16 book_idx + projected-book u32 from/to`; use the invocation's checksum-matched producer projection to locate source positions, and use a typed rule-kind union with signed Q8.8 display digests. |
| lazy rich finding args | **keep as a capability** | The compact record is not the full truth. Final host spelling waits until Galley is designed. |
| content-derived analysis IDs | **evaluate during Galley** | Useful for cache validation, but not required to prove core rules or the first codec. Do not make persistence part of core. |
| resident Galley state machine | **audit, then redesign** | Keep one stateful owner over a pure core and complete-snapshot semantics. Port only calls an actual host needs; do not preserve the v1 API for compatibility. |
| JS reconcile-in-place helper | **evaluate with the consumer** | Receiver-owned identity reuse is sound, but implement only when a real UI consumes snapshots. |
| WASM/TS mirror structs | **ditch** | The binary schema is the boundary. Generate declarations and keep hand-written adapters thin. |
| rule-development contract | **migrate and shorten** | Preserve claim/counterclaim, evidence roles, typed observations, calibration, and surface/verification gates in the shared contract in [rules/README.md](rules/README.md). |
| byte-identical full-fleet oracle infrastructure | **do not port wholesale** | Keep small/fleet dump-and-compare capabilities. Exact parity gates unchanged deterministic ports; redesigned statistical rules use explicit divergence adjudication. |
| corpus blobs and Git LFS-era storage | **ditch** | Keep checksum-pinned fetch manifests. Add a local derived blob only if file-open/parse cost becomes a measured bottleneck. |
| v1 playground | **replace** | Do not rebuild the visual playground as the development harness. Grow the real CLI as a thin walking consumer from the first executable capability; consider a visual consumer later only for editor-specific workflows. |

## Decisions already made

- Four durable authorities only: [charter.md](charter.md), the
  [rules/](rules/) folder, this roadmap, and the [evidence.md](evidence.md)
  ledger. Implementation detail belongs in a README beside its code.
- A neutral projected-book view is the input boundary. Onion's masked
  text/TOC and a vref loader can both produce it.
- `Corpus` validates the immutable caller-ordered book table. `BookKey` pairs
  scripture books across corpora; checked `BookIndex` values address the
  caller's snapshot position. Filesystem discovery is a CLI concern and is
  immediate, extension-filtered, and lexically sorted only for deterministic
  debug assignment.
- Core findings use an immutable book-table index plus projected UTF-8 byte
  spans. The invocation's matching producer projection locates numeric verse
  designators and raw source spans; checksum-keyed UTF-16 index data maps
  editor positions outside core without retaining the source string.
- Book is the discourse unit; chapter is the independently mappable rebuild
  unit; ordered reduction stitches boundary state.
- Counts are config-free; judgment is a cheap pure read over observations.
- No retained corpus rollup, scalar tape, site list, worker, or scheduler
  without a measured promotion gate.
- Fixed-width serializable findings are designed before rule implementation.
- Rule discriminants use explicit `u8`, not a packed 6-bit field.
- Active proportionality lanes are signed Q8.8 display digests with an
  unavailable `i16::MIN` sentinel; saturation metadata is explicit, while rich
  evidence remains out of band.
- Corpora stay outside git and are pinned through a small manifest.
- `sous-core` stays independent of Onion. The CLI initially depends on both and
  adapts Onion's projection into Sous's neutral borrowed input; extract a shared
  adapter only after a second consumer exists.
- The CLI is the walking consumer, built with `usage-rs`; it begins with debug
  output and gains stable machine-readable output only when an oracle or host
  contract needs it.

## Resolved owner boundaries

- Missing or incompatible aligned units are typed adapter/alignment facts.
  Proportionality skips or abstains; it does not turn structural absence into a
  length finding. A future presence or shear rule needs its own actionable
  claim and evidence contract before entering Sous.

Proportionality retains the current calibrated defaults: `z_long = 3.5`,
`z_short = 3.5`, and `min_verses = 50`. The v2 paired survey verifies them; it
is not an open redesign gate. Galley caches target/source chapter observations
independently, so a changed source input never invalidates target-only work.

## Opening implementation ledger

Which capability each production owner holds now, and what it owes next.
Shapes and layouts live in the module README each row names.

| capability | production owner | next work |
| --- | --- | --- |
| projected offsets | `sous-core::TextRange`, checked half-open projected UTF-8 bytes | reuse in rich findings |
| chapter/verse input | `ProjectedBook` rows; `Corpus` validates the caller-ordered book table | consume the aligned-unit/fact table from source-comparison rules |
| Onion projection | the CLI adapter, over `Mask` and `Toc` | move reusable conveniences into Onion when a second host needs them |
| vref projection | semantic contract settled; no production loader yet | implement after the Onion path pins duplicate and bridge fixtures |
| nonletter substrate | `donor/` probes are measured donors only | port consumer-led classifier bits in Stage 3 |
| finding transport | `sous-core::codec` and `sous-core::corpus`, rebased and published by `galley::sous`. See [core/src/codec/README.md](core/src/codec/README.md) | Galley's canonical snapshot identity and checksum-keyed detached reuse (Stage 2) |
| unicode classification | `sous-core::unicode`. See [core/src/unicode/README.md](core/src/unicode/README.md) | Level 1b consumes the bits in Stage 3; casing beyond the two predicate bits waits for Stage 4 |
| convention judging | `sous-core::judge`, published as the envelope's pattern table, all four grains plus rarity. See [core/src/judge.md](core/src/judge.md) | whether Terminal vs Separator is the split that matters, and whether Quote and Bracket should merge — both open until fleet evidence says |
| word conventions | `sous-core::words` in `Brigade`, judged as `Channel::Casing` and `Channel::WordLength` over a learned `TerminalTable`, sited by `Words::locate`; the same table's counts read the other way are the glyph-side `Channel::SentenceStart`. See [core/src/words.md](core/src/words.md) | whether spaceless scripts get a dictionary fallback or keep abstaining; whether the length channel ever earns being on |
| convention sites | `sous-core::sites` behind `ChapterPass::locate`, cached per book by `(RawChecksum, FiringHash)` in `galley::sous::Expediter`. See [core/src/sites.md](core/src/sites.md) | Aho-Corasick if a corpus ever shows many rare needles per book (evidence.md, 2026-09-04); a general one-pass byte-class sweep, still unbuilt |
| hygiene | `sous-core::hygiene::scan` for the byte classes, the substrate row's `hygiene` lane for the four scalar ones. See [core/src/hygiene.md](core/src/hygiene.md) and [rules/hygiene.md](rules/hygiene.md) | NBSP's verse-edge case once the walk is verse-grained; a snapshot identity instead of the CLI's zero id |

Stage 0 is closed. Stage 1 was entered hygiene-first rather than
classifier-first: the byte-level checks need no Unicode data, they put the
first real finding through the new publication seam, and they leave the
classifier's bit set to be chosen by its actual consumers rather than by the
donor layout.

## Stage 0 — Freeze the foundation contracts

**Goal:** make the smallest hard-to-reverse decisions executable before rule
code creates pressure to bend them. **Closed.**

Work:

1. Define production domain types for projected text ranges, chapter rows,
   verse/bridge keys, typed input refusal, and the neutral borrowed book view
   consumed by `sous-core`. For Onion, compose existing mask spans and TOC
   anchors rather than defining duplicate stored rows. **Landed; raw file
   identity and document version remain host/Galley work.**
2. Define and implement the aligned-unit table using a tiny synthetic
   target/source pair, including duplicate keys, occurrence ordinals, empty
   units, absent units, bridge coalescing, chapter seams, and incompatible
   ordering. **Landed.**
3. Define and implement the headerless 16-byte finding record in a tiny codec
   module. Keep rich `Finding` separate from `PackedFinding`; make the wire
   code a typed finding-kind union tag, with signed Q8.8 payload lanes and an
   unavailable sentinel. Leave the snapshot header/schema identity and typed
   detail handle unresolved. **Landed.**
4. Hand-assign the dense active rule-code table for wire version 1. Reserve no
   ranges and keep no retired entries; removal or renumbering requires a new
   wire version. Do not pre-allocate a code for every idea-shelf rule.
   **Landed.**
5. Implement one complete corpus findings publication around the landed
   record: versioned header, ordered `BookKey` directory, per-book published
   length/offset/count, aligned record sections, and a generated lazy
   TypeScript `DataView` reader. Copy Onion's assembly law: reusable chapter
   products feed a fresh complete publication. Keep the resident observations
   and rule summaries out of this wire. The corpus publisher accepts
   publication-ready book coordinates; Galley later owns projected-UTF-8 to
   raw-source to UTF-16 rebasing through each invocation's checksum-matched
   producer projection and detached UTF-16 index data. **Landed, with the
   rebasing seam stateless in `galley::sous`.**
6. Replace the placeholder CLI with a minimal `usage-rs` command declaration.
   It accepts one target file or directory, mirrors that shape for an optional
   source, constructs caller-ordered book tables, and prints provisional
   projection/alignment debug output. Keep all Onion adaptation in the CLI,
   not `sous-core`. **Landed.**

Verification gate — **green**:

- exact 16-byte layout and byte-exact golden vectors;
- invalid book indices, reversed/out-of-bounds spans, unknown codes/flags,
  malformed lengths, and unavailable-sentinel misuse fail explicitly;
- a finding spanning astral text remains projection-byte-correct, locates to
  its producer source, and publishes the correct UTF-16 range through an Onion
  adapter test; split-mask mapping behavior is explicit and tested rather than
  treated as a contiguous identity map. A discontinuous projection publishes
  the documented bounding navigation span;
- schema generation is deterministic and `git diff --exit-code` clean after a
  second run.

## Stage 1 — Unicode and content-walk foundation

**Goal:** establish the one walk every later observation trusts. **Items 1, 2,
4, and 5 landed; item 3 is proven by the CLI adapter and item 6 by the hygiene
CLI path, so the gate review can run before Stage 2 opens.**

Work:

1. Port the minimal Unicode classifier generator with pinned Unicode inputs.
   Start from consumer questions, not the donor bit layout: alphabetic,
   casing, decimal digit, whitespace, mark, punctuation/symbol, extender,
   grapheme-complex, and only proven refinements. **Landed as
   `sous-core::unicode`; the atom rule proved three refinements the charter
   list needed.**
2. Commit generated compact ranges plus an at-runtime BMP lookup strategy.
   Compare a static expanded table against a local snapshot only if both are
   viable in the new workspace; choose by measured whole-walk cost and memory.
   **Landed as one static pool with two index paths; the rejected shape is in
   [experiments/](experiments/).**
3. Implement masked iteration over Onion's projection in projected UTF-8
   coordinates. Chapter and verse APIs derive ranges from the existing `Mask`
   and `Toc`; they do not allocate per-chapter/per-verse strings or store a
   parallel key table. Implement numeric `locate()` by composing
   `Mask::to_source` with `Toc::locate`, without scripture-key strings.
   **Proven by the CLI adapter; promotion to a shared Onion convenience waits
   for a second host.**
4. Implement grapheme-safe emitted boundaries with the fast atom rule and a
   correctness fallback/check for complex cases. **Landed; the runtime carries
   no segmentation dependency.**
5. Implement deterministic hygiene scans over raw bytes with content-mask hit
   validation. **Landed over projected content, byte-level and
   classifier-dependent checks both. One deferred item, in
   [rules/hygiene.md](rules/hygiene.md).**
6. Expose the implemented hygiene findings through the same CLI command. This
   is the first real consumer path, not a separate playground API. **Landed as
   `--findings` and `--publish`.**

Verification gate — **green**:

- exhaustive generated-table agreement with source predicates/UCD data, and a
  deterministic second generator run;
- script-diverse UAX/grapheme differential, including ZWJ/ZWNJ and the regional
  indicator edge, against the pristine `GraphemeBreakTest.txt` and a segmenter
  oracle;
- every reported span lies in projected content and on UTF-8/grapheme
  boundaries, and `locate()` returns the correct raw producer position;
- CRLF, astral, and empty-span synthetic cases here; markup-only and split-mask
  stay proven at the Stage 0 publication seam;
- production benchmark remains within an explicitly reviewed regression band
  of the probe floor. Carrying the classifier walk costs `hygiene::scan` real
  throughput; the rows are in [evidence.md](evidence.md), and that is the pass
  every Level 1b observation rides.

## Stage 2 — Galley chapter cache and ordered reduction

**Goal:** prove incremental correctness before the shared model grows.

Work:

1. Define the pass contract with a chapter observation, schema stamp, minimal
   fold-owned seam state, book aggregate, and consumers. **Landed as
   `sous_core::ChapterPass` and `analyze`, with hygiene as the first pass and
   the CLI as its caller; D2a-1 split it into map / fold / judge, with judging
   corpus-level and config-driven; see [core/src/pass.md](core/src/pass.md).**
2. Add the workspace's minimal Galley coordinator. Each invocation owns its
   complete target and optional source strings/TOCs only for that call. Galley
   retains config and independently content-addressed target/source chapter
   observations plus checksum-keyed detached projection and UTF-16 index data;
   it does not retain a canonical rope or accept splices. The pure Sous
   map/fold/judge functions own no resident cache. **B1 landed as `galley::Pantry`:
   the id-keyed registry of detached per-book products (Warmer, TOC, mask,
   detached UTF-16 table, published length, `Fingerprint`), canonical `BookKey`
   order, `Target` role only; see
   [../galley/src/pantry.md](../galley/src/pantry.md). Text retention landed
   next to it: a target keeps its text — D2b made that mandatory, since placing
   its findings rescans it — and `update` hands back an `Entry`, the per-book
   handle `lint`, `parse`, and the detached products all answer from. B2 landed
   too: `galley::sous::Expediter` maps lazily at `publish`, keyed by
   `ObservationKey`, and publishes a complete corpus buffer byte-equal to cold
   `analyze`. C2 added the opt-in `parallel` feature — a book's missing
   chapters map on rayon, inserted in chapter order, with no pool of galley's
   own, no scheduler and no background work — and `resident_bytes`. See
   [../galley/src/sous/expediter.md](../galley/src/sous/expediter.md).**
3. Implement ordered book reduction over independently mapped chapters. Keep
   only the smallest carry facts needed for nonletter adjacency, casing
   terminal state, and doubled-word state; do not create a generic monoid
   framework or retained book contribution layer.
4. Drive successive invocations with complete corpus snapshots containing
   target/source replacement, insertion, deletion, and source removal. Reuse
   coordinate data only for byte-identical raw-book checksums and observations
   only for matching declared inputs. A changed source preserves eligible
   target observations, remaps only changed source books/chapters, and reruns
   cheap pairing and source-rule reduction.
5. Mark-and-sweep unreachable chapter observations. Keep carry, paired ratios,
   and book/corpus aggregates derived on demand; add no rollup cache. **C1
   landed: every `Expediter::publish` sweeps the chapter tables outside a
   book's generation ring (its current `RawChecksum` plus `with_generations(n)`
   before it, four by default) and every observation no surviving table names;
   the host mutates through `Expediter::{update, update_with, remove}` and the
   fold borrows its observations. See
   [../galley/src/sous/expediter.md](../galley/src/sous/expediter.md).**

Verification gate — **green for a target-only pass; two bullets name the half
that waits for a consumer.** The harness is
[../galley/tests/equivalence.rs](../galley/tests/equivalence.rs): a seeded edit
churn that republishes after every step, asserts the bytes against a cold
`analyze` of the same texts, and asserts that no more chapters were mapped than
the step's own analysis inputs changed. The bound is recomputed there from
`for_each_chapter`, not read off the cache it bounds, and a failure prints
`seed=… step=… edit=…`.

- cold analysis equals chapter-at-a-time rebuild after every chapter is
  replaced in sequence — `every_chapter_replaced_in_sequence_equals_cold`, and
  `every_chapter_of_a_whole_bible_book_replaced_in_sequence_equals_cold` over
  the committed 66-book corpus;
- randomized edit/insert/delete operations, submitted as complete new input
  strings, equal a fresh rebuild after each step —
  `churn_over_a_synthetic_corpus` (200 steps) and `churn_over_en_ulb` (50
  steps), each from two fixed seeds. The menu is insert/delete/replace of a
  random content run, a masked footnote, a chapter appended or dropped, two
  chapters moved, a chapter copied between books, NUL runs either side of a
  chapter seam, and a book added or removed;
- every raw edit refreshes the Onion projection; a markup-only edit whose
  projected pass inputs are unchanged reuses target observations while the new
  source map rebinds positions —
  `a_markup_only_edit_reuses_every_observation_and_still_rebinds`, plus every
  footnote step of the churn, which asserts a zero bound and zero maps;
- an unchanged raw-book checksum reuses detached projection/UTF-16 index data,
  while all returned offsets still resolve against the exact string supplied
  to that invocation — `an_identical_update_derives_nothing_and_recopies_no_text`
  and `a_second_publish_maps_nothing_and_republishes_the_same_bytes`; every
  churn step re-checks the offsets, because the cold oracle rebases from the
  very string that step supplied;
- identical chapter content reuses mapping but is counted at both positions —
  `two_identical_chapters_share_one_observation_and_report_both` and
  `a_chapter_copied_onto_another_maps_nothing_and_publishes_both`, plus every
  copy step of the churn;
- seam contributions change only where an adjacent prefix/suffix changed — the
  churn's seam edits pin the chapter boundary for a pass whose fold carries no
  seam state, which is every emitting pass so far. **Open:** the bullet's real
  claim needs the first emitting pass that carries seam state, which is work
  item 3;
- serial and parallel chapter mapping fold to byte-identical results in
  deterministic order — `the_parallel_map_publishes_the_serial_bytes` compares
  both inside one binary, and
  `parallel_publish_byte_equals_the_serial_cold_oracle_over_en_ulb` puts the
  whole-Bible publication against the cold oracle. The gate runs both ways:
  `cargo test --release -p usfm_galley --features parallel -- --include-ignored`.
  The feature is off by default, and measured: for `HygieneBytes` the parallel map
  is 1.7× slower than the serial one, because the map is a tenth of a cold
  publication and allocates per chapter ([evidence.md](evidence.md));
- **Open, and not this slice's:** work item 4's source half. `Pantry` registers
  `Role::Target` only, so "a changed source preserves eligible target
  observations" has nothing to change yet; it lands with Stage 5's source
  corpus.

**Stop gate — ready for review.** Items 1, 2 and 5 landed and their gate
bullets are pinned by name above. The two open lines are item 3's real carry
and item 4's source half; both wait on a consumer that does not exist until
Stage 3 and Stage 5, so they are what to adjudicate before Stage 3 opens.

## Stage 3 — Shared hygiene and nonletter rules

**Goal:** deliver the small engine that replaces most v1 rule modules.

Rule of thumb (ruled 2026-09-02): **one scalar walk per chapter; byte sweeps
as many as are useful.** A byte sweep (range compare, `memchr`) runs near
memory bandwidth over a chapter already in cache and costs nothing to keep
separate. A scalar walk (decode, classify, look at neighbors) is 10–30×
slower and is the cost that must not multiply: every rule that needs class
bits reads them from the substrate walk's observation. Hygiene's byte classes
stay their own sweep; its four scalar classes move onto the substrate walk.

Work:

1. Add dense scalar interning, digit pooling, topology, run composition,
   directed pairs, run lengths, per-book occurrence masks, and the terminal
   follow table. Substrate walk landed (D1a): `sous_core::substrate` maps the
   chapter row and folds a book's seams. Hygiene's four scalar classes now ride
   that walk too (D1b): `scan_scalars` is retired, the row carries a `hygiene`
   site lane, and `sous_core::Brigade` — `(HygieneBytes, Substrate)` — is the
   product pass;
2. Keep sites absent from stored observations. Implement semantic
   `sites(query)` and `sites_many(queries)` operations returning projection-true
   spans that the retained producer can locate. Choose `memchr`, `memmem`,
   optional measured Aho-Corasick, or a
   classifier rescan internally; callers never select the search engine.
   Landed (D2b): `sous_core::sites` rescans a book's current text behind
   `ChapterPass::locate`, one `memmem::Finder` per distinct firing glyph and a
   classifier pass for the pooled digit key, and the caller names patterns
   rather than an engine. Aho-Corasick is measured and not taken; a general
   one-pass byte-class sweep is not built, and the needles/hits/sites per book
   that would justify either are recorded (evidence.md, 2026-09-04);
3. Implement rarity rosters and the G0–G3 evidence ladder. Landed whole
   (D2a-2, completed D3): `sous_core::judge` reads the folded counts and emits
   one pattern row per firing claim on every grain. G2's pools are derived from
   pinned UCD properties into `unicode::pools.rs` rather than hard-coded, so a
   danda and an Ethiopic full stop are terminals beside `.`; G2 shares G3's
   denominator, so the two are entitled together and G2's contribution is the
   coarser *statement*, not a fallback for an abstaining G3. D3 also took
   digits out of runs: a digit breaks a run and joins none, so `600,000` is a
   lone comma between two digits and G0 and G1 tell one story about digits;
4. Implement the fraction-band judge with explicit numerator, denominator,
   support floor, fallback grain, abstention, and union-of-reasons behavior.
   Landed (D2a-2, completed D2b): `JudgingConfig` carries the staircase in basis
   points, a channel under the support floor abstains, and every pattern row
   publishes its own numerator and denominator. The union landed with the sites
   — one maximal run is one `Convention` row whose lane A names the finest
   matched pattern and whose `Reasons` lane carries every rung any pattern
   matched in it. `OuterClass::Edge` counts in the denominator and fires no
   placement row: a book boundary is a fact about the file;
4b. Judge the capital a glyph hands off to. Landed (W5):
   `Channel::SentenceStart` reads the substrate's own `follows` lane against
   `sentence_start_upper_bp` (9,800), so a glyph that almost always precedes a
   capital makes every lowercase letter after it one site for review. It is a
   glyph rule and not a word one — the key is the glyph, the site is the word
   the glyph handed off to, and `Substrate` owns both. The knob is separate
   from `terminal_upper_share_bp` because the two ask opposite questions of one
   count: 80% decides whether the punctuation chose a capital, 98% decides
   whether it failed to get the one it always gets;
5. Add dispersion annotation and the narrowly defined “forgiven but
   clustered” view without making dispersion a conviction gate. The annotation
   landed (D3): `Pattern::books` is books-touched, books-possible is the
   header's `book_count`, and `judge::books_touched` recomputes it from the
   retained aggregates. Nothing gates on it — no threshold, no flag, no config
   — so the "forgiven but clustered" grouping is a front-end read of the rows,
   not an engine verdict.
6. Pack compact count evidence and expose typed rich evidence. The compact
   half landed (D2a-2): the 24-byte pattern table rides the corpus envelope,
   so a convention's argument is published once and a site names it by index.
   D2b put sites on that wire, and `sous --report <out.html>` is the
   self-contained page that reads them back in context. D3 spent one of the
   row's two reserved bytes on `books` and left the other reserved. `--report`
   now writes the v1 "Punctuation & Symbol Inventory" page fed by these
   counts, amber wherever `Findings::patterns()` fired: it is the review
   surface until a wasm handle exists.

Verification gate:

- every required Level 1a/1b example in [rules/](rules/) is pinned at claim
  level;
- entitlement falls back to a coarser comparison rather than to silence;
- convention examples and small-corpus examples abstain/fire for the documented
  reason;
- one maximal run yields one finding with all independently firing reasons —
  met (D2b): `sites::locate` emits one row per run, headlined by the finest
  matched channel, with every matched rung in its `Reasons` lane;
- changing judgment bands maps zero chapters and folds zero books — met;
  `set_config_relocates_from_cached_aggregates_without_mapping` pins that a
  re-judge maps and folds nothing, and re-places its sites from the cached
  aggregates;
- current-text site rescan agrees with the pattern counts it materializes —
  met (D2b): `core/tests/sites_agree_with_counts.rs`, occurrence for
  occurrence over every book of the committed tier.

Fleet gate:

- reproduce the band sweep and report per-corpus p50/p90/p95/max volume,
  examples around each band edge, and any cliffs;
- reproduce the dispersion study before shipping the clustered review group;
- explicitly adjudicate old deterministic/nonletter findings lost, retained,
  or newly produced. Do not call redesigned output byte-compatible.

## Stage 4 — Word conventions

**Goal:** add only the word behaviors already justified by the shared model.

Work:

1. Add short-word packed keys and a rare long-word overflow path only after the
   Level 1 walk is stable. **Superseded and closed:** words fit a `u64` only
   12-23% of the time outside Latin (evidence.md, 2026-09-02), so `WordCount`
   keys by xxh3-64 and carries the scalar count in one byte beside it. There is
   no packed lane and no overflow path.
2. Implement free-position casing observations using the learned terminal
   table and small chapter seam state. **Landed (W1):** `sous_core::words`
   rides beside `Substrate` in `Brigade`, `Channel::Casing` judges the minority
   form of a case-folded word in free positions, and `Words::locate` sites it.
   A word never crosses a masked `\c`, so the fold carries no seam state at
   all. **Completed (W3):** the terminal table is learned after all. The row
   stores the glyph that stood before each word (`Before`), quotes and brackets
   transparent, and the judge asks a `TerminalTable` built from the substrate's
   own follow lane which glyphs this corpus capitalizes after — so `¡`, `;`,
   and the danda force where their corpora put capitals after them and a comma
   forces only where reported speech really follows one. See
   [core/src/words.md](core/src/words.md). **Landed (W1b):** word rows are
   retained at BOOK grain (`ChapterPass::RETAIN_CHAPTERS`) and the corpus word
   tally is resident in `galley::sous::Expediter`, moved one changed book at a
   time instead of re-merged every publication. Caching policy only: not a byte
   of the publication changed.
3. Implement adjacent and punctuation-separated doubled-word observations as
   distinct claims. **Landed (W2):** the same word walk fills a second lane,
   `DoubleCount` keyed by hash alone (16 B), and `Channel::Doubled` judges two
   keys — adjacent and nonletter-separated — against the word's own
   occurrences, so `vous vous` x300 of 9,000 is silent and one `the the` fires.
   Sites span both words and the separator. Productive reduplication recuses
   the whole corpus through `doubles_productive_bp` (300 bp of the vocabulary),
   overridable with `doubles: Auto | Always | Never`. `channels.doubled` ships
   **on**: the fleet run puts it at p50 10 / p90 29 rows per corpus, the glyph
   channels' own volume. Doubling has nothing to do with case, so **uncased
   scripts are judged here and pay for it** — hin2017's word rows go from ~0 to
   4.96 MB a Bible and it fires 14 rows. `Reasons` widened to the full i16 lane
   B for the two new bits. See [core/src/words.md](core/src/words.md).
4. Measure a word-specific evidence floor/band column; do not reuse glyph
   bands after the observed eightfold volume difference. **Landed (W3):** the
   fleet run measured twentyfold, not eightfold — p50 201 casing rows per
   corpus against the glyph channels' p50 10 over all 1,504 corpora
   (`core/examples/word_volume.rs`). `word_bands` is the glyph staircase at a
   tenth of its shares (`Staircase::WORD_STEPS`) and `word_support_floor` is
   20, which puts casing at p50 11 / p90 31 / p95 42; `channels.casing` ships
   on, because that was the condition.
5. **Added by W4, on by default:** `Channel::LetterRun` (channel 8), one row
   per `(letter, run length)` a corpus writes more rarely than that letter's
   own repeat history supports — `theee` against thousands of `ee`. The same
   word walk fills a third lane, `(ScalarKey, [u16; 7])` per letter for run
   lengths `2..=8+` (7-33 KB a Bible, evidence.md W4), and the channel is the
   one out of that walk whose key is a real scalar: the wire row carries the
   folded letter in `glyph` and the run length in the key byte, plus
   `Reasons::LETTER_RUN` (bit 10). Length 2 never fires and a length fires only
   where every shorter one is established on `word_support_floor` runs.
   Uncased scripts are judged. Over the committed tier the shipped defaults
   fire 9 rows across 8 corpora — mean 1.1, max 4 — and every one is a real
   typo, so `channels.letter_runs` ships **on** with no fleet sweep.
   [rules/word-conventions.md](rules/word-conventions.md) carries the rule.
6. **Added by W3, off by default:** `Channel::WordLength`, one row per
   case-folded word standing `word_length_sigma` (4) whole standard deviations
   above the corpus's own occurrence-weighted mean word length. Long end only,
   sited on the word's spans, `channels.word_length = false` — names and
   loanwords are this tail. [rules/word-conventions.md](rules/word-conventions.md)
   carries why it left the idea shelf as a deviation rather than a rule.

Verification gate:

- uncased scripts abstain and avoid expensive case-fold work;
- `David, david said`, bivariant words, cross-verse doubles, French repeated
  forms, names, and productive case variants have explicit tests — all present
  as of W2, `core/src/words/tests.rs`;
- word config re-judges retained observations without rewalking text;
- fleet calibration includes volume tails and representative false/ambiguous
  cases before defaults are accepted.

**Landed, on demand:** `sous_core::typos` and `sous --typos <corpus>` — rare
words one edit from a frequent word, grouped by target. Not a channel: no
`PatternKey`, never judged, never rides `Brigade`. `core/src/typos.md` and
the two 2026-09-07 `evidence.md` rows carry the measurement and the ruling.

**Deferred:** n-gram surprisal, hapax, and compound-split ideas remain
separate probes. They do not ride this stage merely because word tokens exist.
Length is the one that left, as an off-by-default channel and a logged
deviation, not as an approved rule.

## Stage 5 — Aligned source comparison and proportionality

**Goal:** make source pairing a first-class library capability and land the
first source-compared rule without importing v1's surrounding machinery.

Work:

1. First add chapter and verse rows to Onion's masked-text/reverse-map
   projection, exposing borrowed `chapters()` and `chapter.verses()` views.
   Keep that work in the Onion workspace. The Sous CLI adapter composes
   target/source Onion views without making `sous-core` depend on Onion.
   Landed: `galley::sous::OnionBook` is the `ProjectedBook` over Onion's mask
   and TOC, and sous reads `chapters()`/`verses()` off it.
2. Accept the complete optional source corpus on each applicable Galley
   invocation. Retain only checksum-keyed derived source observations and
   coordinate indexes between calls; map target and source per-unit grapheme
   lengths independently, then pair by exact key plus occurrence ordinal
   during analysis.
   Also accept an addressless `BOOK C:V<TAB>text` vref source loader in the CLI;
   only the target side must provide an addressable projected book. Preserve
   `<range>` placeholders so a concrete row followed by contiguous placeholders
   reconstructs one interval unit rather than silently dropping bridge shape.
   The same loader may produce an addressable target projection for vref fleet
   work: projected offsets locate back to the original vref line and typed
   designator rather than pretending to be raw-USFM offsets.
   Landed (S1): `Pantry::Role::Reference` retains `Toc` plus one grapheme count
   per verse — 1.17 MB against a target's 7.29 MB over `en_ulb` — and the
   target lane rides the substrate's chapter row. `sous-cli`'s `--source` takes
   either producer; the vref loader fuses `<range>` placeholders into one
   interval unit and lays its rows out in key order, because a projection has
   to be monotone and a vref export's line order is not always.
3. Derive ordered per-book ratio vectors and a project pool. Recompute
   medians/MADs from the small ratio sets; do not build a mutable order-
   statistic cache. Landed (S1): recomputed per publication, no cache; the
   cost is a ledger row and the lever behind it is named there.
4. Port v1's asymmetric one-sided MAD with pooled fallback, preserving its
   accepted semantics and defaults; use the survey to catch port drift.
   Landed (S1): `sous_core::proportionality`, with v1's `0.6745` scale and its
   three-deviation per-side floor. The survey caught no drift.
5. Resolve presence/shear ownership (open gate 1). Retain the current
   proportionality defaults and use the paired survey as their regression
   gate. **Still open**, and deliberately: presence and shear stay parked. S1
   reports unpaired keys as per-book counts in the CLI and as
   `Paired::facts` in the library, and emits no row for either.

Verification gate:

- pairing is unaffected by independent source/target array order and handles
  duplicate occurrence ordinals exactly;
- absent source, source replacement, empty units, small books, and project
  fallback match the documented contract;
- exact bridges and bridge-versus-constituent ranges coalesce to one ratio;
  ambiguous duplicates and partial overlaps abstain;
- chapter edit and cold results are identical under randomized paired edits;
- source choice legitimately changes results without invalidating target-only
  observations;
- reproduce the v1 paired survey: clean-fleet volume, seeded truncations,
  small-book behavior, source sensitivity, and shear exclusions;
- every divergence from v1 receives “fix, accepted model change, or upstream
  ownership” disposition.

**Status after S1:** every gate bullet is met except the shear half of the
last-but-one, which has no rule to exclude anything from yet. The dispositions
are in `planning/RESUME-2026-09-04.md` §8 and the survey rows in
`evidence.md` (2026-09-07). What Stage 5 has NOT built, and did not promise to:
untranslated-word detection, the vref loader's target-side projection, and any
resident cache for the pairing.

## Stage 6 — Behavioral bookend and port closure

**Goal:** prove v2 covers the intended product, not merely that its internals
are elegant.

Work:

1. Promote the CLI's initial debug print into a deterministic dump format from
   rich findings and compact records. Do not promise additional public formats
   until a consumer requires them.
2. Keep three execution tiers: synthetic, a small script-diverse corpus set,
   and the full fleet. The corpus manifest pins inputs; derived local blobs are
   optional and disposable.
3. For unchanged deterministic behaviors, compare exact normalized finding
   rows against v1.
4. For redesigned statistical behaviors, produce a divergence ledger with
   counts and representative samples:
   - v1 noise correctly removed;
   - v1 useful catch lost;
   - v2 useful catch added;
   - ambiguous difference requiring owner adjudication.
5. Reconcile every PO checklist item to shipped, deferred with a gate, or
   owned outside Sous.

Exit gate:

- no unexplained lost useful class;
- full-fleet run reproducible from the manifest;
- performance and retained-memory results measured on production code;
- charter/rules/roadmap reflect actual behavior;
- owner explicitly accepts any intentional statistical movement.

## Stage 7 — Galley and consumer boundaries

**Goal:** harden the minimal Galley coordinator only after pure rule behavior
and the transport contract are stable.

Work:

1. Harden the minimal Galley introduced in Stage 2 into the public resident
   derived-state owner: analyze complete target/optional-source inputs, change
   judging config, query sites/details, and manage suppressions. Keep document
   mutation and rope ownership outside Galley; expose no splice protocol.
2. Preserve one invalidation policy and complete packed snapshots. Do not port
   v1 methods solely for compatibility.
3. Decide whether content-derived analysis identity and persisted-snapshot
   validation are currently needed. Storage remains host-owned.
4. Generate WASM/TypeScript schema from the Rust wire source. Avoid mirrored
   per-finding structs across the boundary.
5. Add receiver reconciliation only with a real list/editor consumer that can
   demonstrate the object-identity benefit.
6. Harden CLI help, tests, generated usage specification, and distribution.
   Build a read-only editor view only if scroll-to-finding, pattern expansion,
   band controls, or suppression feedback need a real visual consumer.
7. The wasm handle exists (X1). `galley::wasm::Galley` is one opaque handle
   over `Expediter<Brigade>`: whole books by id, `publish()` out as the corpus
   buffer `sous-chef/reader.ts` reads, `Knobs` in and out. The publication is
   byte-identical to the native one, pinned by three tests over one committed
   fixture corpus (`galley/src/wasm.md`). Still absent from it: fingerprints,
   `changedSinceUpdate`, lint, find, detail, suppression.

Verification gate:

- resident and stateless snapshots are byte-identical for the same inputs;
- edit/undo, source add/remove/replace, config-only rejudge, deletion, and
  analysis failure have explicit lifecycle tests;
- stale document versions and stale detail requests fail closed;
- transferred-buffer and decoder smoke tests run in the actual consumer
  environment;
- no worker or background scheduler is added unless main-thread measurement
  crosses an agreed frame-budget gate.

## Deferred lanes

- delimiter pairing after its bounded carry probe;
- untranslated/source-copy residue after counterexample adjudication;
- word idea-shelf models after individual claim/calibration packets;
- suppression UI and persistent policy beyond Galley's initial in-memory
  workflow;
- alternative aligned-unit implementations for non-scripture documents;
- binary snapshot diffs, worker execution, persistent observation caches, and
  corpus rollups only after measurement.

## Evidence

Every measurement this roadmap's architecture rests on is in
[evidence.md](evidence.md): the roofline and fleet-differential runs with their
dates, machines, and commands, and the v2 probe results behind the starting
architecture. Rejected implementations keep their code in
[experiments/](experiments/).
