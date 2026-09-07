# Level 3 — source-copy runs

**Claim.** *These N or more consecutive words of the target verse each appear,
as the same exact scalar sequence, in the paired source verse.*

That is the whole statement. It does not say the run is untranslated, and it
does not rank, score, or explain. It is reviewable where an accidental paste of
source text is plausible, and the row publishes the run so a reviewer decides
which of the legitimate readings applies.

## What it needs to be true

- The unit paired by the [Level 3 pairing law](length-proportionality.md):
  exact key plus occurrence ordinal, or a bridge against the exact contiguous
  constituent run. A partial overlap or a one-sided key abstains and stays an
  `AlignmentFact`.
- Word boundaries as `sous_core::words::walk` draws them: a maximal run of
  letters and glue, extended through one medial nonletter. A run of digits
  holding no letter is not a word, so a verse number or a year neither extends
  a run nor breaks one and is not eligible.
- **Exact surface forms.** Matching is over raw UTF-8 bytes, case included.
  `The` and `the` are two words here. Case-insensitive matching, normalization,
  and stemming are separate claims nobody has adjudicated, and this rule does
  not smuggle any of them in.
- **No name recognizer and no stoplist.** A proper-name excusal without a
  language-aware authority is exactly the move the idea shelf warned against,
  and a corpus-learned stop list would be a second, unstated rule. The run
  length is the only filter.

## The honesty bound

A 32-bit hash **nominates**; it does not prove. The source side retains hashes
and nothing else, so a "present" verdict can in principle be a collision: about
1.5e-7 per lookup against a verse's ~20 distinct words, and a false run of
three needs three of them in a row. This is stated rather than fixed, because
fixing it means rescanning source text sous deliberately does not hold. A
reviewer reading the published run against the published source verse closes
the gap by eye, which is what the review page shows both texts for.

## What it does NOT model

Every one of these is a legitimate reason for a long shared run, and none is
excused:

- shared proper names (`Isaka Yakobo Yuda`);
- a quotation the target deliberately preserves;
- transliterated or borrowed religious vocabulary;
- closely related languages, and two translations from one family — the
  measured worst case;
- shared function words, which is why one and two-word runs are not rows;
- source text intentionally left in place.

## Volume, and why it ships off

Measured over `testData/exampleCorpora/*` paired against `en_ulb`
(evidence.md, U1 (c)):

| target | rows at run ≥ 3 | at run ≥ 4 |
| --- | --- | --- |
| `bdf_reg` (27 books, another language) | 0 | 0 |
| `en_ult-fixtures` (one book, an English sibling) | 1,393 | 1,065 |
| `examples.bsb` (66 books, another English Bible) | 71,081 | 52,766 |
| `en_ulb` against itself | 31,078 | 30,978 |

Raising the floor does not rescue it: two English translations share three and
four-word runs about twice per verse whatever the bar. So
`LengthConfig::source_copy` ships **false**, and `source_copy_min_run` stays
the claim's own floor of 3. A host that knows its declared source is not in the
target's language family turns the lane on and gets the rule this document
describes; a host that declares a sibling translation would get a row on every
other line, which is not a review queue.

The first ten rows of `en_ult-fixtures` against `en_ulb`, read by eye, are all
the same finding: two English translations of one publisher sharing whole
clauses (`beginning of the gospel of Jesus Christ, the Son of God`). Not one is
a paste, and not one is an error. That is the rule working exactly as specified
and the specification being the wrong default.

## Cost

The source side retains 2.77 MB per Bible for the word lane, 61% of the raw
text it hashes — but only when the lane is on when the source is registered; at
the shipped default a declared source stays 1.17 MB, as it was before this
rule. Turning the lane on afterwards means re-sending the source's text, and a
publication reports how many books are waiting on that rather than reading as
clean. The judge walks the
target's words only for a book whose own checksum or whose source's moved; an
unchanged republication walks nothing. With the lane on against an identical
source — the worst case there is — a warm republication of a whole Bible costs
3.57 ms against 0.83 ms with it off, all of it row emission.
