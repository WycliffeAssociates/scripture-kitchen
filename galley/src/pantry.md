# The Pantry

The Galley-wide, id-keyed registry of detached per-book products, beneath lint,
CST, and Sous alike. `galley/docs/analysis-host.md` states the ownership
contract this implements; `sous-chef/charter.md` ("Hosts own lifecycle and
presentation") states the same rule from Sous's side.

## The text rule

**A method that takes `&str` needs the current bytes and the caller supplies
them; a method that does not promises to work from detached products alone.**

`update(id, role, text)` and `changed_since_update(id, text)` take text.
`books`, `checksum_for`, `mask_for`, `toc_for`, `utf16_for`,
`published_len_for` and `resident_bytes` do not, and never will: they are what
a publication runs on when no book was touched this call.

The Pantry retains no string and no borrow into one — the `Book` struct is
asserted `'static` at compile time, which is the mechanical form of that
promise. There is no splice API and never will be: the only mutation is
whole-book replacement under a caller-chosen opaque id, which is idempotent and
cannot shift coordinates inside a book. A book the caller forgets to resend is
stale as a whole and heals on its next update.

## Two questions, two answers

An editor asks two different things about an edited book, and the sketch's
recipe answers them from one `Fingerprint` — chunk starts plus one xxh3-128 per
chunk, about 1 KB per book and no text:

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
per-chunk hash `galley::chunks` prints as hex, and the key under which detached
projection and UTF-16 data may be reused. A Sous `ObservationKey` (Slice B2) is
a different newtype over the same algorithm, keyed on a chapter's *projected*
inputs plus a pass schema stamp. Keeping them distinct is what lets a
markup-only edit move the raw checksum, invalidating the projection and the
UTF-16 table, while the observation key stands still and the observation is
reused with rebased coordinates.

## Roles and retention

A book is registered as `Target` — full detached products, publishes findings —
or, from Stage 5, `Reference`: TOC and per-verse observations only, no mask and
no UTF-16 table, because a reference corpus never publishes a coordinate. B1
ships `Target` alone; the enum carries the second variant's shape as a doc line
rather than as dead code.

Retained per `Target` book, roughly 25% of the raw text:

| Product | Source | Why it survives the string |
| --- | --- | --- |
| chunk products | the owned `Warmer` | content-addressed; an unchanged chapter is never re-lexed |
| `Toc` | `onion::toc` | chapter/verse anchors in raw bytes |
| `Mask` | `onion::mask`, verse-text filter | owned ranges + starts; already detached |
| `Utf16Table` | `galley::utf16` | byte → UTF-16 with no source present |
| published length | the table | a publication's `published_len` |
| `Fingerprint` | `galley::pantry::fingerprint` | the baseline for the next update |

Order is canonical by `BookKey` — Onion's `tables::books::BOOK_CODES` order,
which is the USFM spec's — ties broken by id, so `BookIndex` never depends on
the order updates arrived in. Two ids carrying the same `\id` are both present;
Galley never parses an id.

## The detached UTF-16 table

Onion's `Utf16Index` stores one cumulative `u32` per 256 bytes and counts the
remainder by scanning the source. With the source gone, the remainder has to be
counted from the table itself, so `Utf16Table` adds **one bit per source byte**:
set where a UTF-16 code unit STARTS — at every character's lead byte, and again
at the byte after a 4-byte lead, which is exactly the low surrogate. One mask
therefore counts units directly, where separate character and astral masks would
need two:

```text
to_utf16(byte) = totals[byte / 256] + popcount(marks over [stride start, byte))
```

Cost is 8 bytes of mask per 64 source bytes plus 4 bytes of cumulative total per
256 — 36 bytes per 256, **14.06%**, measured identical on all eight tier corpora
(the ratio is a function of length, not script). A 64-byte stride would spend
`4 + 8` bytes per 64 = 18.75% for a shorter scan; 256 matches Onion's `STRIDE`,
bounds a query at four population counts, and leaves room under the 25% ceiling
for the rest of a book's products.

The table answers character-boundary offsets. Onion's index snaps an interior
byte down to its character by reading the source; with no source there is
nothing to snap with, and every offset the Pantry rebases — mask ranges, TOC
anchors, finding spans — is boundary-legal already.
