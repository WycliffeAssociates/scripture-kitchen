# USX / USJ projection

Seed file (2026-08-14). How our tokens project OUT to USX/USJ, and the
facts that projection needs which are not facts about USFM markers.
Nothing here is built. Export design lives in NEXT-STEPS "Later"; this file
holds the USX-side data those surfaces will need.

## The governing principle (Will, 2026-08-12, recorded on the `cat` row)

> `\cat category\cat*` is CHAR-SHAPED — it takes content and requires its
> closer, exactly like `\nd`. USX expressing the category as an attribute
> on the enclosing note/sidebar is a PROJECTION artifact, the same class of
> thing as `cp`→`pubnumber`, and does not change what the marker IS.

So: **"becomes an attribute in USX" is a property of the projection, never
of the row.** The marker keeps its own kind, scope, and closing behavior;
the exporter is what flattens it. This is why the marker table has an
`html_element` column but no "usx_element" column — and it is the reason to
keep these facts here rather than adding a row column that would lie about
what the marker is.

## Markers whose CONTENT becomes a USX attribute

The 3.2 docs flag these with an "attribute" indicator (Will, from the 3.2
pages): **`cp`, `vp`, `ca`, `va`, `usfm`, `cat`**.

Only one target name is confirmed so far — `cp` → `pubnumber`, from the
comment above. **TODO: read the remaining five target attribute names off
their own marker pages before writing any exporter**; do not infer them
from the marker name, since `cp`→`pubnumber` already proves the names
differ. Note the shape this implies for the exporter: each of these
projects onto its ENCLOSING element, so the walker's frame is what the
attribute attaches to.

Consistent with our own rulings, none of this changes the token stream:
`ca`/`cp`/`va`/`vp` are adjacency lint rules `(lastMarker, token)` with
empty context slices (NEXT-STEPS), and `cat` opens an ordinary Character
scope.

## Attributes that are already attributes in USFM

Verified against the shipped 3.2 pages, 2026-08-14:

| marker | attribute | status | notes |
|---|---|---|---|
| `tl` | `lang` | Optional, DEFAULT | ISO639-1 2-letter source language. Syntax `\tl content\|@lang\tl*` |
| `wl` | `lang` | Optional, DEFAULT | same shape as `tl` |
| `ta` | `a-<identifier>` | Optional, one or more | a PATTERN, not a name — see the gap below. Introduced 3.1.2 (U24002 Textual Alternatives) |
| `vid` | `ref` | Required | milestone; "current reference identifier" for a scripture fragment |
| `vid` | `h` | Optional | supplies the current header text, so a fragment need not carry `\c`/`\v` |

`vid`'s row is already complete and correct (`SelfClosingSpan`,
`ref` required + `h` optional).

## Open gaps this surfaced

1. **Attribute PATTERNS — RULED and landed 2026-08-14.** A name ending in
   `*` in `defined_attributes` is a prefix wildcard, so `\ta` carries
   `("a-*", Optional)`. Rationale and the rejected alternatives live at the
   column's doc comment in schema.rs, which is where the matcher's author
   will be. Two consequences to honor when lint gets written: whatever
   matches attribute names is the ONE place that learns the convention,
   and `\ta`'s "one or more" is family-level cardinality — a lint rule,
   like the `sid`/`eid` conditional, not a status. The `\z` markers.ext
   config shape should reuse the same spelling.
2. **`tl` and `wl` `lang` — landed 2026-08-14** (Optional AND default, per
   the pages quoted above; codegen re-run).
3. **There is no version column.** `MarkerRow` carries only
   `deprecated: bool`, but version-keyed facts keep accumulating: `ta`
   introduced in 3.1.2; trailing attribute lists deprecated in 3.2 and
   removed in 4; `\list-s`/`\table-s` optional in 3.2 and required in 4.
   Lint severity is supposed to be keyed on the declared version (the
   `\usfm` marker's payload). Decide whether that needs a column, a
   separate authored aux table, or stays in lint's own rules table.

## Losslessness, for the record

USX and USJ cannot round-trip an attribute list: key order, whitespace
around `=`, and spacing between pairs are all unrepresentable in an XML
attribute set or a JS object. That is exactly why `AttrList` is a SPAN and
the k/v view is derived on demand — the export is the lossy view, the token
stream is not.
