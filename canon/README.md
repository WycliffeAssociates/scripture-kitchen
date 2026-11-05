# canon

What a book is, before any text of it exists: its code, its two numbers, its
English names, and how each numbering tradition counts its chapters and verses.

```rust
canon::book(MAT)                  // Book { usfm_number: "41", canonical_index: Some(40), .. }
canon::parse_book("1samuel")      // Some(1SA): a code in any case, an English name or alias
canon::books(Canon::Protestant66) // GEN .. REV, in canonical order
canon::usfm_file_name(MAT)        // "41-MAT.usfm"

canon::scheme("eng")?.chapters(MAL)       // Some(4); org has 3
canon::scheme("org")?.last_verse(PSA, 3)  // Some(9); org numbers the title
canon::scheme("ulb")?.last_verse(ACT, 19) // Some(40); eng has 41
canon::scheme("ulb")?.chapters(TOB)       // None: ulb is the 66
```

```ts
import { book, books, parseBook, usfmFileName } from "@wycliffeassociates/scripture-kitchen/canon";
import { chapters, lastVerse, verseCounts } from "@wycliffeassociates/scripture-kitchen/canon";

lastVerse("ulb", "ACT", 19); // 40
```

## Two numbers, never one

`canonical_index` is a book's place among the 66 (Matthew is 40).
`usfm_number` is the spec's file number (Matthew is 41; 40 is unused between
the testaments). The org's ~35 hand-kept book tables disagree on which one "the
number" is. Here both are named, and neither is "the" number.

## Where things are

| file | what | who writes it |
|---|---|---|
| `data/books.tsv` | one row per USFM book identifier, in spec order | by hand |
| `data/schemes/*.json` | one file per versification scheme (see its README) | vendored, and `ulb` measured |
| `src/tables.rs`, `src/scheme_tables.rs` | the Rust tables | `cargo run -p canon-gen` |
| `ts/tables.ts`, `ts/scheme-tables.ts` | the same tables for TypeScript | the same run |
| `src/books.rs`, `src/scheme.rs` | the Rust lookups | by hand |
| `ts/canon.ts`, `ts/versification.ts` | the TypeScript lookups | by hand |
| `fixtures/books.json`, `fixtures/schemes.json` | the cases both lookups must pass | by hand |

The generator is `gen/`, a separate unpublished crate (`canon-gen`): it reads
the scheme JSON with serde_json, and canon's only dependency is `mise`. Its
test fails when a table is stale. `tests/fixtures.rs` (Rust) and
`ts/canon.test.mjs` (`npm test` here) run the same fixtures. A behaviour
change starts as a fixture change.

## Why TypeScript is plain TypeScript

The book table is data, and its lookups are a few lines. Shipping them as wasm
would make every book picker wait on `init()` and hold a handle. So the data
is generated as TypeScript from the same rows as the Rust, the lookups are
written once per language, and the shared fixtures keep them in agreement. The
logic that versification brings next (mapping, coverage, sniffing) is a
different case: it has one implementation, reached through galley's wasm.

No serde and no runtime parsing. canon depends on `mise` and nothing else, so
onion can use it without gaining a dependency.

## Names and aliases

`parse_book` matches exactly, never by prefix: "phil" choosing Philippians is a
navigation box's policy, not the table's. A code wins over a name, so "Jon" is
Jonah. Every name and alias must fold to something no other book uses, and
`gen-canon` refuses a table where two do. Abbreviations and names in other
languages arrive with the localized names (plan step 6).

## Sources

The rows follow the USFM 3.1 Book Identifiers table
(`ubsicap/usfm` `docs/identification/books.rst`), in its row order, which is
also `mise::books::BOOK_CODES`'s order (a test holds them equal). English names
for the 66 follow Sefer's `packages/text/src/location/canon.ts`.

The versification schemes other than `ulb` are the Copenhagen Alliance files
(CC BY-SA 4.0); `NOTICE` carries their attribution, and
`data/schemes/README.md` says where each came from and how `ulb` was measured.
The sniffing rules still to come (TVTMS-derived, CC BY) will join them there.

## Schemes

A scheme answers "which verses should this book have": `chapters`,
`last_verse`, `verse_counts` and `excluded`. What a text actually has is its
own TOC, in onion; comparing the two is coverage (plan step 4).

- `org` is the pivot every mapping goes through: BHS for the OT, UBS/NA for the NT.
- `eng`, `lxx`, `vul`, `rsc`, `rso`: English, Septuagint, Vulgate, and the
  Protestant and Orthodox Russian Synodal numberings.
- `ulb` is `eng` except in two chapters, where en_ulb numbers like `org`:
  Acts 19 ends at 40 and 2 Corinthians 13 at 13, each with a footnote
  about the other numbering.
- `lxx` has no NEH, DAN or EST, and `vul` no EST: Nehemiah is the back half
  of the Septuagint's EZR (2 Esdras), and Daniel and Esther are the Greek
  DAG and ESG.
- No built-in scheme excludes a verse. `MAT 17:21` and its kind are numbered
  everywhere and left to the text to footnote.

Scheme names are exact (`"org"`, not `"ORG"`). Mapping between schemes uses
the same files' `mappedVerses` and comes next (plan step 3).
