# Review: custom `\z` markers — fixes before commit

Review of the uncommitted tree implementing `custom-markers.md`. The gate was
re-run independently: 965 nextest passed, 15 skipped; `cargo clippy
--all-targets` clean; `cargo fmt --all --check` clean.

Verdict: faithful to the plan and to the repo's habits. All five recorded
departures from the plan (`PlainOnly` templates, the registry branch in
`extensions::marker_idx`, `priority: None`, the digit-trimmed cell align, the
Pantry re-deriving rather than dropping) are improvements and stand. Five
things change before commit; two decisions are settled below; a few nits.

Same rules as the pass: `cargo nextest run` and `cargo clippy --all-targets`
green over the whole workspace, `cargo fmt --all --check`, both codegens
idempotent, `conformance.mjs` passing. Do not commit.

## 1. `Marker.name()` in the reader lies on a template row

`onion-wasm/reader.ts` (generated from `onion/src/wire/emit.rs` /
`reader.ts.tmpl`): `Marker.name()` returns `null` for row 0 and otherwise
`MARKERS[idx].name`. For a registered `\zmyp` that is `"zpara"` — the
template's name, which the table doc itself says is never the marker's.

- Make `name()` return `null` whenever `isExtension(idx)`, the same rule row 0
  already has: the spelling is only in the document. Consumers that already
  fall back on `null` then need no change.
- The new table doc tells consumers to read `TokenView.spelling`. No such
  accessor exists. Point at `span()` (and `Marker.name()` being `null`).
- `conformance.mjs`: assert `name()` is `null` on every template row, the way
  it asserts row 0 has no name.

## 2. `cell_align` invents a naming convention

`onion/src/export.rs::cell_align` strips a leading `z` so `\ztcr2` aligns
`end`. The spec's `cell` category carries no alignment, and the helper's own
comment says "nothing else about the name is interpreted" while interpreting
it.

- Keep the digit trim — the spec rows need it (`\tcr1`).
- Drop the `strip_prefix('z')`. An extension cell aligns `start`, in all three
  exports.
- Update `extensions_behave::a_cell_extension_aligns_by_its_spelling` to
  assert `start` for `\ztcr2`, and rename it to say what it now proves: a cell
  extension aligns start, the spec cells are unmoved.
- Ledger: one line under the existing "Cell alignment" entry.

## 3. Name validation is written twice

The three name rules and their reason strings live in both
`mise::extensions::close` and `onion::extensions::Extensions::new`.

- Add `pub fn check_name(name: &str) -> Option<&'static str>` to
  `mise::extensions` (empty → "…no name"; not `z`-initial; not ASCII
  alphanumeric), and call it from both. The two callers keep their own
  duplicate-name and missing-category reasons, which are theirs.
- The reason strings stay exactly as they are so no test moves.

## 4. A poisoned lock swallows the install

`onion/src/extensions.rs::set_extensions`: on a poisoned write lock it installs
nothing, then bumps the generation and returns the reports as if it had
installed. The new value is a fresh `Arc`, so poison carries no information
here.

- `set_extensions`: `REGISTRY.write().unwrap_or_else(PoisonError::into_inner)`
  and install unconditionally.
- `current()`: the same recovery on the read lock, instead of answering the
  empty registry.
- Drop the "poisoned means empty" comment; replace with one line saying why
  poison is ignorable.

## 5. The onion unit test mutates the global with no guard

`extensions.rs::tests::installing_bumps_the_generation` installs `zgen` and
clears it at the end. A panic mid-test leaves `zgen` installed for every other
test in the process under `cargo test`. `galley/src/pantry/tests.rs` already
has the `Restore` drop-guard pattern for exactly this.

- Add the same guard to the onion test (or lift it to a tiny shared test
  helper if a third site appears — two is not yet three).

## Settled

- **Eager re-derivation in `Pantry::check_registry` stays.** A registry
  change is a project-open event, not a keystroke one; paying the whole corpus
  once on the next door is acceptable. Record that in the ledger's Pantry
  paragraph in one sentence, so the next reader knows it was weighed.
- **`is_empty()` → `resolves_nothing()`.** `attribute` and `internal` entries
  sit in `declared()` but register no name, so a registry holding only those
  is "empty" while declaring two. Rename `is_empty` to say what the scanner's
  early return actually asks, doc it as "no name maps to a row", and leave
  `declared()` as the full list — a host may want to show "accepted, no USFM
  behaviour" beside those two, and a valid `markers.ext` line must never
  vanish silently.

## Nits

- `lint/structure.rs`: the `debug_assert!` message is a paragraph; the repo
  wants one line. Keep the claim ("no template copies a U25003 container
  row"), drop the rest.
- `mise::extensions::Malformed.line` is `0` for registry-level reports. A
  sentinel where an `Option<u32>` would be honest. Not blocking — the mise
  type is shared and the file reader is the common case — but if `Malformed`
  is touched for §3 anyway, consider it.
- `generated.rs.tmpl::marker_idx` still carries its own `z` short-circuit
  while the scanner never sends it a `z` lexeme. Harmless and keeps the spec
  lookup total; fine to leave, but the comment should say it is a belt, not
  the door.

## Kept, and worth saying so

- Templates copied from their source rows, then DIFFED against them in
  `every_template_copies_its_source_row`. Curating `\p` reaches `zpara` or
  the build fails.
- `PlainOnly` on the templates: the spelling rule in the column that already
  means spelling. Better than the plan's §2.
- `lex_with` threaded through the scanner, the global read once per document.
- `spelled_len` is behaviour-preserving for every spec row and dead for
  templates today (only deprecated spec rows reach `rename`); the comment says
  so accurately.
- The quirks entry: `unknown-marker` fires for openers only, so an unknown
  milestone has never raised anything. Older than this pass; now written down.
- `wire/emit.rs::doc_ts` repaired a pre-existing misaligned JSDoc block.
