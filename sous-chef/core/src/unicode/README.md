# `sous_core::unicode` and `mise::unicode`

One `u16` of classification bits per scalar, from pinned UCD 17.0.0. The bits
are the leaf both engines read, so they live in `mise`; what only Sous means —
the G2 pools and the atom rule — stays here. This file documents both halves,
because the bit layout and the rules over it are one argument.

```rust
mise::unicode::class_of(c) -> Class        // the bits for one scalar
mise::unicode::is_glue(c)  -> bool         // Mark plus grapheme extenders
mise::unicode::lookup::trie_at(bytes)      // the same bits off raw UTF-8
mise::unicode::lookup::{walk, walk_trie, walk_trie_swar}   // bench subjects

sous_core::unicode::pool_of(c) -> Pool     // G2's neighbour category
sous_core::unicode::atoms::widen_to_atoms(text, range) -> TextRange
sous_core::unicode::atoms::is_atom_boundary(text, at)  -> bool
```

`sous_core::unicode` re-exports `Class` and `class_of` alone, because
`substrate::is_nonletter`, `atoms`, and `pool_of` all speak in `Class`. The
bits, `is_glue`, and the index paths are `mise::unicode` and only there;
`galley::find` reads them from there too. `Class::bits`/`from_bits` and
`bits::*` are the generator's shared layout, not a caller API.

| file | crate |
| --- | --- |
| `unicode/{mod,lookup,table}.rs` | `mise` |
| `unicode/{mod,pools,atoms,tests}.rs`, `bin/gen-unicode.rs`, `testdata/ucd/` | `sous-core` |

## Bit layout

`Class` is a `u16`. The bits are exactly the charter's authorized list —
alphabetic, casing, decimal digit, whitespace, mark, punctuation/symbol,
extender, grapheme-complex — plus the three refinements the atom rule proved
it needs. There is no script lane, quote set, word-break lane, or
normalization prefilter: each returns only with a consumer.

| bit | name | source | read by |
| --- | --- | --- | --- |
| 0 | `ALPHABETIC` | DerivedCoreProperties `Alphabetic` | format placement; Level 2 |
| 1 | `UPPERCASE` | DerivedCoreProperties `Uppercase` | Level 2 casing |
| 2 | `LOWERCASE` | DerivedCoreProperties `Lowercase` | Level 2 casing |
| 3 | `WHITESPACE` | PropList `White_Space` | free-mark and NBSP checks |
| 4 | `DECIMAL_DIGIT` | General_Category `Nd` | the one pooled digit lane (charter invariant 7) |
| 5 | `MARK` | General_Category `Mn \| Mc \| Me` | free-mark check; glue |
| 6 | `PUNCTUATION` | General_Category `P*` | Level 1b |
| 7 | `SYMBOL` | General_Category `S*` | Level 1b |
| 8 | `CONTROL` | General_Category `Cc` | free-mark check |
| 9 | `FORMAT` | General_Category `Cf` | misplaced-format check |
| 10 | `NONCHARACTER` | `U+FDD0..=U+FDEF`, every `U+xxFFFE`/`U+xxFFFF` | noncharacter check |
| 11 | `EXTENDER` | GCB `Extend \| SpacingMark \| ZWJ` | glue; GB9/GB9a |
| 12 | `COMPLEX` | GCB `Prepend \| Control \| CR \| LF \| Regional_Indicator \| L \| V \| T \| LV \| LVT`, plus emoji `Extended_Pictographic` | GB6-GB8, GB11-GB13 |
| 13 | `GCB_CONTROL` | GCB `Control \| CR \| LF` | GB4/GB5 — the COMPLEX members that break on both sides |
| 14 | `PREPEND` | GCB `Prepend` | GB9b — the COMPLEX members that join forward |
| 15 | `LINKER` | DerivedCoreProperties `InCB; Linker` | GB9c — the viramas the atom rule joins through |

Noncharacters are a spec constant, not a UCD file.

## Generation

```sh
cargo run -p sous-core --bin gen-unicode      # testdata/ucd/*.txt → table.rs, pools.rs
```

The generator needs a hash map and `mise` takes no dependencies, so it stays in
`sous-core` beside the extracts and writes `table.rs` across into `mise`.

Never a `build.rs`. Both are committed, reviewable artifacts, and a second run
must leave `git diff --exit-code` clean. Inputs, their checksums,
the trim commands that produced them, and the version-bump procedure are in
[`../../testdata/ucd/README.md`](../../testdata/ucd/README.md).

## One pool, two index paths

`table.rs` holds one deduplicated pool of 64-scalar blocks. Both lookups read
it, so they cannot disagree by construction.

```text
             BLOCK_INDEX[1024]              BLOCKS[207][64]
                (u16 each)                    (u16 each)
                                          ┌────────────────┐
  cp >> 6  ───▶ [ .. 41 .. ] ─────────────▶│ block 41       │
                                          │  [0]  [1] ..   │──▶ Class
  cp & 63  ─────────────────────────────────────▶ [ .. ] ───┘
```

The byte trie reaches the same two indices without reassembling a scalar,
because UTF-8's continuation fanout is also 64:

```text
  0xxxxxxx                     →  ASCII[lead]                        width 1
  110aaaaa 10bbbbbb            →  BLOCK_INDEX[lead & 0x1F] , b       width 2
  1110aaaa 10bbbbbb 10cccccc   →  BLOCK_INDEX[(a << 6) | b] , c      width 3
  11110... (astral)            →  binary search over CLASS_RANGES    width 4
```

Above the BMP the ranges are long and hits are rare, so a search over the
committed runs beats any table.

Size: 1024 index entries plus 207 blocks, about 28.7 KiB of `.rodata`. No
heap, no `OnceLock`, nothing extra in the `.wasm`. The rejected 128 KiB flat
BMP array and the rejected decoding SWAR walk keep their code and numbers in
[`../../../experiments/`](../../../experiments/); the measurements are in
[`../../../evidence.md`](../../../evidence.md).

## The G2 pools

`Pool` is the neighbour category G2 judges on: `Quote`, `Bracket`, `Dash`,
`Terminal`, `Separator`, `Digit`, `Symbol`, `Other`. Precedence is first match
wins, so `«` is a quote rather than a bracket and `U+2212 MINUS SIGN` is a dash
rather than a symbol.

| pool | source | needs a row |
| --- | --- | --- |
| `Quote` | PropList `Quotation_Mark` | yes |
| `Bracket` | General_Category `Ps \| Pe \| Pi \| Pf` | yes |
| `Dash` | PropList `Dash` | yes |
| `Terminal` | PropList `Sentence_Terminal` | yes |
| `Separator` | PropList `Terminal_Punctuation` | yes |
| `Digit` | `DECIMAL_DIGIT` bit | no |
| `Symbol` | `SYMBOL` bit | no |
| `Other` | everything else | no |

`pools.rs` is 504 sorted `(u32, Pool)` rows — only the scalars a `Class` bit
cannot already answer. `Nd` is above every pool it could collide with and no
`Nd` carries one of the four properties, so the digit lane answers before the
search; `Symbol` is the last pool before `Other`, so it answers only where no
row claimed the scalar first. No bit is spent on any of this: `pool_of` runs at
judge time over run atoms, never in the walk.

`Pool::Digit` cannot occur as an in-run neighbour, because a digit is not a run
atom. It exists so `pool_of` is total over every scalar.

## The atom rule

An *atom* is a base scalar plus everything that cannot stand without it.
`is_atom_boundary` is the whole rule, and `widen_to_atoms` snaps an emitted
range outward to the nearest boundaries on both sides.

The claim is exactly **"no atom boundary falls inside a UAX #29 cluster"** —
never split, not minimal. The converse does not hold and is not wanted:

```text
"qx\u{0301}"            widen 2..4  → 1..4   the mark keeps its base
"a\r\nb"                widen 2..3  → 1..3   CRLF is one atom (GB3)
"\u{915}\u{94D}\u{937}" widen 6..9  → 0..9   the conjunct stays whole (GB9c)
"ab\0\0cd"              widen 2..4  → 2..4   controls break both sides (GB4/GB5)
"🛑\u{200D}🛑"          widen 4..7  → 0..11  emoji ZWJ sequence, one atom
"🇺🇸"                    widen 4..8  → 0..8   two regional indicators, one atom
```

Two adjacent emoji, two adjacent Hangul syllables, or two adjacent regional
indicators widen into one over-wide atom. That is the price of carrying no
segmentation dependency in the runtime: `unicode-segmentation` enters only as
the dev-dependency oracle.

## Conformance gates

| gate | what it proves |
| --- | --- |
| `tests::table_matches_a_fresh_ucd_parse_for_every_scalar` | the committed table equals an independent re-parse of the extracts, for all 0x110000 scalars |
| `tests::std_char_predicates_agree_over_every_scalar` | the pin matches `std`'s Unicode version |
| `tests::the_decimal_digit_lane_is_nd_not_every_numeric` | `Nd` only, both directions |
| `tests::the_two_index_paths_agree_over_every_scalar` | `class_of(c)` equals `trie_at(c)` and its width, for every scalar |
| `tests::the_bit_list_is_the_one_the_charter_authorizes` | one named scalar per bit |
| `tests::the_pool_table_matches_a_fresh_ucd_parse_for_every_scalar` | `pool_of` equals an independent re-parse of the extracts, for all 0x110000 scalars, and no `Nd` is claimed by a punctuation pool |
| `tests::pool_precedence_is_first_match_wins` | one named scalar per pool, and `Pool::from_raw` round trips |
| `tests/unicode_generator.rs` | a second generator run reproduces `table.rs` and `pools.rs` byte for byte |
| `tests/atom_conformance.rs` | no atom boundary falls inside a `GraphemeBreakTest.txt` cluster; widening any sub-range of a cluster returns the whole cluster; the test tier holds no cluster the rule would split |
| `examples/atom_fleet.rs` | the same claim over the calibration fleet — a deliberate run, not a test |
