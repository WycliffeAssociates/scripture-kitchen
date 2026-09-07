# Galley analysis-host boundary

Status: implemented for one pass. `galley::sous::publish_onion_findings` is the
stateless cold path (fresh derivation per invocation, no retained cache);
`galley::Pantry` is the registry, text retention and the per-book `Entry`
handle included (see `galley/src/pantry.md`); and `galley::sous::Expediter`
(see `galley/src/sous/expediter.md`) is the composed publication that reads
from it — chapter observations keyed by content, mapped at `publish` or, for a
book the Pantry keeps no text for, at `update`, rebased through the retained
mask and UTF-16 table, and swept per publication down to a book's retained
generations. Parallel map and the `Reference` role are not built.

This note records the lifecycle decisions that belong to Galley rather than
either engine. Sous remains independent of Onion. Galley supplies their shared
workflow, cache, coordinate conversion, and publication boundary.

## Update owns text; Galley does not own the document

Revised 2026-09-02. The caller registers each book under an opaque `BookId`
it keeps consistent (a file path in practice) with a role, `Target` or
`Reference`, and replaces a book only whole: `update(id, role, text)`. For a
target, Galley retains the text as last updated and derives its products
beside it — chunk products, TOC, mask, detached UTF-16 index, chapter
observations. Unchanged books are never resent; analysis, publication, and
find run against the retained copy. A target may NOT opt out of text retention: placing its findings rescans that
text, so `update_with(id, Role::Target, Retain::ProductsOnly, text)` returns
`Err(PantryError::TargetNeedsText)` rather than registering a book publication
would fail on. `Retain::ProductsOnly` belongs to `Reference`, and
`Err(PantryError::NoText)` is what a text-needing operation answers for one. Measured motivation: marshaling a 5 MB corpus into WASM costs 3.5–5 ms,
a quarter of a frame, while 5 MB of retained text is cheap (evidence.md,
2026-09-02). Book order in a publication is canonical by `BookKey` within a
role, ties by id, so `BookIndex` does not depend on the order of updates, and
the corpus buffer carries the ids in a string table.

Galley never accepts a splice and never mutates its copy in place; the copy
is replaced whole or not at all, so it cannot drift from the editor's buffer
by anything but staleness, and staleness heals on the next update. The
earlier rule — resend every string every call — remains a valid resync and is
no longer required, because a 100 MB aligned corpus would otherwise be
re-encoded on every keystroke. A host with such a corpus decides for itself
whether to retain its text (edit and search it) or opt out — a reference may;
a target may not.

Every returned location is numeric and relative to the exact string last
supplied for that book, which is also the string Galley retains.

The implemented Onion `Warmer` already follows the important half of this law:
`parse` receives the complete current `&str` on every call and retains only
content-addressed products. The future Sous composition must preserve that
property rather than turning Galley into a document owner.

## What may remain resident

Galley is resident in derived work, not canonical text. Reuse has two distinct
identity laws:

| Product | Reuse key | Reuse consequence |
| --- | --- | --- |
| Onion/Sous chapter observations | checksum of every declared local input plus schema/context stamp; which chapters those are is `Fingerprint::changed_chunks` | unchanged chapters skip their expensive map work |
| producer projection/source-map data | exact raw-book `RawChecksum` plus producer/schema stamp | projected offsets may be mapped through a byte-identical later input |
| detached UTF-16 index/map data | exact raw-book `RawChecksum` plus UTF-16 schema stamp | unchanged books skip rebuilding their byte-to-UTF-16 map, retained as `mise::utf16::Utf16Table` |
| per-invocation UTF-16 cursor | none | recreate it against the current string; its traversal position is not reusable state |
| book/corpus summaries and judgments | none independently | recompute from current observations in deterministic order |
| packed findings | complete analysis snapshot identity | replace as one corpus publication; do not reuse an unchanged book section alone |

The chapter observation's reuse key is `galley::sous::ObservationKey`, and the
snapshot identity is xxh3-128 over the canonical (`BookKey`, `BookId`,
`RawChecksum`) table plus the pass schema.

The current Onion `Utf16Index<'s>` and `Cursor<'s>` borrow their source. Those
exact values therefore cannot outlive an invocation unless Galley also retains
the source, which this contract forbids. Reusing UTF-16 work means retaining a
detached table such as owned run/index data and binding or applying it only
after the new input matches the raw checksum. The concrete detached type is a
host implementation choice: `mise::utf16::Utf16Table`, one bit per source byte
marking where a UTF-16 unit starts, is what `galley::Pantry` retains.

A markup-only edit changes the raw checksum, so its projection and UTF-16
coordinate data must be rebuilt. If the rule's projected chapter input is
unchanged, its config-free Sous observation may still be reused under that
observation's separate key and resolved through the new source map.

Target and source observations are cached independently. Adding, removing, or
changing a source corpus cannot invalidate target-only observations. Pairing,
proportionality, and their reductions still rerun against the current two
corpora.

## One invocation's coordinate pipeline

The internal and published coordinate spaces are deliberately different:

```text
owned raw book String (UTF-8)
    -> Onion tokens / TOC / verse-text Mask
    -> Sous projected-book UTF-8 findings
    -> Mask or producer locator -> raw-book UTF-8
    -> UTF-16 map -> raw-book UTF-16
    -> complete corpus findings buffer
```

The projection and UTF-16 conversion must be tied to the exact invocation
input, either freshly built or admitted by an exact raw checksum match. Galley
performs this composition before releasing the input strings. Sous does not
carry raw-USFM or UTF-16 coordinates, and the editor does not infer them.

For a vref producer, the same projected-book contract maps a finding back to
the original line/newline bytes and numeric designator. It does not pretend a
lossy vref export has raw-USFM coordinates.

## Reduction and publication

Chapter observations and book aggregates are the authoritative reusable
caches. Fold seam state, aligned units, project inventories, denominators,
judgments, and packed findings are products of the current deterministic
reduction.

A change in one chapter may change a corpus denominator and therefore add,
remove, or alter findings in an otherwise untouched book. This is visible at
small support counts even when a large corpus threshold would appear stable.
Consequently Galley publishes one complete findings snapshot after each
analysis. The buffer contains a corpus header and caller-ordered book directory
so a consumer can seek directly to one book and lazily decode its fixed-width
rows, but the sections are not independently reusable cache authorities.

Galley supplies the publication's snapshot identity and published book lengths
after rebasing to its declared coordinate space. The hot findings buffer does
not serialize the complete resident analysis state. Typed rule inventories and
details may remain ordinary derived responses until measurement justifies a
second binary format.

## Details and searches

A compact finding can render its code-specific summary without materializing
the rule inventory. Rich detail is valid only against the matching analysis
snapshot. A `FindingHandle` therefore combines snapshot identity with the
immutable row position; `book_idx`, `from`, `to`, and rule kind remain
navigation and selector inputs rather than a universal detail key.

`galley::find` is the search half of that, and it belongs to Galley rather
than to either engine for the same reason this note exists: Onion contributes
the mask and its two-way offset map, Sous contributes nothing at all, and what
is left — running a needle over the projection and placing the answer back in
the document — is workflow. `Find::in_book` reads a Pantry entry's retained
mask and text; `Find::in_pantry` sweeps a role in canonical book order. A hit
carries BOTH coordinate spaces, `projected` for a consumer that reads the view
and `source` for one that edits the document, and a hit crossing a masked gap
comes back as one source range per contiguous piece rather than one range that
would swallow the markup between them. No wire row, no `Expediter` involvement,
and no `sous_core` type in the API — the contract is `galley/src/find.md`.

If a detail or site-search operation needs source text after the analysis call,
the caller supplies the current complete string again. Galley validates its
checksum before applying cached coordinate or inventory state. This preserves
the no-resident-document boundary while allowing first-class `sites(query)` or
rule-inventory workflows.

## Explicit non-goals

- no resident canonical rope or splice/update protocol;
- no borrowed source slices in results or caches;
- no independently authoritative per-book findings buffers;
- no persisted binary codec for the full analysis state yet;
- no requirement that Sous depend on Onion;
- no promise that the current Onion-only WASM `Galley` API is the final
  composed-analysis API.

