# Versification: STEP's TVTMS as a kitchen table plus a small engine (idea, NOT SCHEDULED)

Dropped here 2026-09-25 from a Sefer discussion: every cross-resource pane
needs to map one Address between numbering schemes (Macula Hebrew against an
English-numbered translation, TN chunks, TW occurrences), and Location has no
such primitive (Sefer `planning/00-ideas/resource-kinds.md`, open question
1). Nothing decided; this records the source and a possible split.

## The source

[`STEPBible/STEPBible-Data/Versification`](https://github.com/STEPBible/STEPBible-Data/tree/master/Versification):
ONE file, TVTMS ("Translators Versification Traditions with Methodology for
Standardisation"), Tyndale House, CC BY 4.0. Read 2026-09-25:

- 5.8 MB, ~29.9k lines, tab-separated, UTF-8 with BOM, prose header.
- `#DataStart(Condensed)` (lines 162–4109): one block per section, the
  traditions side by side (English KJV, Hebrew, Latin, Greek, Greek2, …).
- `#DataStart(Expanded)` (4181–27570): one row per verse per tradition —
  `SourceType | SourceRef | StandardRef | Action | NoteMarker | notes… | Tests`.
  75 distinct SourceTypes (Latin, Greek, `Eng-KJV+Hebrew`, `GrkTitleSeparate`, …).
- The standard is English (KJV/NRSV, identical in the OT). Every mapping goes
  through it: source → standard → target, never pairwise.
- Actions, by count: Renumber verse 9,785 (+2,030 starred), Keep verse 8,383,
  Concatenation 664, DividedPrev 589, MergedPrev 404(+135), IfEmpty 254,
  Renumber title 248, CopiedFrom 122, Psalm title 117, MovedFrom 18(+43).
  Not a 1:1 renumbering: verses merge, split, move, and Psalm titles become
  verses.
- Part-verse references: `Gen.3:1!a`, `Gen.3:1!b`.

## The part that matters: detection is per section, by tests

A Bible does not "have a versification". It may follow Latin for one section,
Hebrew for another, English elsewhere. TVTMS's methodology says which, per
section, with a `Tests` column evaluated against the text itself:

```text
Gen.3:1=Exist & Gen.2:24=Last & Gen.6:1<Gen.6:2
```

Three predicates, all answerable from a parsed book: a verse **exists**
(29.5k uses), a verse is the **last** in its chapter (24.3k), and one verse
is **shorter/longer** than another (`<` 1.5k, `>` 0.9k). Onion already
knows the first two from its chapter/verse spine and the third from verse
extents. So detection needs no metadata the project declares — it reads the
project's own shape, which is the kitchen's habit anyway.

## A possible split

`mise`'s scope rule allows a standard-derived data table used by more than one
crate, and forbids logic and I/O. So:

- **`mise::versification`** — the GENERATED table only: compact rows
  (source type, source ref, standard ref, action, test expression) in the
  shape `unicode/table.rs` already sets. Only if a second crate needs it;
  otherwise it lives beside its one consumer.
- **The generator** reads a pinned, checksummed copy of the TVTMS file and
  writes the table, with a regeneration gate — the UCD pattern
  (`gen-unicode`). The licence allows "include any part of this data in
  software" and "reformat it for your application, without changing the data",
  and asks that the FILE not be redistributed: pin by URL and hash, don't vendor
  the text. A changes note is owed if we ever correct a row.
- **The engine** (onion, or a small crate over it) evaluates a section's
  tests against a parsed book → which tradition each section follows → maps an
  Address to and from the standard. Exposed on the galley wasm build too,
  since galley is a superset of onion.
- **Sefer** consumes it through Location: `mapAddress(address, from, to)`
  plus "which scheme does this book follow here", pure and synchronous like
  the rest of Location.

## Questions for when it is picked up

1. **`!` collides.** TVTMS's `!a` is a PART of a verse; U23003's `!` (Sefer
   `location.md`) refines to a word. One of them needs another spelling at
   any boundary where both appear.
2. **Merges and splits aren't functions.** Mapping `Gen.6:1` in Greek gives
   `Gen.5:32; 6:1` — an Address can become several, or a part. The result type
   has to say "one", "several", or "part of", not pretend it's 1:1.
3. **Resources vs projects.** A project's scheme is detected from its text; a
   resource (TN, TW, Macula) mostly declares one (ULB ≈ English; WLC ≈
   Hebrew). Is a declared scheme trusted, or checked by the same tests when
   the resource has text?
4. **Size.** Condensed vs expanded, and whether the table ships whole or per
   book. The expanded section is ~23k rows; compact, it is probably small next
   to the Macula blobs.
5. **Deuterocanon.** TVTMS covers it and English versions disagree there;
   decide whether it is in scope before generating.
