# Level 3 — presence and shear

Two proposals over the same pairing. Presence has landed; shear has not.

## Presence — the claim

Three statements about keys and counts, each already an `AlignmentFact` or one
comparison away:

- **Missing**: a source verse key with no target unit in that book.
- **Extra**: a target verse key with no source unit in that book.
- **Empty**: a paired unit whose target side has zero graphemes while the
  source has more than zero.

Wire code 4 `Presence`, one row per coalesced run: consecutive keys of one kind
in one chapter are one statement, so a whole absent chapter is a single row and
not thirty. The row says HOW MANY keys it covers; a consumer reads WHICH from
the gap in its own table of contents around the published span. The shape of
the computation and where the span lands:
[`../core/src/presence.md`](../core/src/presence.md).

## What is NOT claimed

- Not "this verse is untranslated" and not "this translation is missing". A
  different declared source legitimately keeps a different list of verses, and
  most of what fires on real corpora is exactly that.
- No expected-verse table and no versification repair. Sous holds no opinion
  about which verses a book ought to have; both sides' keys are the caller's.
- No ratio. An absent or empty unit produces no length row, and nothing here
  fabricates a zero-length one.
- Nothing about content. An `Empty` row says the target unit counted no
  graphemes beside a source that counted some. It may be intentional drafting.

An ambiguous duplicate and a partial-overlap bridge stay `AlignmentFact`s and
never become rows: the pairing could not say which key belongs to which, so
neither can a finding. A source book with no target book of its key produces no
rows either — presence is per paired book, and a book the target simply does
not hold is the caller's own inventory question, reported as facts.

## Volume

The default is on. Against `testData/exampleCorpora/en_ulb`, the tier's
comparable corpora fire 18 and 23 rows over 27 and 66 books, and en_ulb against
itself fires none (evidence.md, 2026-09-07). Almost every row is one key: what
fires is the received textual-critical verse set — MAT 17:21, MRK 9:44, ACT
8:37 — plus a handful of verse-division differences, which is a review list a
person can rule on in a minute.

## Shear

Status: parked. Adjacent opposite length extremes may mean content was divided
differently, or may just be real translation differences. A useful first action
is "compare these neighbouring aligned units together", showing individual and
combined lengths, rather than relabelling verses or replacing their individual
ratios. A source-compared observation — "neighbouring deviations oppose each
other while the combined lengths are less unusual" — needs declared adjacency,
bridge compatibility, opportunity counts, and counterexamples from genuine
short/long neighbours before it is a claim.
