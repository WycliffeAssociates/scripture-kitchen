# Recipes

Examples of what the plate pipeline can print, kept as inspiration — none of
them is product. Each is a small Rust program (`main.rs`) that prepares the
markup, usually through one of plate's hooks, and a Typst overlay
(`<name>.typ`) that starts `#import "/usfm.typ": *` and redefines only what it
changes. The overlay is read on every run, so it can be edited without a
rebuild. PDFs land in `plate/debug/recipes/`.

```sh
cargo build --release
B=../testData/exampleCorpora
./target/release/recipe-links  $B/examples.bsb/{19PSA,23ISA,41MAT,42MRK}BSB.usfm
./target/release/recipe-notes  ../testData/stressCorpora/en_ult/42-MRK.usfm data/tn_MRK.tsv
./target/release/recipe-diglot $B/examples.bsb/42MRKBSB.usfm $B/en_ulb/42-MRK.usfm "Berean Standard Bible" "Unlocked Literal Bible"
./target/release/recipe-diff   ../testData/stressCorpora/en_ult/42-MRK.usfm $B/en_ulb/42-MRK.usfm side ULT ULB    # or redline
```

The studio (`plate-serve`) runs them too: pick one from **Recipe** in the
bar. Server compile only. Diglot pairs the selected book with the ULB; diff
compares the ULT and the ULB of the selected book; notes need
`data/tn_<CODE>.tsv`.

`data/tn_MRK.tsv` is unfoldingWord's English translation notes for Mark (CC
BY-SA 4.0), fetched with
`curl -L -o data/tn_MRK.tsv https://git.door43.org/unfoldingWord/en_tn/raw/branch/master/tn_MRK.tsv`.

| recipe | what it shows | Rust | Typst | measured (24-core Linux) |
|---|---|---|---|---|
| `links` | contents page, PDF bookmarks for books and sections, every `\xt` a link to its verse when the verse is in the PDF (book names parsed by canon) | 37 | 32 | Psalms + prophets + NT: 733 pages, 885 references resolved, 5,918 link annotations, 7.8 s |
| `notes` | a study edition: translation notes after the paragraph holding their verse, book and chapter introductions in place, Markdown notes turned into Typst | 110 | 34 | ULT Mark + 3,116 notes: 308 pages, 2.7 s |
| `study` | a classic study Bible page: text above, the notes for that page's verses below a rule in small type; each note a caller-less footnote anchored at its verse, so Typst keeps it with its verse and carries overflow on (`sample` = first note per verse, `all` = every note) | 50 | 31 | ULT Mark: 54 pages at sample density, 2.7 s; 143 pages with all 3,116 notes, 5 s |
| `diglot` | two texts side by side, one row per paragraph of the left text | 74 | 17 | BSB + ULB Mark: 366 rows, 48 pages, 0.5 s |
| `diff` | verse-by-verse word diff, unchanged runs collapsed; `side` or `redline` | 93 | 37 | ULT → ULB Mark: 676 verses changed, 63 pages (side) / 38 (redline), under 1 s |

Line counts are non-blank lines.

## Hooks they use

`plate::Hooks` (in `src/lib.rs`): `xt` resolves a cross reference's text to
an anchor; `book_start`, `chapter_start` and `after_paragraph` return markup
the emitter places there. Every verse call carries its book
(`#u.v("MRK", "1", "1")`), which is what `links` turns into an anchor.

## Where each one stops

- `links`: a reference to a verse outside the PDF stays plain text; chapters
  are not bookmarked.
- `notes`: notes follow the paragraph, not the bottom of the page; a classic
  study-Bible note zone needs a page builder Typst does not have.
- `study`: notes sit at the foot of each column by default. `{"study_layout":
  "band"}` puts them in one full-width band (the text in a `columns()` block
  on a one-column page), but the band is a single column of notes, notes fall
  behind their verses when they crowd, and layout is about 11x slower (Mark
  29 s against 2.6 s). A band set in two or three columns, or notes run in as
  one paragraph, needs a page builder Typst does not have. Past about one
  note per verse the text thins to a few lines a page.
- `diglot`: verse text only (notes and headings drop out); verses matched by
  number, not mapped between versifications through canon.
- `diff`: verse text only; a moved verse shows as a delete and an insert.
