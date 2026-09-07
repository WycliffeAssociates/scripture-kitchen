# The Pantry

Galley's one caching layer. Everything galley keeps between calls is here or on
a store this defines: chunk products, per-book products, retained text, and
every derived value the Expediter reuses.

```js
const g = new Galley(16 << 20);        // the ceiling, in bytes
g.update("books/MRK.usfm", text);      // → "MRK"
g.publish();                           // one complete corpus publication
g.misses();                            // chunk units computed, cumulative
g.entryCount();                        // chunk units resident
g.residentBytes();                     // everything above, summed
```

```rust
pantry.update("books/mrk.usfm", Role::Target, &text)?   // -> Entry for MRK
    .lint()          the folded lint report, fed from the retained text
    .mask()          the verse-text projection, no text needed
    .text()          Ok(&str) — or Err(NoText) under Retain::ProductsOnly

pantry.book(&id)                            -> Option<Entry<'_>>
pantry.changed_since_update(&id, &edited)   -> [1_284..2_006]   // one chapter
pantry.books(Role::Target)                  -> [(id, GEN), (id, MRK)]

pantry.lint(&loose)                         // the four free-text doors: loose
pantry.parse(&loose, opts)                  // text the host holds, served off
pantry.parsed(&loose, opts)                 // the same content-addressed
pantry.masked(&loose, &filter)              // chunks a registered book warmed

pantry.chunk_stats()   -> ChunkStats { misses, hits, evictions, len, resident_bytes }
pantry.budget()        -> Budget { ceiling: 16_777_216 }
pantry.tally()         -> Tally { pinned, hot, rebuildable }
pantry.resident_bytes() == pantry.tally().total()          // by definition
```

`galley/docs/analysis-host.md` states the ownership contract this implements;
`sous-chef/charter.md` ("Hosts own lifecycle and presentation") states the same
rule from Sous's side. `galley/src/sous/expediter.md` states what a publication
DOES with these stores; this note states what they are keyed by and when they
go.

## The three tiers

| tier | what is in it | who holds it | evicted |
| --- | --- | --- | --- |
| **pinned** | a `Target`'s text, its `Toc`/`Mask`/`Utf16Table`/`Fingerprint`, a `Reference`'s verse lanes, the generation rings that name them, and the id lists — canonical order, the hot set, the tally — that name them a second time | `Pantry::books`, `Pantry::targets`/`references`, `Expediter::generations`/`hot`/`cooling`/`tallied`/`project` | never, until `remove` |
| **hot** | the hot set's per-chapter site rows | `Expediter::chapter_sites` | with the hot set |
| **rebuildable** | chunk products, and every content-addressed derived value | `Pantry::chunks`, the Expediter's `derived::Store`s | chunks by the budget; the rest by the sweep |

**In this slice the ceiling is enforced on the rebuildable tier's chunk
products and nowhere else; the other tiers are counted against it and reported,
never evicted.** `resident_bytes()` IS `Tally::total()` — one arithmetic, so no
byte can be reported and attributed to nothing. What makes the total itself a
comparison rather than a claim is a counting allocator:
`galley/tests/aggregate_accounting.rs` holds it to a tenth of the bytes actually
live over a whole Bible. Enforcement is a later measured slice.

Pinned is pinned for a reason that is not sentiment: placing a target's finding
rescans its current text, so a target that lost its text could be judged and
never sited. Rebuildable is safe to lose for a reason that is not luck: every
key names everything its value is a function of, so a lost entry is a miss and
never a wrong answer. Nothing here is invalidated — moved inputs simply miss.

## Every store, its key, and what moves it

This is the law. A key that omits an input its value depends on is a
correctness bug, not a performance one.

| store | key | value | moves when | tier |
| --- | --- | --- | --- | --- |
| chunk cache | (chunk content hash, chunk 0's carry-outs) | one chunk's CST, tokens, local lint report | that chunk's bytes, the declared `\usfm` version, or the is-scripture bit | rebuildable |
| `books` | `BookId` | `Toc`, `Mask`, `Utf16Table`, `Fingerprint`, lanes, text | `update` with a different `RawChecksum`, role, retention, or `SourceLanes` | pinned |
| `observations` | `ObservationKey` | one chapter's `P::Observation` | the chapter's projected text, its rebased verse rows, or `P::SCHEMA` | rebuildable |
| `chapter_tables` | `RawChecksum` | the book's `(ObservationKey, start)` rows | any edit at all, markup included | rebuildable |
| `aggregates` | `RawChecksum` | the book's `P::Aggregate`, book-index free | the same | rebuildable |
| `firing` | `RawChecksum` | `(TableHash, FiringHash)` | the book's text, or the pattern table's CLAIMS | rebuildable |
| `sites` | `RawChecksum` | `(FiringHash, TerminalHash, [SiteRow])` | the book's text, its firing set, or the corpus's terminal table | rebuildable |
| `chapter_sites` | `(ObservationKey, FiringHash, TerminalHash)` | that chapter's rows, chapter-relative | any of those three | hot |
| `paired` | (target `RawChecksum`, source `RawChecksum`, words walked) | the book's ratios, presence rows, source-copy runs | either side's text, or the word walk switching on | rebuildable |
| `verdicts` | (terminal table, judging config, doubling recusal) | the word channels' patterns | any of the three; otherwise only the delta's keys are re-judged | rebuildable |

Four rules generate that table, and each is one an earlier pass got wrong:

- **Content, never position.** `FiringHash` and `TableHash` read a pattern's
  glyph, channel and key — never its table index. A publication renumbers the
  pattern table whenever any other book's counts move a denominator, so an
  index-keyed cache would miss on every keystroke anywhere in the corpus. A
  cached row names its pattern by content (`PatternRef`) and resolves to *this*
  publication's `PatternIndex` at replay.
- **Both sides of a comparison.** `paired` keys on both raw checksums, because
  either side moving is a different sample.
- **The evidence a walk reads, not only the text it walks.** `TerminalHash` is
  in the site keys and NOT folded into `FiringHash`: the word walk reads the
  corpus's terminal table to split free occurrences from forced, and a firing
  set is position-blind about exactly that. The same rows fire while the table
  decides differently which of their occurrences are free.
- **A property of the entry, not a knob.** `paired`'s third key member is
  whether the source-copy words were WALKED — a pairing made without the walk
  holds no runs and cannot answer for one that wants them. `source_copy_min_run`
  is not in the key: the runs are cached from a floor of two and the knob
  filters them at judging time.

Config is in none of these keys except `verdicts`, because neither `map` nor
`fold` is handed it: `set_config` is a re-judge, never a re-map or a re-fold.

## Two questions, two answers

An editor asks two different things about an edited book, and one `Fingerprint`
— chunk starts plus one xxh3-128 per chunk, about 1 KB per book and no text —
answers both:

```text
dirty(book)   = baseline.checksum() != current.checksum()   // POSITIONAL
rework(book)  = baseline.changed_chunks(&current)           // SET MEMBERSHIP
```

Dirty is positional: a moved chapter is a different byte string on disk, so the
file is dirty and the save button lights up. Rework is set membership: a moved
chapter's content products are still valid, so only chunks whose checksum the
baseline never saw need re-lexing or re-mapping — `changed_chunks` returns their
ranges *in the current text*.

Every diff names both sides. The Pantry keeps only the previous fingerprint per
id, so `changed_since_update` is free; an editor keeps its own on-disk baseline
(taken at load, refreshed at save) and diffs that against the current one. No
method silently picks a reference point.

## The two hashes

`RawChecksum` is the xxh3-128 of a book's raw bytes — the whole-book form of the
per-chunk hash `galley::chunks` prints as hex. A Sous `ObservationKey` is a
different newtype over the same algorithm, keyed on a chapter's *projected*
inputs plus a pass schema stamp. Keeping them distinct is what lets a
markup-only edit move the raw checksum, invalidating the projection and the
UTF-16 table, while the observation key stands still and the observation is
reused with rebased coordinates.

`ObservationKey` deliberately excludes the chapter's own address, which is what
lets one observation serve identical chapters in two books while the fold still
counts both positions. A pass whose `map` reads `ChapterInput::key` would break
that and does not belong behind this cache.

## The text rule

**`update` is the only method that takes a registered book's text.** A target
book's text is retained as last updated, so analysis, publication, and find run
against the copy and nothing is resent per call. The editor's buffer is
canonical; the copy is exactly as current as its last update. The four
free-text doors are the exception that proves it: `lint`, `parse`, `parsed` and
`masked` take LOOSE text the host holds, register nothing, and serve off the
same content-addressed chunks — so a book already in the Pantry is already warm
for them.

**A `Target` cannot opt out.** Placing a target's findings rescans its current
text — a pattern is a corpus fact with no coordinates, and `sites::locate`
gives it some — so a target that kept no text could be judged and never sited.
`update_with(.., Role::Target, Retain::ProductsOnly, ..)` returns
`Err(PantryError::TargetNeedsText { id })` rather than registering a book
publication would later fail on. `Retain::ProductsOnly` is what
`Role::Reference` takes by default, and `PantryError::NoText { id }` is the
refusal a text-needing method answers with. `update` picks the ROLE's own
retention — a target keeps its text, a reference keeps none — and `update_with`
is how a host overrides that; `Retain::Text` on a reference is accepted and
pointless, since no operation on one reads text.

There is no splice API and never will be: the only mutation is whole-book
replacement under a caller-chosen opaque id, which is idempotent and cannot
shift coordinates inside a book. A book the caller forgets to resend is stale
as a whole and heals on its next update.

`update` returns an `Entry` — a per-book handle borrowing the Pantry mutably,
because its `lint`/`parse` pass-throughs run the chunk cache — and
`pantry.book(&id)` reopens one later. The `Entry` is the one way in.

A host running Sous mutates through `Expediter::{update, update_with, remove}`
instead, which forward here; the Expediter's own `pantry()` is `&Pantry`, so a
book cannot be registered behind its caches' backs —
`galley/src/sous/expediter.md`.

## Roles and retention

A book is registered as `Target` — full detached products, publishes findings —
or as `Reference`: `Toc` plus one projected grapheme length per verse, and —
only if the caller asks for it — that verse's sorted deduplicated 32-bit word
hashes. Nothing else, because a reference corpus never publishes a coordinate
and nothing ever asks it for text. Both roles are live; `books(role)` lists each
in the same canonical order and neither sees the other.

The asymmetry is the point, and it is measured (evidence.md, U1 (a)): over the
committed 66-book `en_ulb`, a target's own products are **7.29 MB** (162% of
the 4.51 MB raw, of which 4.51 MB is the retained text) and a reference's are
**1.17 MB** (26%) for lengths alone, **3.94 MB** (87%) with the word lane. The
length rows are 12 B each, 31,101 of them, 0.37 MB; the word lane is **2.77 MB**
(629,890 distinct-word slots, 20.3 per verse); the rest is the `Toc` both roles
keep.

Retained per `Target` book — the text under `Retain::Text`, and roughly 25% of
it again in products:

| Product | Source | Why it survives the string |
| --- | --- | --- |
| chunk products | the chunk cache | content-addressed; an unchanged chapter is never re-lexed |
| `Toc` | `onion::toc` | chapter/verse anchors in raw bytes |
| `Mask` | `onion::mask`, verse-text filter | owned ranges + starts; already detached |
| `Utf16Table` | `mise::utf16` | byte → UTF-16 with no source present |
| published length | the table | a publication's `published_len` |
| `Fingerprint` | `galley::pantry::fingerprint` | the baseline for the next update |
| the text | `update`'s argument | `Retain::Text`; `resident_bytes` counts it |

And per `Reference` book, which is the whole list:

| Product | Source | Why it survives the string |
| --- | --- | --- |
| chunk products | the chunk cache | budget-bound and shared; a reference is parsed once |
| `Toc` | `onion::toc` | chapter/verse anchors, the book's identity |
| verse lengths | `sous_core::source_lengths` over the transient mask | 12 B per verse; the source half of a length ratio |
| verse words | `sous_core::SourceWords::of`, opt-in | the source half of a copy run |
| `Fingerprint` | `galley::pantry::fingerprint` | the baseline for the next update |

`Pantry::text_bytes()` sums the retained text alone and
`Fingerprint::resident_bytes` is public, so a host can read `resident_bytes() -
chunk_stats().resident_bytes - text_bytes()` for the Pantry's own per-book
products without a residual.

Order is canonical by `BookKey` — `mise::books::canonical_rank` over
`BOOK_CODES`, which is the USFM spec's order — ties broken by id, so
`BookIndex` never depends on the order updates arrived in. Two ids carrying the
same `\id` are both present; Galley never parses an id.

## The word lane is opt-in, and turning it on needs the text again

`Retain` says whether a target keeps its text; `SourceLanes` says which verse
lanes a reference derives, and the default is `Lengths` — the 1.17 MB shape.
`Expediter::update` and `update_with` pass `LengthsAndWords` only while the
CURRENT judging config would judge with it (`LengthConfig::source_copy`), so a
host that never turns the lane on never pays 2.77 MB per declared Bible.

**A config flip does not reach back into a registered reference.** Turning
`source_copy` on after the sources are loaded leaves them with lengths only,
and the honest consequence is stated rather than hidden: **the host re-sends
those references' text**, which is one `update` per reference with the same
bytes. The Pantry does not serve that from the cheaper entry — `SourceLanes` is
part of the idempotence check — and the publication reports how many books were
in that state through `Expediter::last_wordless_references()`, one line per
SOURCE
(`lastWordlessReferences()` across the wasm wall, `sourcecopy unavailable BOOK`
from the CLI), so "no rows" never quietly means "no lane".

A reference derives the mask to project its verses and then drops it: both
lanes are built once, at `update`, by the same
`sous_core::proportionality::source_lengths` and `sous_core::SourceWords::of` an
Onion or a vref producer uses, so the two sides of a ratio cannot disagree about
what a grapheme is and the two sides of a run cannot disagree about what a word
is. A book whose projection is not an analyzable `sous-core` input is refused
there rather than at publication, as `PantryError::InvalidBook`.

`Entry::mask`, `utf16`, and `published_len` therefore answer
`Err(PantryError::NoProjection)` on a reference, and `Entry::verse_lengths` and
`Entry::verse_words` answer `Err(PantryError::NoLengths)` on a target — the same
shape `text()` already had. There is no accessor that quietly returns something
empty.

## The chunk cache, and the one ceiling

Per keystroke the pipeline otherwise pays one pre-scan (~1 ms/5 MB) and one
checksum per chapter. Onion's internals model per-chapter units of work, with
cross-book concerns collected into an explicit `Carried` and whole-book products
completed by a final reduce — so an unchanged chunk's CST, tokens and local
lint report are kept and reused, and the assembled dish is byte-identical to a
cold `onion::wire::parse`.

Entries are chunk-relative and NEVER mutate; absolute offsets go out the door.
A dirty `\c` boundary (a sidebar straddling the seam) is remembered as an open
verdict, so repeat calls widen to the fused unit without re-lexing to rediscover
it. A book of two chunks or fewer is computed and thrown away: an entry that can
only ever serve front matter never pays, and would evict the large-book units
that do.

This is the one store the `Budget` bites. The LRU is hand-rolled — at
dozens-to-hundreds of entries eviction is a linear scan — and it evicts by
last-touched tick until the retained heap is back under the ceiling. A ceiling
is what makes a session-long cache safe: wasm linear memory grows and never
shrinks, so a high-water mark is permanent.

INPUT CONTRACT: checksums are only stable against LF-normalized text, the same
contract `onion::wire::parse` documents. CRLF works and is a different content
hash.

## The detached UTF-16 table

`mise::utf16::Utf16Index` stores one cumulative `u32` per 256 bytes and counts
the remainder by scanning the source. With the source gone, the remainder has
to be counted from the table itself, so `Utf16Table` adds **one bit per source
byte**: set where a UTF-16 code unit STARTS — at every character's lead byte,
and again at the byte after a 4-byte lead, which is exactly the low surrogate.
One mask therefore counts units directly, where separate character and astral
masks would need two:

```text
to_utf16(byte) = totals[byte / 256] + popcount(marks over [stride start, byte))
```

Cost is 8 bytes of mask per 64 source bytes plus 4 bytes of cumulative total per
256 — 36 bytes per 256, **14.06%**, measured identical on all eight tier corpora
(the ratio is a function of length, not script). A 64-byte stride would spend
`4 + 8` bytes per 64 = 18.75% for a shorter scan; 256 matches the index's
`STRIDE`, bounds a query at four population counts, and leaves room under the
25% ceiling for the rest of a book's products.

The table answers character-boundary offsets. The borrowing index snaps an
interior byte down to its character by reading the source; with no source there
is nothing to snap with, and every offset the Pantry rebases — mask ranges, TOC
anchors, finding spans — is boundary-legal already.

## The modules

```text
pantry/mod.rs      the facade: one place says what is stored and how much
pantry/chunks.rs   an unchanged chunk is never re-lexed
pantry/derived.rs  every derived value is reachable from a live key, or gone
pantry/budget.rs   one ceiling, three tiers, no unattributed byte
pantry/tests.rs    the registry's own claims
```

`books.rs` is not a file: the id-keyed registry, the roles, the retention and
the canonical order are the facade's own state, and splitting them from it
would put the `Entry` handle on one side of a seam and the map it borrows on
the other.
