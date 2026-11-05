# Versification schemes

One file per scheme, in the Copenhagen Alliance mapping format
(`maxVerses`, `mappedVerses`, `partialVerses`, `excludedVerses`). `gen-canon`
reads `maxVerses` and `excludedVerses` today; mapping (plan step 3) will read
the rest. Licence and attribution: `../../NOTICE`.

## The six vendored files

`org`, `eng`, `lxx`, `vul`, `rsc` and `rso` are copied unchanged from
nida-institute/versification, `versification-mappings/standard-mappings/`, at
commit `519af776ab65d89a51e37f750779250f940459ef`. To update them, copy the
new files over these, rerun `cargo run -p canon-gen`, and review the table
diff and any fixture that changes.

`ethiopian_custom.json` from the same directory is not vendored: nothing here
asks for it yet.

`org`, `eng` and `lxx` also count six Paratext-only texts (BLT, DNT, JDB,
JSA, SST, TBS: alternate Greek forms of Bel, Daniel, Judges, Joshua, Susanna
and Tobit). They are not USFM book identifiers, so `gen-canon` drops them by
name. Any other unknown code is an error.

## `ulb.json`

Measured from `testData/exampleCorpora/en_ulb` (WycliffeAssociates/en_ulb) on
2026-11-05: for each book, the highest verse number in each chapter, with
bridges counting their upper end. en_ulb has no internal gaps, so the counts
are the verses it numbers. `mappedVerses` is `eng.json`'s rows for the 66
books, copied, because en_ulb numbers like `eng` everywhere except:

| chapter | en_ulb | eng | org | en_ulb's footnote |
|---|---|---|---|---|
| ACT 19 | 40 | 41 | 40 | "Some Greek copies number this last sentence as verse 41." |
| 2CO 13 | 13 | 14 | 13 | "Some translations number this sentence as verse 14." |

`eng.json` maps neither chapter, so `ulb` needs no mapping rows of its own
there: in both, `ulb` and `org` already agree.

The two disagreements the plan expected (3 John 1 and Revelation 12) are not
disagreements: en_ulb has 15 and 18 verses, as `eng` and `org` do.

To re-measure after en_ulb changes, recount `\c`/`\v` the same way and
compare with `eng.json` chapter by chapter. A new difference is a
versification decision for review, not a silent update.
