# Unicode Character Database extracts (UCD 17.0.0)

The pinned inputs behind `src/unicode/table.rs` and the grapheme-atom
conformance gate. They are committed reference data, read only by
`bin/gen-unicode.rs` and by tests — `sous-core` opens no file at runtime.

Each extract keeps the file's pristine eight-line header (title, date,
copyright, licence pointer) so its provenance travels with it, followed by
only the lines the generator reads. `GraphemeBreakTest.txt` and
`GraphemeBreakProperty.txt` are pristine.

## Files

| file | source under `https://www.unicode.org/Public/17.0.0/ucd/` | lines kept | feeds |
| --- | --- | --- | --- |
| `DerivedGeneralCategory.txt` | `extracted/DerivedGeneralCategory.txt` | GC ∈ M\*, P\*, S\*, Nd, Cc, Cf | mark, punctuation, symbol, decimal digit, control, format |
| `DerivedCoreProperties.txt` | `DerivedCoreProperties.txt` | Alphabetic, Uppercase, Lowercase, `InCB; Linker` | alphabetic, casing, linker |
| `PropList.txt` | `PropList.txt` | White_Space | whitespace |
| `GraphemeBreakProperty.txt` | `auxiliary/GraphemeBreakProperty.txt` | pristine | extender, complex, gcb-control, prepend |
| `emoji-data.txt` | `emoji/emoji-data.txt` | Extended_Pictographic | complex |
| `GraphemeBreakTest.txt` | `auxiliary/GraphemeBreakTest.txt` | pristine | `tests/atom_conformance.rs` only |

Noncharacters (`U+FDD0..=U+FDEF`, every `U+xxFFFE`/`U+xxFFFF`) are a spec
constant, not a file.

## Checksums

xxh3-64 of each committed file, so a silent edit is visible:

| file | xxh3-64 | bytes |
| --- | --- | --- |
| `DerivedCoreProperties.txt` | `b7474ad887ba584a` | 206910 |
| `DerivedGeneralCategory.txt` | `e36a96d517790f5b` | 92115 |
| `GraphemeBreakProperty.txt` | `989f35989eaea2d1` | 99377 |
| `GraphemeBreakTest.txt` | `38c0549730f9c575` | 126570 |
| `PropList.txt` | `e9d53165a25c179b` | 996 |
| `emoji-data.txt` | `dba1f22ec5cbd739` | 39444 |

## Reproducing the extracts

From a directory holding the six pristine downloads:

```sh
{ sed -n '1,8p' DerivedGeneralCategory.txt
  grep -E '; (Mn|Mc|Me|Pc|Pd|Ps|Pe|Pi|Pf|Po|Sm|Sc|Sk|So|Nd|Cc|Cf) ' DerivedGeneralCategory.txt
} > out/DerivedGeneralCategory.txt

{ sed -n '1,8p' DerivedCoreProperties.txt
  grep -E '; (Alphabetic|Uppercase|Lowercase)( |#)' DerivedCoreProperties.txt
  grep -E '; InCB; Linker' DerivedCoreProperties.txt
} > out/DerivedCoreProperties.txt

{ sed -n '1,8p' PropList.txt; grep -E '; White_Space ' PropList.txt; } > out/PropList.txt
{ sed -n '1,8p' emoji-data.txt; grep -E '; Extended_Pictographic' emoji-data.txt; } > out/emoji-data.txt
cp GraphemeBreakProperty.txt GraphemeBreakTest.txt out/
```

## Version discipline

The pin must match two other things or the gates disagree with each other:

1. **`std`'s Unicode version.** `unicode::tests::std_char_predicates_agree_over_every_scalar`
   compares `is_alphabetic`/`is_uppercase`/`is_lowercase`/`is_whitespace`
   against `char`'s own tables over every scalar. If a toolchain bump moves
   `std` off 17.0.0, bump this pin — do not relax the test.
2. **`unicode-segmentation`'s Unicode version.** It is the dev-dependency
   oracle for `tests/atom_conformance.rs`; a skew would compare the atom rule
   against a different algorithm than `GraphemeBreakTest.txt` states.

## Refreshing to a new Unicode version

1. Re-download the six files from
   `https://www.unicode.org/Public/<VERSION>/ucd/` and re-run the trim
   commands above; update the checksum table.
2. Bump `unicode-segmentation` to the release targeting that version.
3. `cargo run -p sous-core --bin gen-unicode`.
4. `cargo test -p sous-core` — drift, std cross-check, index-path agreement,
   generator determinism, and both conformance gates must stay green. Run
   `cargo run -p sous-core --release --example atom_fleet` as the calibration
   check and record its count in `sous-chef/evidence.md`.

## Licence

These data files are distributed under the Unicode Licence; each file's own
header carries the terms. They are included as reference data for generation
and testing.
