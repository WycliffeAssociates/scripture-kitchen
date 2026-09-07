# `sous_core::codec` and the corpus envelope

The hot transport for findings: a versioned fixed-width snapshot designed from
day one, not a serialized Rust struct and not an array of JS objects. The rich
in-memory finding model may carry typed evidence; these lanes are a compact
representation of it, never the analysis truth.

## The 16-byte record

Little-endian, headerless. The surrounding container declares the coordinate
space.

| bytes | field | contract |
| --- | --- | --- |
| 0..4 | `from: u32` | start in the book coordinate space the container declares |
| 4..8 | `to: u32` | end in that space, exclusive |
| 8..10 | `book_idx: u16` | index into the snapshot's ordered book table |
| 10 | `code: u8` | union tag into the v1 code table |
| 11 | `flags: u8` | representation flags; only `SATURATED` is valid |
| 12..14 | `i16` lane | meaning owned by the rule code |
| 14..16 | `i16` lane | meaning owned by the rule code |

`book_idx` is the caller's snapshot-local array index, not a canonical book
id: a USFM file and a vref corpus may both supply the book, and the caller
owns the addressing space. `u16` is ample without spending four bytes per
finding.

## The v1 code table

Dense, hand-assigned, append-only. No reserved ranges, no retired entries.
Removing or renumbering a code requires a new wire version rather than leaving
a tombstone.

| code | kind | lane 12..14 | lane 14..16 | `SATURATED` means |
| --- | --- | --- | --- | --- |
| 0 | `LengthProportionality` | signed Q8.8 book-scope deviation; `i16::MIN` unavailable | signed Q8.8 project-scope deviation; `i16::MIN` unavailable | a deviation was clamped |

| 1 | `Hygiene` | `HygieneClass` discriminant | run length in code points, `1..=i16::MAX` | the run exceeds `i16::MAX`; the lane reads exactly `i16::MAX` |
| 2 | `Convention` | pattern-table index, `u16` bits in the signed lane | `Reasons` bitmask over the ladder rungs the site matched, the word rungs included | never set |
| 3 | `SourceCopy` | consecutive target words the paired source verse also holds, `1..=i16::MAX` | eligible target words in the unit, `1..=i16::MAX` | one of the two lanes clamped; which one is not said |
| 4 | `Presence` | `PresenceKind` discriminant | consecutive verse keys the row covers, `1..=i16::MAX` | the run exceeds `i16::MAX`; the lane reads exactly `i16::MAX` |

The record is a discriminated union in Rust: `PackedFinding` carries a
`FindingKind`, and `code`, `flags`, and both lanes are *derived* from it. A
caller cannot set a code that disagrees with its payload.

Each code's lane codec lives in its own `codec/<rule>.rs` with a matching
`lanes()` / `from_lanes()` pair, so `mod.rs` never becomes a dumping ground for
payload shapes.

Every assigned code is emitted. `Presence` took 4 out of order so that
`SourceCopy`, already specified against 3 before P1 landed, could keep it; U1
took 3 and the table is dense again. `SourceCopy` is the one code whose lane
ships OFF by default (`LengthConfig::source_copy`), so a default publication
carries none — `galley/tests/goldens/sous/knobs.bin` is where the wall and the
JS reader see one. Code 0 is
`sous_core::proportionality`, which
publishes BOTH lanes on every row: a book scope and a project scope, and
`i16::MIN` where a scope did not judge. That sentinel is also, by construction,
the under-`min_verses` flag — a book with too few paired verses is judged by
the project alone — so no flag bit says the same thing twice
([`../proportionality.md`](../proportionality.md)).

## Fail-closed rules

Decoding refuses rather than guesses:

| rejected | error |
| --- | --- |
| a buffer that is not exactly 16 bytes | `InvalidLength` |
| a code outside the table | `UnknownRuleCode` |
| any flag bit outside `SATURATED` | `UnknownFlags` |
| `from > to` | `ReversedSpan` |
| `to` past the selected book's published length | `SpanOutOfBounds` |
| a `book_idx` with no directory entry | `InvalidBookIndex` |
| a record whose `book_idx` is not its section's book | `BookIndexMismatch` |
| `i16::MIN` read as a proportionality value | `MissingDeviationSentinel` |
| a hygiene class discriminant past the table | `UnknownHygieneClass` |
| a hygiene run of zero or negative | `EmptyHygieneRun` |
| `SATURATED` on a hygiene row whose lane is not exactly `i16::MAX` | `UnknownFlags` |
| a presence kind discriminant past the table | `UnknownPresenceKind` |
| a presence key count of zero or negative | `EmptyPresenceRun` |
| `SATURATED` on a presence row whose lane is not exactly `i16::MAX` | `UnknownFlags` |
| a source-copy run of zero, negative, or longer than its eligible count | `InvalidSourceCopyRun` |
| `SATURATED` on a source-copy row with NEITHER lane at exactly `i16::MAX` | `UnknownFlags` |
| a convention reasons lane of zero | `EmptyReasons` |
| a convention reasons bit outside the table | `UnknownReasons` |
| `SATURATED` on a convention row | `UnknownFlags` |
| a convention `pattern_idx` at or past `pattern_count`, or a direct `pattern(index)` call past the table | `PatternIndexPastTable` |
| a pattern row's reserved byte or `flags` set | `InvalidPattern` |
| a pattern channel, key, band, or share outside its table | `InvalidPattern` |
| a `Casing` key byte of `Uncased`, or a `Casing` row carrying a glyph | `InvalidPattern` |
| a `Doubled` key byte above 1 | `InvalidPattern` |
| a `LetterRun` key byte outside `2..=8` | `InvalidPattern` |
| a pattern `books` of zero on a row with a numerator, or past the header's `book_count` | `InvalidPattern` |
| a `pattern_offset` that is not the running cursor | `PatternSectionOutOfOrder` |
| more than 65,535 patterns | `PatternCountOverflow` |

`i16::MIN` cannot be constructed as a `QuantizedDeviation`, and a zero run
cannot be constructed as a `HygieneDigest`, a `PresenceDigest`, or a
`SourceCopyDigest`, so the
invalid states are unrepresentable on the way in as well as rejected on the way
out.

## The corpus envelope

One complete corpus publication replaces the previous one. It promises no
independently reusable per-book buffers and no finding patches: a change in one
chapter may move a project denominator and thereby add or remove findings in an
untouched book.

```text
  header  48 bytes   SOUS magic · format version · coordinate flags ·
                     book count · record stride (16) · total findings ·
                     pattern count · absolute pattern offset ·
                     opaque 16-byte SnapshotId
  directory          one 20-byte row per book, in caller order:
                     3 BookKey bytes + zero terminator · published length ·
                     absolute section offset · finding count ·
                     absolute id offset
  id strings         one per book, directory order, each a u16 little-endian
                     byte length followed by that many UTF-8 bytes; the
                     section is zero-padded to a 4-byte boundary
  pattern table      contiguous 24-byte rows in emission order, corpus-level
                     and not per book; a row is 4-byte aligned, so the
                     sections behind it stay aligned however many fired
  sections           contiguous 16-byte records, no incidental padding
```

### The pattern table

The judge's output, one row per firing pattern. It is the corpus's evidence,
and a `Convention` record carries only a position in it plus the reasons that
position matched — so ten thousand sites of one convention cost ten thousand
16-byte records and *one* 24-byte row of argument.

| bytes | field |
| --- | --- |
| 0..4 | `glyph: u32` (`ScalarKey` raw; `u32::MAX` is the pooled digit key) |
| 4..8 | `neighbor: u32` (the G3 key; 0 on every other channel) |
| 8 | `channel: u8` (`Channel` discriminant, finest grain first) |
| 9 | `key: u8` (Placement: `side << 4 \| OuterClass`; RunShape: `pure << 4 \| bucket`; PooledNeighbor: `Pool`; Casing: `Form`; WordLength: whole deviations above the mean; Doubled: 0 adjacent, 1 separated; LetterRun: run length `2..=8`; SentenceStart: 0; else 0) |
| 10 | `band: u8` (staircase step index; `0xFF` = none, which only `Rarity` carries) |
| 11 | `flags: u8` (reserved, 0; the decoder refuses nonzero) |
| 12..16 | `numerator: u32` |
| 16..20 | `denominator: u32` |
| 20..22 | `share_bp: u16`, at most 10,000 |
| 22 | `books: u8` — books holding part of the numerator; books-possible is the header's `book_count` |
| 23 | reserved `u8` 0 (the decoder refuses nonzero) |

**Channel 9 `SentenceStart` is an ordinary glyph row with an empty key:** the
glyph field carries the run terminal as a `ScalarKey`, the neighbor field is
zero, and the key byte is zero, because the claim needs nothing beside the
glyph. Unlike `Rarity`, which is the other zero-key channel, it carries a band.
It judges the substrate's `follows` lane rather than its pairs or runs, so
`Channel::is_word` and `judged_by_words` are both false and `Substrate` owns
its sites.

**Channel 8 `LetterRun` comes out of the same word walk and is NOT one of
them:** it judges a real scalar, so bytes 0..4 carry the folded letter as an
ordinary `ScalarKey`, the neighbor field is zero like every other non-`G3`
channel, and the key byte is the run length `2..=8`, the last saturating.
`Channel::is_word` is false for it and `Channel::judged_by_words` is true;
the reader's own `word` predicate names the three, not the four.

**The three word channels, 5 `Casing`, 6 `WordLength`, and 7 `Doubled`, read
bytes 0..8 as one thing:** the u64 word hash, little-endian, low half where a
glyph would be and high half where a neighbor would be. They judge no scalar,
so `ScalarKey::from_raw` is not applied to the glyph field there and a decoder
returns `ScalarKey::NONE` for it. The key byte is a `Form` discriminant `0..4`
on `Casing`, with `Uncased` refused; a saturating sigma on `WordLength`, where
every value is legal; and `0` (adjacent) or `1` (separated) on `Doubled`, with
everything above refused. Every other channel still refuses a nonzero neighbor.
`Pattern::word_hash` reads the pair back on all three, and the generated reader
decodes it as a `bigint`. Why a hash and not the bytes: `../words.md`.

`Reasons` **is the full `i16` lane B, not its low byte.** Bit 7,
`WORD_LENGTH`, was the last one a `u8` could hold; W2 spent bits 8 and 9 on
`DOUBLED_BARE` and `DOUBLED_SEPARATED`, W4 bit 10 on `LETTER_RUN`, and W5 bit
11 on `SENTENCE_START`, which cost nothing on the wire because the lane was
always sixteen bits and both readers always read it as one — Rust holds
`Reasons` in a `u16` and the generated reader calls `getUint16`. Four bits are
left, and `KNOWN_BITS` is the whole of what is legal: bit 12 is refused, which
`convention_refuses_unknown_reason_bits` pins.

`books` is dispersion, and dispersion is information: nothing in the engine
gates on it. A row with a numerator names at least one book, and no row may
name more books than the publication has.

`pattern_count` is capped at `u16::MAX`, because a `PatternIndex` is a `u16`.
What the channels mean, and the order the rows arrive in: `../judge.md`. Who
emits the code-2 rows that name these, and why one maximal run is one of them:
`../sites.md`.

Directory position *is* `BookIndex`, so a consumer seeks by index, by key, or
by the host's id and lazily decodes one book:

```ts
const snapshot = FindingsSnapshot.open(buffer);
const mark = snapshot.book("MRK");
mark.length;
mark.at(0);
snapshot.findingsFor("books/mrk.usfm");
```

### The id string table

The host's opaque `BookId` — a file path in practice — travels in the wire so a
consumer can address a book the way it registered it. It is also the only
unique identity a publication has: **two books may carry the same `BookKey`**,
because two files may carry the same `\id`, and both are published as
independent rows. A repeated *id* is refused on the way in and on the way out.

Each id offset is absolute into the buffer, like the section offset beside it,
and must be exactly the running cursor, so the strings cannot overlap,
reorder, or hide bytes. The length prefix means an id needs no forbidden byte
and no scan. The 4-byte padding after the section keeps every record section
aligned for a typed-array view.

This is a **v1 layout edit**, made while nothing is released: the directory row
grew from 16 bytes to 20, then the header grew from 40 bytes to 48 for the
pattern table, and the hex goldens were re-pinned each time. It is the last
one that gets to be free. The charter's rule — a layout change means a new wire
version — applies from the first release, and from then on this table's shape
is frozen inside version 1.

Sous analysis emits projected-book UTF-8 ranges; `galley::sous` composes the
producer locator with UTF-8-to-UTF-16 conversion and publishes raw-book UTF-16
for JS. A projected range that maps to discontinuous raw spans publishes the
declared **bounding** range — first retained raw byte through last — as its
navigation span. That is the documented contract, not a silent contiguity
pretense; the exact retained run set stays reachable through the typed-detail
path.

## The generated TypeScript reader

Rust owns the schema. `reader.ts` is generated and committed:

```sh
cargo run -p sous-core --bin codegen     # reader.ts.tmpl → ../reader.ts
```

`corpus::generated_reader_ts()` substitutes `@@NAME@@` placeholders in
`sous-chef/reader.ts.tmpl` with the Rust constants — every offset, the magic,
the format version, the UTF-16 flag, and the `HygieneClass`, `PresenceKind`,
`Channel`, `OuterClass`, `Pool`, and `Reasons` name lists. The
freshness test `corpus::tests::checked_in_reader_is_fresh` compares the
committed file against a fresh render, so a constant that moves without a
regenerate fails the suite. `sous-chef/reader.test.mjs` (`npm test`) decodes
the same golden buffers the Rust tests use, so both readers are pinned to one
set of bytes.

## Adding a code

1. Append the variant to `RuleCode` with the next free `u8`. Never renumber.
2. Add its payload type in a new `codec/<rule>.rs` with `lanes()` /
   `from_lanes()`, making the invalid lane values unconstructible.
3. Add the variant to `FindingKind` and to `code()`, `flags()`, `encode()`,
   and `decode_wire()`.
4. Add a golden-vector test and a fail-closed test beside the payload.
5. If the payload adds a name list the reader needs, add a `@@PLACEHOLDER@@`
   to `reader.ts.tmpl` and its substitution in `generated_reader_ts()`, then
   re-run the codegen bin.
6. Add the row to the code table above and to `charter.md`.
