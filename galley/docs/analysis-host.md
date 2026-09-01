# Galley analysis-host boundary

Status: accepted direction; the Onion `Warmer` exists, while the composed
Onion + Sous host described below is not implemented yet.

This note records the lifecycle decisions that belong to Galley rather than
either engine. Sous remains independent of Onion. Galley supplies their shared
workflow, cache, coordinate conversion, and publication boundary.

## Invocation owns text; Galley does not own the document

An analysis invocation receives a caller-ordered list of complete books. Each
book carries its caller navigation identity and an owned source string for the
duration of that invocation. An optional source corpus follows the same shape.
Book array order is not scripture identity: Sous pairs target and source books
by `BookKey`, while `BookIndex` points back into the caller's particular list.

Galley does not retain the editor's rope, accept splice edits, or maintain a
second mutable copy of a document between calls. Paying the small cost to move
or encode the complete current strings avoids a distributed mutation protocol
whose revisions or coordinates could diverge when a splice is lost, rejected,
or expressed in the wrong coordinate space.

Every returned location is numeric and relative to exactly the string supplied
for that invocation. Borrowed source slices do not cross the call boundary.
After publication is complete, the invocation may drop its strings.

The implemented Onion `Warmer` already follows the important half of this law:
`parse` receives the complete current `&str` on every call and retains only
content-addressed products. The future Sous composition must preserve that
property rather than turning Galley into a document owner.

## What may remain resident

Galley is resident in derived work, not canonical text. Reuse has two distinct
identity laws:

| Product | Reuse key | Reuse consequence |
| --- | --- | --- |
| Onion/Sous chapter observations | checksum of every declared local input plus schema/context stamp | unchanged chapters skip their expensive map work |
| producer projection/source-map data | exact raw-book checksum plus producer/schema stamp | projected offsets may be mapped through a byte-identical later input |
| detached UTF-16 index/map data | exact raw-book checksum plus UTF-16 schema stamp | unchanged books skip rebuilding their byte-to-UTF-16 map |
| per-invocation UTF-16 cursor | none | recreate it against the current string; its traversal position is not reusable state |
| book/corpus summaries and judgments | none independently | recompute from current observations in deterministic order |
| packed findings | complete analysis snapshot identity | replace as one corpus publication; do not reuse an unchanged book section alone |

The current Onion `Utf16Index<'s>` and `Cursor<'s>` borrow their source. Those
exact values therefore cannot outlive an invocation unless Galley also retains
the source, which this contract forbids. Reusing UTF-16 work means retaining a
detached table such as owned run/index data and binding or applying it only
after the new input matches the raw checksum. The concrete detached type is an
implementation choice, not yet a public API commitment.

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

Chapter observations are the authoritative reusable cache. Carry, aligned
units, book summaries, project inventories, denominators, judgments, and
packed findings are products of the current deterministic reduction.

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

