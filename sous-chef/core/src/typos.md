# `sous_core::typos`

```text
sous --typos testData/exampleCorpora/en_ulb
  Possible typos: rare words one edit from a common word. Review, do not
  trust — real on agglutinative languages, mostly noise on short-word
  languages like English.
  water (525)   ← 3 possible slips
      waver (1)   JER 31:22
      waiter (2)   JHN 2:8, JHN 2:9
      wafer (3)   EXO 29:23, LEV 8:26, NUM 6:19
```

An on-demand reviewer action, not a channel: it never fires a wire row, is
never judged, and has no `Channel` or `PatternKey`. `sous --typos <corpus>`
is the whole interface — no `--publish`, `--report`, or `--source` flag
composes with it.

## What it is not

`evidence.md`'s two 2026-09-07 rows measured this as a corpus-wide rule and
rejected it: a flat top-N ranked by neighbour frequency collapses onto a
handful of short, high-degree English function words (`will`, `for`, `that`)
— 0-1 of a top-25 read as a real typo. Grouping by frequent TARGET instead of
ranking by neighbour count is the fix the second row recommended and this
module ships: each frequent word's own collision set is a group a reviewer
works through on its own terms, rather than a ranking that buries a language
like Swahili's genuinely useful `katika`/`kama` clusters under English's
`will`.

It is also not fast enough to ride a publication — 20-235 ms parallel per
corpus in the tier, against the ~90 µs a corpus-wide judge costs on every
keystroke (`evidence.md`, 2026-09-03 D2a-2) — which is exactly why it lives
behind a flag a reviewer pulls deliberately, not behind `Brigade`.

## The algorithm

No BK-tree. For every word under `rare_below` occurrences, generate its
Damerau-Levenshtein-1 neighbourhood (one insertion, deletion, substitution,
or adjacent transposition of scalars) over the CORPUS'S OWN scalar alphabet —
not a fixed ASCII set, so Amharic and Devanagari get their own — and look
each variant up by hash in the word→count table the walk already built. A
candidate is a rare word with at least one neighbour at or above
`frequent_at_least`.

Three filters, each a plain `TypoConfig` field:

- `skip_title` — drop a rare word ever seen in Title form anywhere in the
  corpus: a likely proper name (`Esek`, the well in GEN 26:20), not a slip.
- `min_scalars` — require the rare word to hold at least this many scalars;
  a 3-letter word collides with an unrelated frequent word by chance far too
  often to be worth a reviewer's time.
- `shared_first` — require the frequent neighbour to share the rare word's
  first scalar; cuts the neighbourhood before the lookup, for free.

`evidence.md`'s 2026-09-07 re-measure found these cut volume 35-90%, sharpest
on the large alphabets (Amharic, Greek), for no extra wall time — the shared
first scalar filter runs before a variant is even hashed.

## `WordCounts` and who builds it

`typo_candidates` takes a `WordCounts`: one `WordEntry` per case-folded word,
carrying its own text, count, whether it was ever seen Title-cased, and up
to a few verse references kept while walking. The module never sees USFM,
Onion, or a book's coordinates — the CLI walks the corpus with
`sous_core::words::for_each_word`, recovers each word's exact scalars from
its own copy of the projected text (a hash alone cannot be turned back into
letters), and keeps the first few occurrences' addresses as it goes.

## Parallel, but not in here

`rare_word_candidates` is pure and touches no shared state, so a caller may
run it across rare words with `rayon` and fold the results with
`group_candidates` afterward — `typo_candidates` itself is the serial
composition of both, kept as the reference the unit tests exercise directly.
The parallel loop lives in `sous-cli`, not here: `sous-core` carries `rayon`
as a dev-dependency only (examples and benches), by design — see the crate's
own `Cargo.toml` comment. `core/examples/edit_neighbors.rs` remains the
measurement harness this module was ported from: it sweeps every filter
combination and times serial against parallel over the whole 8-corpus tier,
which the shipped CLI action does not.

## Grouping and ordering

Groups are ordered by candidate count descending, then target text; within
a group, candidates by count ascending then text — the smallest, most
surprising rare word first. A rare word one edit from two different
frequent words appears in both groups' candidate lists; nothing here picks
a single "best" match the way the pre-filter measurement did.

## Honesty, not a verdict

The two `evidence.md` rows this ported from are the honest read: on
`WA-en-ulb`, all-filters-on, 0-1 of a top-25 sample is a real typo — most of
what fires is a legitimate short word or plural one scalar from an unrelated
frequent word. On `swhulb`, roughly 15-18 of 25 read as genuine slips on
`katika` ("in/at") and `kama` ("like/as"), and the `yake` ("his/her") cluster
is murkier still — Swahili's agglutinative morphology produces exactly the
counterexample the idea shelf named: a legitimate inflected form one edit
from an unrelated stem (`akisoma` "while reading" vs `akisema` "while
saying"). This is why the header line says review, not trust, and why the
rule stays a reviewer's tool rather than a rule that fires silently.
