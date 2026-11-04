# canon

What a book is, before any text of it exists: its code, its two numbers, its
English names, and (next) how each numbering tradition counts its verses.

```rust
canon::book(MAT)                  // Book { usfm_number: "41", canonical_index: Some(40), .. }
canon::parse_book("1samuel")      // Some(1SA): a code in any case, an English name or alias
canon::books(Canon::Protestant66) // GEN .. REV, in canonical order
canon::usfm_file_name(MAT)        // "41-MAT.usfm"
```

```ts
import { book, books, parseBook, usfmFileName } from "@wycliffeassociates/scripture-kitchen/canon";
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
| `src/tables.rs` | the Rust tables | `cargo run -p canon --bin gen-canon` |
| `ts/tables.ts` | the same tables for TypeScript | the same run |
| `src/books.rs` | the Rust lookups | by hand |
| `ts/canon.ts` | the TypeScript lookups | by hand |
| `fixtures/books.json` | the cases both lookups must pass | by hand |

`tests/generator.rs` fails when a table is stale. `tests/fixtures.rs` (Rust)
and `ts/canon.test.mjs` (`npm test` here) run the same fixtures. A behaviour
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

The versification data still to come (the Copenhagen Alliance schemes, CC BY-SA
4.0, and the TVTMS-derived sniffing rules, CC BY) will carry its attribution
in `NOTICE` beside it.
