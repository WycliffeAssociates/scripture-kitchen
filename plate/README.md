# plate

An experiment: USFM to PDF through Typst, over onion's public CST. Onion
folds a book into Typst markup (`src/lib.rs`); one Typst template
(`typst/usfm.typ`) makes every visual decision from a settings JSON. Not in the
kitchen's workspace or gate: it has its own `Cargo.lock`.

```sh
cargo build --release
./target/release/plate --out mark.pdf --runs 2 ../testData/exampleCorpora/examples.bsb/42MRKBSB.usfm
./target/release/plate-serve --corpus ../testData/exampleCorpora/examples.bsb --host 0.0.0.0
# browser build (onion + Typst in one module), served by plate-serve at /wasm:
(cd wasm && CARGO_PROFILE_RELEASE_DEBUG=0 wasm-pack build --target web --release --out-dir pkg)
```

Fonts: Typst's embedded set, anything in `plate/fonts/`, and the system's
(native only; the browser build has the embedded set).

## Numbers, 2026-11-06 (BSB, A5, two columns, 24-core Linux)

| | plate native | plate wasm (V8, 1 thread) | PTXprint 3.1.6 (Docker) |
|---|---|---|---|
| Mark | 0.9 s, 27 pp | 1.8 s | 8.0 s, 31 pp |
| Psalms | 1.7 s, 101 pp | 3.4 s | 41 s, 112 pp |
| NT | 3.0 s, 318 pp | 18 s | 89 s, 364 pp |
| Bible | 13 s, 1,408 pp, 3.4 GB | | |
| settings change, Mark | 0.6–0.9 s | 1.2 s | full rerun |

Onion's share (lex, CST, markup) is 1 ms for Mark, 63 ms for the Bible. The
wasm module is 40 MB, 16 MB gzipped (Typst's fonts and hyphenation data).
PTXprint sets Charis SIL with more leading, so its page counts run higher.

## One document per book, in parallel

`plate --split` compiles each book as its own Typst document on rayon's pool
(books start on a new page, so pagination does not cross books). NT 1.4 s
against 3.1 s as one document; Bible 4.6 s against 13.2 s, 2.6 GB against
3.4 GB. The slowest book bounds it. Not done: merging the PDFs, carrying
page numbers across books, and a contents page or links across books, which
need a first pass for page counts.

## Recipes

`recipes/`: links and contents, a study edition with translation notes, a
diglot, a printable diff. Examples, not product; see `recipes/README.md`.

## What's missing against PTXprint

Paragraph-style footnotes across the page width, cross-reference apparatus,
true drop caps (`chapter: "drop"` is a stub), diglots, adjust lists, pictures,
Graphite.
