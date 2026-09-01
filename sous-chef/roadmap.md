# Sous Chef 2 roadmap

Sous Chef 2 is a deliberate rewrite of the small, language-agnostic content
reviewer inside `scripture-sous-chef`. The old repository is a donor and a
behavioral comparator, not an architecture to preserve. The exploratory v2
probes have already shown that a streaming counter model can cover most of the
desired checks with much less code and state.

This roadmap is capability-led. It records what to carry, what to leave
behind, the order in which contracts become real, and the gates that may still
say no. It does not authorize unreviewed implementation beyond the named
stage. [charter.md](charter.md) and [rules.md](rules.md) are the two durable
semantic authorities.

## Current baseline

Already present:

- a Cargo workspace with production `core`/`cli` crate boundaries;
- checked projected-book domain types and a producer-neutral iterator contract;
- an Onion CLI adapter that composes the existing `Mask` and `Toc`, including
  projected chapter/verse ranges and reverse location to discontinuous raw
  source spans;
- a typed `usage-rs` CLI that walks one USFM target and prints debug projection
  rows without promising a stable output format;
- an explicitly disposable probes crate;
- a manifest-and-fetch corpus lane with no Git LFS dependency;
- measured prototypes for byte hygiene, streaming classification, the shared
  nonletter substrate, per-chapter rows, reduction, bands, dispersion, and
  grapheme fast-path fidelity;
- a clean ownership direction for Onion TOC/content masks and file-relative
  findings;
- a catalog mapping the PO checks into Sous, Onion/editor, or unresolved
  lanes.

The production library has started only at the Stage 0 input boundary. Probe
types and APIs remain evidence, not a compatibility surface.

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
| packed 16-byte finding snapshots | **migrate concept, change layout** | Keep versioned fixed records, append-only codes, generated consumers, fail-closed decode, and complete snapshots. Change from `key_idx + u16 verse offsets` to `u16 book_idx + projected-book u32 from/to`; retain the producer projection to locate source positions, and keep count lanes as saturating display digests. |
| lazy rich finding args | **keep as a capability** | The compact record is not the full truth. Final host spelling waits until Galley is designed. |
| content-derived analysis IDs | **evaluate during Galley** | Useful for cache validation, but not required to prove core rules or the first codec. Do not make persistence part of core. |
| resident Galley state machine | **audit, then redesign** | Keep one stateful owner over a pure core and complete-snapshot semantics. Port only calls an actual host needs; do not preserve the v1 API for compatibility. |
| JS reconcile-in-place helper | **evaluate with the consumer** | Receiver-owned identity reuse is sound, but implement only when a real UI consumes snapshots. |
| WASM/TS mirror structs | **ditch** | The binary schema is the boundary. Generate declarations and keep hand-written adapters thin. |
| rule-development contract | **migrate and shorten** | Preserve claim/counterclaim, evidence roles, typed observations, calibration, and surface/verification gates in the shared contract in `rules.md`. |
| byte-identical full-fleet oracle infrastructure | **do not port wholesale** | Keep small/fleet dump-and-compare capabilities. Exact parity gates unchanged deterministic ports; redesigned statistical rules use explicit divergence adjudication. |
| corpus blobs and Git LFS-era storage | **ditch** | Keep checksum-pinned fetch manifests. Add a local derived blob only if file-open/parse cost becomes a measured bottleneck. |
| v1 playground | **replace** | Do not rebuild the visual playground as the development harness. Grow the real CLI as a thin walking consumer from the first executable capability; consider a visual consumer later only for editor-specific workflows. |

## Decisions already made

- Three durable design files only: charter, rules/examples, roadmap/evidence.
- A neutral projected-book view is the input boundary. Onion's masked
  text/TOC and a vref loader can both produce it.
- Core findings use an immutable book-table index plus projected UTF-8 byte
  spans. The retained producer projection locates numeric verse designators,
  raw source spans, and UTF-16 editor positions outside core.
- Book is the discourse unit; chapter is the independently mappable rebuild
  unit; ordered reduction stitches boundary state.
- Counts are config-free; judgment is a cheap pure read over observations.
- No retained corpus rollup, scalar tape, site list, worker, or scheduler
  without a measured promotion gate.
- Fixed-width serializable findings are designed before rule implementation.
- Rule discriminants use explicit `u8`, not a packed 6-bit field.
- `u16` numerator/denominator lanes saturate and are display digests; raw
  evidence remains wider.
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
independently, so a source mutation never invalidates target-only work.

## Opening implementation ledger

This is the first-pass coordination map. It records which earlier sketches are
now executable contracts and keeps donor code from becoming an accidental API.

| capability | production owner and current shape | next work |
| --- | --- | --- |
| projected offsets | `sous-core::TextRange`, checked as half-open projected UTF-8 bytes | reuse in rich findings and the packed codec |
| chapter/verse input | `ProjectedBook` yields `Chapter` and ordered `Verse { VerseKey, TextRange }` rows | add aligned-pair semantics after the input cut is reviewed |
| Onion projection | CLI adapter materializes verse text once, derives ranges through `Mask::project_source`, and locates with `Mask::to_source` plus `Toc::locate` | move only reusable conveniences into Onion when a second host needs them |
| vref projection | semantic contract is settled; no production loader yet | implement after the Onion path pins duplicate and bridge fixtures |
| nonletter substrate | `donor/src/probe3.rs` and `donor/src/rows.rs` are measured donors only | port consumer-led classifier bits after Stage 0 closes |
| finding transport | 16-byte layout and book-table semantics are chartered but not coded | next independent Stage 0 slice: codec, header, golden vectors, rejection tests |

Do not begin classifier or rule ports merely because the input trait exists.
The packed finding boundary and aligned-unit fixtures remain the Stage 0 stop
gate.

## Stage 0 — Freeze the foundation contracts

**Goal:** make the smallest hard-to-reverse decisions executable before rule
code creates pressure to bend them.

Work:

1. Define production domain types for projected text ranges, chapter rows,
   verse/bridge keys, typed input refusal, and the neutral borrowed book view
   consumed by `sous-core`. For Onion, compose existing mask spans and TOC
   anchors rather than defining duplicate stored rows. **Initial cut landed;
   raw file identity and document version remain host/Galley work.**
2. Define the aligned-unit semantic tests using a tiny synthetic target/source
   pair, including duplicate keys, occurrence ordinals, empty units, absent
   units, chapter seams, and incompatible ordering.
3. Define the 16-byte finding record and versioned header in a tiny codec
   module. Keep rich `Finding` separate from `PackedFinding`.
4. Hand-assign the dense active rule-code table for wire version 1. Reserve no
   ranges and keep no retired entries; removal or renumbering requires a new
   wire version. Do not pre-allocate a code for every idea-shelf rule.
5. Define the immutable ordered book-table contract used by `book_idx`, then
   generate Rust test vectors plus one language-neutral schema artifact; prove
   encode/decode round trips and malformed-buffer rejection.
6. Replace the placeholder CLI with a minimal `usage-rs` command declaration.
   It accepts a deliberately narrow target input, constructs the immutable
   book table deterministically, and prints only debug output until real
   findings exist. Keep all Onion adaptation in the CLI, not `sous-core`.
   **Initial one-USFM walking command landed.**

Verification gate:

- layout size/alignment assertions and byte-exact golden vectors;
- invalid book indices, reversed/out-of-bounds spans, bad
  versions/codes/flags, count mismatch, saturation, and overflow all fail
  explicitly;
- a finding spanning astral text remains projection-byte-correct, locates to
  its producer source, and projects to UTF-16 through an Onion adapter test;
- schema generation is deterministic and `git diff --exit-code` clean after a
  second run.

**Stop:** no classifier or rule port until this gate is reviewed.

## Stage 1 — Unicode and content-walk foundation

**Goal:** establish the one walk every later observation trusts.

Work:

1. Port the minimal Unicode classifier generator with pinned Unicode inputs.
   Start from consumer questions, not the donor bit layout: alphabetic,
   casing, decimal digit, whitespace, mark, punctuation/symbol, extender,
   grapheme-complex, and only proven refinements.
2. Commit generated compact ranges plus an at-runtime BMP lookup strategy.
   Compare a static expanded table against a local snapshot only if both are
   viable in the new workspace; choose by measured whole-walk cost and memory.
3. Implement masked iteration over Onion's projection in projected UTF-8
   coordinates. Chapter and verse APIs derive ranges from the existing `Mask`
   and `Toc`; they do not allocate per-chapter/per-verse strings or store a
   parallel key table. Implement numeric `locate()` by composing
   `Mask::to_source` with `Toc::locate`, without scripture-key strings.
   **The CLI adapter proves this shape; promotion to a shared Onion convenience
   waits for a second host.**
4. Implement grapheme-safe emitted boundaries with the fast atom rule and a
   correctness fallback/check for complex cases.
5. Implement deterministic hygiene scans over raw bytes with content-mask hit
   validation.
6. Expose the implemented hygiene findings through the same CLI command. This
   is the first real consumer path, not a separate playground API.

Verification gate:

- exhaustive generated-table agreement with source predicates/UCD data;
- script-diverse UAX/grapheme differential, including ZWJ/ZWNJ and the regional
  indicator edge;
- every reported span lies in projected content and on UTF-8/grapheme
  boundaries, and `locate()` returns the correct raw producer position;
- CRLF, astral, markup-only, empty-span, and split-mask synthetic cases;
- production benchmark remains within an explicitly reviewed regression band
  of the probe floor; absolute probe numbers are evidence, not a promise.

## Stage 2 — Galley chapter cache and ordered reduction

**Goal:** prove incremental correctness before the shared model grows.

Work:

1. Define the pass contract with a chapter observation, schema stamp, minimal
   associated `Carry`, disposable aggregate, and consumers. `Carry = ()` for a
   pass with no seam behavior.
2. Add the workspace's minimal Galley coordinator. It owns target text/TOCs,
   optional source text/TOCs, config, and independently content-addressed
   target/source chapter observations; the pure Sous map/reduce functions own
   no resident cache.
3. Implement ordered book reduction over independently mapped chapters. Keep
   only the smallest carry facts needed for nonletter adjacency, casing
   terminal state, and doubled-word state; do not create a generic monoid
   framework or retained book contribution layer.
4. Implement target replacement/insertion/deletion and
   `update_source_text`/source removal. A source mutation preserves target
   observations, remaps only affected source chapters, and reruns cheap
   pairing and source-rule reduction.
5. Mark-and-sweep unreachable chapter observations. Keep carry, paired ratios,
   and book/corpus aggregates derived on demand; add no rollup cache.

Verification gate:

- cold analysis equals chapter-at-a-time rebuild after every chapter is
  replaced in sequence;
- randomized edit/insert/delete operations equal a fresh rebuild after each
  step;
- every raw edit refreshes the Onion projection; a markup-only edit whose
  projected pass inputs are unchanged reuses target observations while the
  new source map rebinds positions;
- identical chapter content reuses mapping but is counted at both positions;
- seam contributions change only where an adjacent prefix/suffix changed;
- serial and parallel chapter mapping reduce to byte-identical results in
  deterministic order.

## Stage 3 — Shared hygiene and nonletter rules

**Goal:** deliver the small engine that replaces most v1 rule modules.

Work:

1. Add dense scalar interning, digit pooling, topology, run composition,
   directed pairs, run lengths, per-book occurrence masks, and the terminal
   follow table.
2. Keep sites absent from stored observations. Implement semantic
   `sites(query)` and `sites_many(queries)` operations returning projection-true
   spans that the retained producer can locate. Choose `memchr`, `memmem`,
   optional measured Aho-Corasick, or a
   classifier rescan internally; callers never select the search engine.
3. Implement rarity rosters and the G0–G3 evidence ladder.
4. Implement the fraction-band judge with explicit numerator, denominator,
   support floor, fallback grain, abstention, and union-of-reasons behavior.
5. Add dispersion annotation and the narrowly defined “forgiven but
   clustered” view without making dispersion a conviction gate.
6. Pack compact count evidence and expose typed rich evidence.

Verification gate:

- every required Level 1a/1b example in `rules.md` is pinned at claim level;
- entitlement falls back to a coarser comparison rather than to silence;
- convention examples and small-corpus examples abstain/fire for the documented
  reason;
- one maximal run yields one finding with all independently firing reasons;
- changing judgment bands maps/reduces zero chapters;
- current-text site rescan agrees with the pattern counts it materializes.

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
   Level 1 walk is stable.
2. Implement free-position casing observations using the learned terminal
   table and small chapter seam state.
3. Implement adjacent and punctuation-separated doubled-word observations as
   distinct claims.
4. Measure a word-specific evidence floor/band column; do not reuse glyph
   bands after the observed eightfold volume difference.

Verification gate:

- uncased scripts abstain and avoid expensive case-fold work;
- `David, david said`, bivariant words, cross-verse doubles, French repeated
  forms, names, and productive case variants have explicit tests;
- word config re-judges retained observations without rewalking text;
- fleet calibration includes volume tails and representative false/ambiguous
  cases before defaults are accepted.

**Deferred:** n-gram surprisal, hapax, length, and compound-split ideas remain
separate probes. They do not ride this stage merely because word tokens exist.

## Stage 5 — Aligned source comparison and proportionality

**Goal:** make source pairing a first-class library capability and land the
first source-compared rule without importing v1's surrounding machinery.

Work:

1. First add chapter and verse rows to Onion's masked-text/reverse-map
   projection, exposing borrowed `chapters()` and `chapter.verses()` views.
   Keep that work in the Onion workspace. The Sous CLI adapter composes
   target/source Onion views without making `sous-core` depend on Onion.
2. Store optional source text in Galley. `update_source_text` is its mutation
   boundary; map target and source per-unit grapheme lengths independently,
   then pair by exact key plus occurrence ordinal during analysis.
   Also accept an addressless `BOOK C:V<TAB>text` vref source loader in the CLI;
   only the target side must provide an addressable projected book. Preserve
   `<range>` placeholders so a concrete row followed by contiguous placeholders
   reconstructs one interval unit rather than silently dropping bridge shape.
   The same loader may produce an addressable target projection for vref fleet
   work: projected offsets locate back to the original vref line and typed
   designator rather than pretending to be raw-USFM offsets.
3. Derive ordered per-book ratio vectors and a project pool. Recompute
   medians/MADs from the small ratio sets; do not build a mutable order-
   statistic cache.
4. Port v1's asymmetric one-sided MAD with pooled fallback, preserving its
   accepted semantics and defaults; use the survey to catch port drift.
5. Resolve presence/shear ownership (open gate 1). Retain the current
   proportionality defaults and use the paired survey as their regression
   gate.

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
   owner: update document/TOC, `update_source_text`, remove source, change
   judging config, analyze, query sites/details, and manage suppressions.
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

## Compact evidence record

These probe results justify the roadmap's starting architecture. They are not
production guarantees; rerun the relevant probe when a production choice
depends on one.

| question | observed result | architectural consequence |
| --- | --- | --- |
| byte hygiene cost | SWAR range scans about 5.3 GiB/s; fixed needles 2.3–41 GiB/s across stress corpora | rescan; do not retain hygiene state initially |
| stream versus scalar tape | streaming was about 1.3–2.3× faster; tape cost worsened with corpus size | no ambient materialized tape |
| full shared substrate | about 28 ms on the English probe corpus versus roughly 257 ms v1 cold analysis | simple whole-corpus/whole-book passes are viable |
| expanded feature substrate | roughly 12–31% above the first shared-counter cut | counter-shaped additions can share the walk cheaply |
| grapheme atom differential | after fixing extender classification, one nonletter-involving mismatch over 1,504 corpora | fast walk plus grapheme-safe emission is viable |
| per-chapter row reduce | about 17–25 µs for 8k–12k glyph rows; edit plus reduce under 50 µs in the probe | derive aggregates on read; no rollup cache |
| band sweep | default staircase gave about p50 10, p90 36, p95 44 rows/corpus; low cloud tracked `0.8/sqrt(n)` | readable fraction bands are credible shipping candidates |
| dispersion | clustering changed gradually with share; genre remained a confound; above-band clustered rows about 0.3/corpus | annotation/ranking only, never a hard gate |
| packed v1 wire donor | fixed 16-byte buffers made wasm/transfer/decode nearly flat and far cheaper than object arrays in the old measurements | preserve fixed binary snapshots, but redesign addressing |
| proportionality paired survey | project scope covered small books; 10–20% chops were nearly invisible; source choice materially changed results | keep dual scopes and narrow claims; reproduce before defaults |

Current probe entry points live under `spikes/probes`; `cargo test -p probes`
and the Criterion benches are the local starting points. The old repository's
calibration notes remain historical evidence in git and in the donor checkout;
they are intentionally not duplicated into this three-file authority set.
