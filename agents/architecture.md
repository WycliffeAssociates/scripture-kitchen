# Architecture

    mise ◄── onion ◄── onion-wasm            mise ◄── sous-core
       ▲        ▲                                        ▲
       ╰────────┴──────────── galley ◄──────────────────╯
    ticket ◄── onion, galley
    mise ◄── canon ◄── canon-gen (never published)
             canon ◄── galley (galley::versification: onion's facts, canon's sniff)
    mise, canon ◄── bento ◄── bttwriter (mise, canon too)
    mise, canon ◄── reference ◄── srs ◄── galley (srs locate, the reference doors)

- `onion` / `onion-wasm` / `galley` are the engine. `sous-core` / `sous-cli`
  (under `sous-chef/`) are the other product. `GLOSSARY.md` is onion's only;
  `planning/` holds every crate's plans, by tag; everything about tests and
  conventions applies to both.
- `mise/` is the zero-dependency leaf both share: standard-derived tables and
  borrow-free data structures (`BookKey`, book order, UTF-16 offset types,
  the per-scalar Unicode `Class`), plus `words/1`, the word boundary sous,
  galley's whole-word find and alignment all split by. A generated table may live there while its
  generator does not — `bin/gen-unicode` stays in `sous-core` with the pinned
  UCD extracts and writes across. Scope rule and exclusions: `mise/README.md`.
- `canon/` is book identity and numbering: the USFM book table (codes, both
  of Matthew's numbers, English names, `parse_book`, `usfm_file_name`), book
  names in 32 gateway languages (`name`, `parse_book_in`), versification
  schemes' chapter and verse counts, mapping through `org` (`map`, `pairs`,
  `counterparts`) and sniffing. It depends on `mise` only and has no runtime
  parsing: `canon-gen` (`canon/gen/`, its own crate because it reads JSON with
  serde_json and onion's lexer for the name harvest) writes the Rust tables
  AND their `canon/ts/` twins from `canon/data/`, and the package ships the
  TypeScript lookups at `./canon` (names at `./names/<locale>`) with no
  wasm. Mapping and sniffing are Rust only, reached from JS through galley.
  Rust and TS both run `canon/fixtures/*.json`. A text against its scheme is
  onion's `lint_against_counts`, given the counts: onion never depends on
  canon. Pairing across two schemes is sous-core's `pair_keys_across`, given a
  `Crosswalk` galley builds from canon: sous-core never depends on canon
  either. Plan (done):
  `planning/items/2026-11-04-canon-and-versification-the-book-table-schemes-map.mdx`.
- `bento/` is scripture containers: the manifest at a repo's root (RC
  `manifest.yaml`, Burrito `metadata.json`, BTT Writer `manifest.json`), read
  into one `Manifest` with each format's typed document beside it, checked
  (findings with a JSON pointer, never a silent fix), created (`create_bible`)
  and edited (`edit`: add books, refresh checksums, splicing YAML text so only
  the new lines change). Reading never changes anything and never fails. Book
  entries, names and `sort` come from canon. Burrito validity is WA's fork,
  vendored in `bento/schema/burrito` and tested. It depends on `canon` and
  `mise`, never on onion or galley: a manifest's books are files, not text it
  parses. Plan (done):
  `planning/items/2026-11-05-container-rc-burrito-and-writer-manifests-read-and.mdx`.
- `bttwriter/` is BTT Writer migration: `stitch` (a Writer book's chunks to
  USFM, byte for byte as Sefer's template), a sans-IO history `walk` (it asks
  for objects and is handed them, so async isomorphic-git can drive it; the
  `git2` feature adds a libgit2 adapter), `replay` (every book's saves as one
  line of commits, with `clock` estimating impossible device dates), the
  in-memory `pack` writer, and `consolidate` (replay plus a closing commit
  holding bento's manifests). No I/O and no git library on the default
  features. It depends on `bento`, `canon` and `mise`. Plan (done):
  `planning/items/2026-11-05-writer-consolidation-saves-in-one-git-pack-out.mdx`.
- `reference/` is a place in scripture spelled for machines: the Scripture
  Referencing Standard's basic reference (`MAT 1:20-2:3; MRK 1:1`), read
  strictly into `mise::Address` with byte-offset errors, and written back.
  Contextual references (`24` after `ROM 3:21`), one-chapter books, and `end`
  given a canon scheme. `cite` is the lenient reader for people (Sefer's
  `citation.ts` ported: canon's localized names, prefixes in a go-to box) and
  `show` the label. Translation prefixes and word, character and marker
  references are refused by name: they belong to a future `srs` crate. No text
  and no onion; onion's `Sid::address` converts the TOC's answer. Plan:
  `planning/items/2026-11-05-referencing-one-address-its-spellings-and-srs-as-i.mdx`.
- `srs/` is the Scripture Referencing Standard in full, grammar only: a place,
  then a chain of refinements (word, character, element, attribute), with
  translation prefixes and named references. Locating one needs the text, so
  it is galley's (`galley/src/srs.rs`, on onion's tokens; NFD counts from
  `canon::nfd_len`). Same plan.
- bento and bttwriter each build their own wasm module (`./wasm-build.sh
  bento`, `./wasm-build.sh bttwriter`) and ship as `./bento` and
  `./bttwriter` subpaths of the npm package, TypeScript over a web-target
  module like galley. Each links only what it uses of mise and canon. Values
  cross as JSON (`bento/src/js.rs`), files as `Uint8Array`s. Sefer's switch:
  `planning/items/2026-11-05-sefer-adopts-bento-and-bttwriter-what-moves-what-s.mdx`.
- `ticket/` is the second zero-dependency leaf: the WIRE vocabulary (`Width`,
  `Space`, `Field`, `Record`, `Repeat`) and the emitters that write one
  record's Rust writer and one record's TypeScript accessor class. Every wire
  in the workspace is declared against it — onion's dish, galley's census,
  galley's find buffer, galley's mask map — so no two formats can disagree
  about how a field reaches the buffer or where a reader looks for it. A
  format's own declaration, template, magic, version, ENVELOPE, codegen bin and
  generated files stay with the format; `ticket` never knows a format exists.
  It is not in `mise` on purpose — `mise` excludes generators — and it is
  generator-only, so nothing on a runtime path calls it. Scope rule:
  `ticket/README.md`.
- Every wire format is CHECKED IN at both ends with a staleness test, and the
  only supported way to read one from JavaScript is its generated reader:
  `onion-wasm/reader.ts` (the dish), `galley/toc-reader.ts` (the census),
  `galley/find-reader.ts` (find), `galley/mask-reader.ts` (the mask map),
  `galley/sous-reader.ts` (the findings snapshot, generated by `sous-core`'s
  own emitter). A consumer never learns a layout, so a version shift is a loud
  failure at `open` rather than a field silently misread. Two formats are still
  read by hand and are the standing debt: galley's corpus envelope (`ONCP`,
  written in `galley/src/corpus.rs`, decoded inside
  `onion/reader.ts.tmpl`) and sous-core's, whose reader is typed in
  TypeScript with only its numbers generated.
- `sous-core` never depends on onion. It takes a neutral borrowed view of the
  analysis. Do not add the dependency; see the charter's ownership boundaries.
- `galley` is the one adapter allowed to depend on both. `galley::sous` owns
  the Onion→Sous seam entire: `OnionBook` (the `ProjectedBook` over Onion's
  mask and TOC), coordinate rebasing, and findings publication, cold
  (`publish_onion_findings`) or resident (`Expediter`).
- `galley` has ONE caching layer, `galley::pantry`. Everything kept between
  calls is there or on a store it defines: chunk products (`pantry/chunks.rs`,
  the one place a byte ceiling is enforced), per-book products and retained
  text, and the content-addressed `derived::Store`s the Expediter's caches are
  instances of. `pantry/budget.rs` attributes every resident byte to pinned,
  hot, or rebuildable with no residual. `galley/src/pantry.md` is the one
  caching doc; `galley/src/sous/expediter.md` is publication order and
  counters.
- `sous-cli` depends on `sous-core`, `galley` (the stateless `galley::sous`
  publication seam) and `onion` (default features off), only to call them.
- `sous-chef/donor/` (gitignored) is the v1 spike's source, kept to read and
  port from by hand. Not a crate; nothing builds it. `donor/deps.toml.txt`
  records what it compiled against.
