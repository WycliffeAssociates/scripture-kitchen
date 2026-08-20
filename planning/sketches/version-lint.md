# Version-family lint (roadmap 5 — cheap, slots anywhere)

The gap this fills: `MarkerRow.deprecated` is a BOOL, and the
version-data law (settled-facts) forbids a version column on rows —
"the decisive fact is owned by no row." So the Version family needs a
small AUTHORED dataset on lint's side plus one or two codes.

## The dataset (PROPOSED — lives beside LINT_ROWS in lint/rows.rs)

```rust
/// Per-marker version facts lint owns. NOT MarkerRow columns (the law);
/// membership here is authored off the marker pages' history notes.
pub struct VersionRow {
    pub marker: &'static str,        // row-name key, like the books table
    pub deprecated_in: UsfmVersion,  // first version that says "don't"
    pub removed_in: Option<UsfmVersion>,
    pub replacement: Option<&'static str>, // "pn" for addpn — feeds the fix label
}
const VERSION_ROWS: [VersionRow; N] = [ /* … */ ];
```

Seed membership = the five `deprecated: true` rows in tables/rows.rs —
`addpn` (→ `pn`), `fdc` (→ `\f` + `\dc`? read the page), `ph` (→ `\li`
indent forms), `pro`, `xdc` — plus whatever the 3.2 release notes list
that our rows predate. `UsfmVersion` gains earlier variants only if a
page actually names one (don't invent V3_1 unless needed — `jmp`'s
link-* trio is already an ATTR-level deprecation handled by
AttrStatus::Deprecated, not this table).

## The codes

- `deprecated-marker` — a Marker/ClosingMarker token whose row is in
  VERSION_ROWS. Severity: Warning when declared `\usfm >=
  deprecated_in`; escalation to Error at `removed_in` via the EXISTING
  escalation column mechanics. GATED like phase 3's trailing-form rule:
  **no declared \usfm → silent** (a 2.x-era file full of `\addpn` is
  correct for its era; phase-3 precedent — gate, don't guess).
  aux = AuxKind::Version (the deprecating version). Fix when
  `replacement` is Some: replace the marker name bytes, BOTH opener and
  its closer (two edits, one fix) — mechanical, oracle-provable.
- `deprecated-attribute` — `AttrStatus::Deprecated` hits from the k/v
  interpreter (`\xt link-href`, `\jmp link-*`). Belongs to this family
  by category, ships with the interpreter (roadmap 1/2 window). Listed
  here so the family is complete in one sketch.

## Where it runs

One arm in Flat's marker match: `version_rows lookup` — a ≤8-entry
linear scan of &'static strs, or a bitflag stamped at codegen if it
ever grows (it won't). No new pass, no new machine.

## Tests (plain English)

- `\usfm 3.2` + `\addpn text\addpn*` → one deprecated-marker at the
  opener, fix relabels both halves to `pn`; apply → relex → relint
  clean (the fix oracle).
- No `\usfm` + `\addpn` → silent (the gate).
- Corpus: run --lint-stats and pin — expect zero or a handful; only
  bdf_reg/en_ulb might carry legacy markers. Investigate any hit at the
  bytes before pinning, per the standing practice.

## Open

1. Exact deprecated/removed versions per marker — authored off each
   marker page's history note (needs the tcdocs/spec read; the five
   names above are the confirmed seed).
2. Does `fdc`/`xdc` have a mechanical replacement or is it a
   restructure? If restructure → no fix, label None, same reasoning as
   attr-trailing-form.
