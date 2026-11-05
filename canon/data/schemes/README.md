# Versification schemes

One file per scheme, in the Copenhagen Alliance mapping format
(`maxVerses`, `mappedVerses`, `partialVerses`, `excludedVerses`). `gen-canon`
reads all but `partialVerses`, which only lists the parts `lxx` cuts some
verses into: a mapping row that names a part carries its letter itself.
`org.json`'s own eight `mappedVerses` rows (S3Y into DAG) are not read,
because `org` is the pivot and every `org` verse is itself. Licence and
attribution: `../../NOTICE`.

## The six vendored files

`org`, `eng`, `lxx`, `vul`, `rsc` and `rso` are copied unchanged from
nida-institute/versification, `versification-mappings/standard-mappings/`, at
commit `519af776ab65d89a51e37f750779250f940459ef`. To update them, copy the
new files over these, rerun `cargo run -p canon-gen`, and review the table
diff and any fixture that changes.

`ethiopian_custom.json` from the same directory is not vendored: nothing here
asks for it yet.

### Errata

Four `mappedVerses` rows pair ranges of different lengths in a way only a
typo explains. The files stay unchanged; `gen-canon`'s `ERRATA` corrects the
rows as it reads them, and fails if one is no longer there to correct.

| scheme | row as written | read as | why |
|---|---|---|---|
| `vul` | `DAG 3:52-23` → `S3Y 1:30-31` | `DAG 3:52-53` | backwards; `org.json` pairs S3Y 1:30-31 with DAG 3:52-53 |
| `vul` | `DAG 13:1-63` → `SUS 1:63` | `SUS 1:1-63` | a whole chapter onto its last verse; it is Susanna, verse for verse |
| `vul` | `DAG 14:1-42` → `BEL 1:42` | `BEL 1:1-42` | likewise, Bel |
| `rso` | `PSA 89:2-6` → `PSA 90:1-6` | `PSA 90:2-6` | five verses onto six; the row before already sends 89:0-1 to 90:0-1 |

Two rows are dropped instead (`gen-canon`'s `DROPPED`). `vul.json` counts
Jonah as the Vulgate and `org` do (chapter 1 has 16 verses, chapter 2 has
11), but carries `eng.json`'s Jonah rows (`JON 1:17` → `JON 2:1` and
`JON 2:1-10` → `JON 2:2-11`), which assume `eng`'s 17 and 10. Read as written,
vul's JON 1:17 does not exist and every verse of its Jonah 2 maps one off.
Without them vul's Jonah is `org`'s, verse for verse.

Separately, a few rows start past their own scheme's counts (`rso`'s
`2ES 7:106-140`, `eng`'s `ESG 8:37-41`, `lxx`'s `EZR 13:1-38`). They are kept,
and `map` never answers with a verse its scheme doesn't have.

Every other unequal pair is a real merge or split (one side a single verse),
such as `rsc`'s `PSA 89:0-1` → `PSA 90:0`.

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
