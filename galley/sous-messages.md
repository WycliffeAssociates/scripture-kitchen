# Sous messages

Why every squiggle fired, as data. `describe` in
[`sous-messages.ts`](sous-messages.ts) turns a finding into one message id,
its parameters, and the literal searches behind it;
[`sous-messages.en.json`](sous-messages.en.json) is the English reference
catalog in ICU MessageFormat, a headline and details per id. The consumer owns
the final strings, adds languages, and formats every number with its own
`Intl`. Kitchen ships no translation runtime.

```ts
import { FindingsSnapshot } from "@wycliffeassociates/scripture-kitchen/sous-reader";
import { booksByPattern, describe, markBefore } from "@wycliffeassociates/scripture-kitchen/sous-messages";
import catalog from "@wycliffeassociates/scripture-kitchen/sous-messages.en.json";

const snapshot = FindingsSnapshot.open(galley.publish());
const patternBooks = booksByPattern(snapshot);      // once per publication
const finding = snapshot.book("DEU").at(0);
const pattern = finding.kind === "Convention" ? snapshot.pattern(finding.convention.pattern) : undefined;
const { id, params, queries } = describe(finding, pattern, {
  siteText: text.slice(finding.from, finding.to), // the reader has only offsets
  bookCount: snapshot.length,
  bookName: (index) => snapshot.book(index)!.key,
  patternBooks,
  before: markBefore(verseText, siteInVerseText),  // Casing only; see below
  snapshot,
});
const g = (chunks) => chunks.join("");
new IntlMessageFormat(catalog[id].headline, locale).format({ ...params, g });  // the squiggle
new IntlMessageFormat(catalog[id].details, locale).format({ ...params, g });   // "Why?"
```

## Two tiers

Every entry is `{ "headline": ICU, "details": ICU }` over one set of
parameters.

- **Headline**: one fact, and at most one alternative the reader might write
  instead, in about twenty words. No rule internals: never "a group of this
  size", "stands alone", "when another mark follows", a band or a threshold.
- **Details**: the supporting numbers, for a "Why?" expander: what is usual,
  how often, and where.

A comparison reaches the headline only when it is something the reader might
write instead:

| alternative | ids | when |
| --- | --- | --- |
| the same marks in another order | `convention.exactNeighbor.swapped`, `convention.runShape` (`swap`) | `;'` beside `';`, never across a directionless quote |
| a lookalike | `convention.rarity` (`lookalike`) | `’` beside `'`, from Unicode's `confusables.txt` |
| the same word in another form | `convention.casing`, `convention.sentenceStart` | `On` beside `on`, `his` beside `His` |

Otherwise the headline says only how rare it is, and the usual follower, the
usual class, or the most common group moves to the details. A reordering
across a straight `"` or `'` is still a fact, and the details say it plainly
("This project also writes `";`, 6 times"): a straight quote does not say
whether it opens or closes, so `".` opening a quotation is no swap of `."`
closing one.

## Glyph tags

A mark in quotation marks cannot be read when the mark is one: `“"” is
followed directly by “(”` asks the reader to find three quotes in `“"”`. So
the catalog never quotes a mark, a pair of marks or a cluster. It wraps each
one in a `<g>` rich-text tag, and `intl-messageformat` hands the tag's
contents to a function the consumer supplies:

```text
catalog   <g>{pair}</g>: this pair of marks appears only here in this project.
params    { pair: "\":" }
html      g: (chunks) => `<kbd>${chunks.join("")}</kbd>`
          <kbd>":</kbd>: this pair of marks appears only here in this project.
plain     g: (chunks) => chunks.join("")
          ":: this pair of marks appears only here in this project.
```

- `g` is required: `intl-messageformat` throws when a tag has no function.
  React callers return an element (`<kbd key=…>`) and get an array back.
- A plain-text context should keep a delimiter the mark cannot be; every
  example in this file renders `<g>` as `[…]`.
- Only marks take the tag: every parameter typed `Glyph` (one mark: `glyph`,
  `neighbor`, a mark's `usual`, `letter`, `before`) or `Glyphs` (a pair or a
  group: `pair`, `reversedPair`, `cluster`, `usualCluster`, `run`) in
  `ParamsById`, and the literal `�` of a hygiene message. A letter repeated by
  `convention.letterRun` is a glyph too.
- Words keep typographic quotes, since a word is never one: “Moses”,
  “Kohath's”. There is no word tag.

## Kinds of mark

Readers ask "what's a mark?", so every `Glyph` parameter has a `…Kind`
beside it (`glyphKind`, `neighborKind`, `usualKind`, `letterKind`,
`beforeKind`) naming what it is. A `Glyphs` pair or group takes its marks'
kinds from those.

```text
params    { pair: "'.", glyphKind: "quote", neighborKind: "sentenceEnd", count: 3, … }
catalog   {glyphKind, select, quote {A quotation mark} …} right before
          {neighborKind, select, … sentenceEnd {a mark that ends a sentence} …}
          (<g>{pair}</g>) appears only # times in this project.
en        A quotation mark right before a mark that ends a sentence (['.]) appears only 3 times in this project.
```

| kind | what it is, from UCD 17.0.0 | e.g. |
| --- | --- | --- |
| `quote` | `Quotation_Mark` | `"` `'` `“` `’` `«` `「` |
| `bracket` | general category `Ps Pe Pi Pf`, not a quote | `(` `)` `[` `⁅` |
| `dash` | `Dash` | `-` `–` `—` `−` |
| `sentenceEnd` | `Sentence_Terminal` | `.` `?` `!` `।` `。` |
| `separator` | `Terminal_Punctuation`, not a sentence end | `,` `;` `:` `،` |
| `digit` | `Nd` (checked first) | `0` `٣` |
| `symbol` | `S*` | `§` `+` `$` `^` |
| `space` | `White_Space` | |
| `letter` | `Alphabetic` | `a` `ñ` |
| `other` | anything else | `*` `/` `&` `#` |

The first five are the engine's own neighbour pools, first match wins, so
`«` is a quote and `−` a dash; the engine pools a letter or a space as
`other`. The table is generated into [`sous-unicode.ts`](sous-unicode.ts)
(`kindOf(scalar)`, `rangesOf(kind)`, `CLOSERS`) by
`cargo run -p sous-core --bin gen-unicode`, and a test fails when it is stale.
Kinds are `select` keys, so a translator words every one; a catalog `select`
over a kind names all ten (`sous_messages.rs` checks), `other` included.

## What en_ulb renders

`cargo run --release -p sous-cli -- --publish x.sous testData/exampleCorpora/en_ulb`,
headline then details, `<g>` as `[…]`:

```text
NUM 21:14  …the Wars of Yahweh, \q "... Zahab…
           [".]: this pair of marks appears only 2 times in this project.
           ["] is followed directly by another mark 129 times in this project, most often [']
           (77). This project also writes [."], 4,038 times. [".] appears in NUM and JER.
GEN 48:20  …like Ephraim and like Manasseh'." \m In this way…
           ['.]: this pair of marks appears only 3 times in this project.
           ['] is followed directly by another mark 496 times in this project, most often ["]
           (416). This project also writes [.'], 974 times. ['.] appears in 3 of 66 books.
PRO 30:15  …four that never say, "Enough": \q1 \v 16 Sheol…
           [":]: this pair of marks appears only here in this project.
           ["] is followed directly by another mark 129 times in this project, most often [']
           (77).
DEU 7:17   …how can I dispossess them?'— \v 18 do not be afraid…
           ['—]: this pair of marks appears only 6 times in this project.
           ['] is followed directly by another mark 496 times in this project, most often ["]
           (416). This project also writes [—'], once. ['—] appears in 5 of 66 books.
ISA 30:10  …They say to the seers, "Do not see;" \q2 and to the prophets…
           [;"] appears only 2 times.
           [;] stands alone 4,878 of 4,904 times. Unusual groups like this appear 5 times, all
           in ISA. This project also writes [";], 6 times.
EXO 38:26  …those twenty years old and older—603,550 men in all.
           [—] right before a digit appears only 2 times in this project.
           Usually [—] is followed right away by a letter (1,341 of 1,795 times). This happens
           in EXO and JDG.
JOS 21:5   The rest of Kohath’s descendants…
           [’] appears only 2 times in this project; it writes ['] elsewhere (6,292 times).
           It is used in LEV and JOS. [’] and ['] look alike.
JOB 3:8    Those who curse the day–may they curse it…
           [–] appears only here in this project; it writes [-] elsewhere (843 times).
           It is used nowhere else. [–] and [-] look alike.
JOB 12:23  …and he also destroys them; \q2 He enlarges nations…
           “He” is capitalized here; this project writes “he” (6,889 times).
           Of its 6,893 uses in the middle of a sentence, this form appears 4 times, in 3 of 66
           books. After [;], the next word is lowercase 4,367 of 4,891 times.
EXO 3:14   God said to Moses, "I AM THAT I AM."
           “THAT” is written in all capitals here; this project writes “that” (8,593 times).
           Of its 8,594 uses in the middle of a sentence, this form appears only here.
JOB 41:15  …which are a terror? \q \v 15 his back is made up of rows of shields
           “his” is lowercase after [?] here; this project usually writes “His” there.
           After [?], this project capitalizes the next word 2,162 of 2,165 times. A lowercase
           word follows it 3 times, in 3 of 66 books.
DEU 27:15  'May the man be cursed who makes a a carved image…
           “a” is written twice in a row here (“a a”).
           The project does this nowhere else; “a” appears 9,280 times.
PSA 81:1   Shout joyfullly to God our strength
           [l] is written 3 times in a row here (“joyfullly”).
           Of the 24,318 places this project repeats [l], this is the only one with 3.
```

## Queries

`describe` also returns `queries`: exact literal needles a consumer may run on
demand through galley's find, `findAll(needle, { caseSensitive, wholeWord,
scope })`, to show every occurrence behind a message. They are rich data;
consumers choose what to show their audience, and none is run for them.

```ts
interface Query {
  purpose: "this" | "alternative" | "others";
  needle: string;
  caseSensitive: boolean;
  wholeWord: boolean;
}
```

| purpose | means | examples |
| --- | --- | --- |
| `this` | the finding's own form, wherever else the project writes it | `;"`, `’`, `On` (case-sensitive, whole word), `a a` (whole word) |
| `alternative` | what the reader might write instead | `";` (also across a straight quote), `'` for `’`, `on`, `His` |
| `others` | the comparison the details name | `'"` (the usual follower), `."'"` (the most common group), `"` (the pool's most common mark) |

- Find searches the projection, the verse text a reader sees, so a needle of
  marks alone (`;"`) is found like any other; `caseSensitive: false` is the
  simple lowercase fold.
- A placement or book-rate site that holds only its glyph has no `this`: the
  class it touches is no literal, and the glyph alone would find every use.
- Hygiene, presence, source-copy and length messages carry none.

## Naming the books

When the count sits in one or two books, `namedBooks` is how many are named
and `book1`, `book2` name them through `context.bookName`: "all in ISA", "in
NUM and JER". A larger spread is counted: "in 5 of 66 books". One book is the
finding's own. Two need `context.patternBooks`, which `booksByPattern`
builds from one pass over the snapshot's findings; without it a two-book
spread is counted. `namedBooks` is 0 whenever the books are counted.

## The mark before a word

A Casing message can say what this project does after the mark in front of
the word, from the publication's terminal section (`codec/README.md`): "After
`;`, the next word is lowercase 4,367 of 4,891 times." The consumer supplies
the mark, since only it holds the text, and the snapshot to look it up in.

`markBefore(text, at)` walks back from `at` by the engine's ride rule
(`substrate::ride_of`): white space is skipped, a quotation mark rides and
sets `quoted`, a closing bracket rides and sets `bracketed`, an opening
bracket rides and marks nothing, and the first other mark is the glyph. A
letter or digit first means no mark, and the sentence is left out. `text` is
verse text with markers removed, as the engine read it: a raw `\q2` between
`;` and `He` would read as the digit `2`. `hasBefore` is false when no mark
was given or the corpus never handed a cased letter off from that mark in
that context. The section carries raw counts only: whether a context forces a
capital, frees one, or is mixed is the judge's call
(`terminal_upper_share_bp`, `terminal_lower_share_bp`), and no message
restates it. SentenceStart needs none of this: its row is already the count
after its own mark, bare.

## The four questions

Every squiggle answers them, and a message that cannot is a kitchen gap rather
than a wording problem.

1. **What was seen.** The glyph or word, and what it touches.
2. **How often.** The count against the total it is measured against, named in
   the glyph's own terms: "of 54,723 commas", never "of sites".
3. **What is normal instead.** The usual alternative and its count.
4. **How widespread.** Books with it, named when there are few.

The headline answers 1 and 2, and 3 when the alternative is one the reader
might write; the details answer the rest.

## Rules

- **The glyph's point of view, touching.** The claim is adjacency, so the
  catalog says "right before" or "right after". The id names the glyph's
  relation: `…follows` means the neighbour is before the glyph, `…precedes`
  that it is after.
- **One headline.** A site may match several rows, and the finding names only
  its finest. `describe` reads that one pattern; never list every reason bit.
- **Counts, not percents, for a doubling.** "The project does this nowhere
  else; “Moses” appears 895 times", not "0.11%".
- **Silence is not "all clear".** The checks find a minority against a
  majority. With no majority there is nothing to measure against, so an empty
  list means no habit was found, not that the text is consistent. Never render
  it as a pass.
- **A genuine minority still needs the why.** EXO 38:26's em dash before a
  digit is correct, rare style. Its value is a message plain enough to dismiss
  in a second.
- **Never** "error", "wrong", "incorrect", or "unconventional", and none of the
  engine's words: channel, run shape, placement, nonletter, band, basis points,
  sites.

## Parameters

- Numbers are raw: counts are integers, `rate` and `baseline` are fractions of
  one for a `percent` format. Format them with the reader's locale.
- Enums are `select` keys and never engine names: a class is
  `letter | space | digit | punctuation`, a case form
  `lowercase | capitalized | allCaps | mixed`.
- Booleans are `select` keys too (`true`, `other`).
- `siteText` is required for a word, cluster, or doubling: the pattern keys a
  word by hash, so the word comes from the text. Without it those params are
  empty strings.
- `bookTotal` is `context.bookCount`; `book` is `context.bookName(index)`, or
  the book's 1-based position when no namer is given.
- Every convention id with a spread carries `count` `total` `books`
  `bookTotal` `namedBooks` `book1` `book2`.

Questions below: **1** seen, **2** how often, **3** normal instead,
**4** how widespread.

| id | means | params (question) |
| --- | --- | --- |
| `convention.placement.follows` | the glyph touches a class on its left that it rarely touches | `glyph` `digit` `neighbor` (1); `count` `total` (2); `usual` `usualCount` (3); spread (4) |
| `convention.placement.precedes` | the same on its right | as above |
| `convention.bookRate.follows` | one book puts the glyph after a class far more often than the others | `glyph` `digit` `neighbor` (1); `count` `total` `rate` `book` (2); `baseline` `otherBooks` (3, 4) |
| `convention.bookRate.precedes` | the same on its right | as above |
| `convention.exactNeighbor` | the glyph is directly followed by a mark it rarely precedes | `glyph` `neighbor` `pair` (1); `count` `total` (2); `usual` `usualCount`, `reversed` `reversedPair` (3); spread (4) |
| `convention.exactNeighbor.swapped` | the same pair, which the project writes the other way round at least as often and at least 5 times; never when either mark is a directionless quote (`"`, `'`), which cannot say whether it opens or closes | as above; `reversedPair` `reversed` are the headline's alternative |
| `convention.pooledNeighbor` | the glyph is directly followed by a kind of mark it rarely precedes (off by default) | `glyph` `pool` (1); `count` `total` (2); spread (4) |
| `convention.runShape` | the glyph sits in a group of marks the project rarely writes | `cluster` `glyph` `clusterCount` (1); `count` `total` (2); `hasUsualCluster` `usualCluster` `usualClusterCount` `reordered` `swap` `usualSize` `usualAtLeast` `usualSameMark` `usualShapeCount` (3); spread (4). `reordered`: the usual cluster holds the same marks; `swap`: and no directionless quote, so the headline names it |
| `convention.rarity` | a character the project almost never uses | `glyph` (1); `count` (2); `hasUsual` `usual` `usualCount` `lookalike` (3); spread (4). `lookalike` is true when Unicode's `confusables.txt` draws the two alike (`’`/`'`, `“`/`"`, `–`/`-`; not `—`/`–`), and the headline names it; else the details name the pool's most common mark |
| `convention.casing` | a word in a case form the project rarely gives it mid-sentence | `word` `form` (1); `count` `total` (2); `usualForm` `usualWord` `usualCount`, and `hasBefore` `before` `beforeContext` `beforeLower` `beforeCased` from `context.before` (3); spread (4) |
| `convention.wordLength` | a word far longer than the project's usual word (off by default) | `word` (1); `count` (2) |
| `convention.doubled.bare` | a word written twice with only a space between | `word` `text` (1); `count` `total` (2); spread (4) |
| `convention.doubled.separated` | a word written twice with punctuation between | as above |
| `convention.letterRun` | one letter repeated more times than the project ever repeats it | `letter` `length` `atLeast` `run` `word` (1); `count` `total` (2) |
| `convention.sentenceStart` | a lowercase word after a mark the project almost always capitalizes after | `glyph` `word` (1); `count` `total` (2); `usualWord` `upper` (3); spread (4) |
| `hygiene` | a character that does not belong in text | `class` (1); `run` `atLeast` (2) |
| `presence.missing` | source verses this book lacks | `keys` `atLeast` (1, 2) |
| `presence.extra` | verses the source lacks | as above |
| `presence.empty` | verses empty here but not in the source | as above |
| `sourceCopy` | words copied exactly from the source verse | `run` (1); `eligible` (2) |
| `length.long` | a verse much longer than usual against its source | `deviation` (2); `inBook` (4) |
| `length.short` | a verse much shorter than usual against its source | as above |

Enum values: `beforeContext` `bare | quoted | bracketed | both`; `neighbor`
and `usual` on placement and book rate `letter | space | digit | punctuation`;
`form` and `usualForm` `lowercase | capitalized | allCaps | mixed`
(`usualWord` is empty for `mixed`); every `…Kind`
`quote | bracket | dash | sentenceEnd | separator | digit | symbol | letter |
space | other`; `pool` the same without `letter` and `space`;
`class` `control | delete | replacement | carriageReturn | backslash | conflict |
combiningMark | format | noBreakSpace | noncharacter`. `digit` is true on the
pooled digit row, whose `glyph` is the site's own digit.

`galley/tests/sous_conformance.mjs` describes every finding of the golden
publications and checks each id has both tiers and well-formed queries;
`sous_messages.rs` checks the id list, that every entry is exactly a headline
and details, every argument of either tier against `describe`, that tags
balance within every branch, that `<g>` is the only tag and holds every
`Glyph` parameter and nothing else, the forbidden words in both tiers, and the
rule internals no headline may name.
