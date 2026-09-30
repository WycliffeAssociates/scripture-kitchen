# Sous messages

Why every squiggle fired, as data. `describe` in
[`sous-messages.ts`](sous-messages.ts) turns a finding into one message id and
its parameters; [`sous-messages.en.json`](sous-messages.en.json) is the English
reference catalog in ICU MessageFormat. The consumer owns the final strings,
adds languages, and formats every number with its own `Intl`. Kitchen ships no
translation runtime.

```ts
import { FindingsSnapshot } from "@wycliffeassociates/scripture-kitchen/sous-reader";
import { describe } from "@wycliffeassociates/scripture-kitchen/sous-messages";
import catalog from "@wycliffeassociates/scripture-kitchen/sous-messages.en.json";

const snapshot = FindingsSnapshot.open(galley.publish());
const finding = snapshot.book("DEU").at(0);
const pattern = finding.kind === "Convention" ? snapshot.pattern(finding.convention.pattern) : undefined;
const { id, params } = describe(finding, pattern, {
  siteText: text.slice(finding.from, finding.to), // the reader has only offsets
  bookCount: snapshot.length,
  bookName: (index) => snapshot.book(index)!.key,
});
new IntlMessageFormat(catalog[id], locale).format({ ...params, g: (chunks) => chunks.join("") });
```

## Glyph tags

A mark in quotation marks cannot be read when the mark is one: `“"” is
followed directly by “(”` asks the reader to find three quotes in `“"”`. So
the catalog never quotes a mark, a pair of marks or a cluster. It wraps each
one in a `<g>` rich-text tag, and `intl-messageformat` hands the tag's
contents to a function the consumer supplies:

```text
catalog   <g>{glyph}</g> is followed directly by <g>{neighbor}</g> here (<g>{pair}</g>).
params    { glyph: "\"", neighbor: "(", pair: "\"(" }
html      g: (chunks) => `<kbd>${chunks.join("")}</kbd>`
          <kbd>"</kbd> is followed directly by <kbd>(</kbd> here (<kbd>"(</kbd>).
plain     g: (chunks) => chunks.join("")
          " is followed directly by ( here ("().
```

- `g` is required: `intl-messageformat` throws when a tag has no function.
  React callers return an element (`<kbd key=…>`) and get an array back.
- A plain-text context with room for a delimiter should keep one the mark
  cannot be; every example in this file renders `<g>` as `[…]`.
- Only marks take the tag: every parameter typed `Glyph` in `ParamsById`
  (`glyph`, `neighbor`, `pair`, `reversedPair`, a mark's `usual`, `cluster`,
  `usualCluster`, `letter`, `run`) and the literal `�` of a hygiene message.
  A letter repeated by `convention.letterRun` is a glyph too.
- Words keep typographic quotes, since a word is never one: “Moses”,
  “Kohath's”. There is no word tag.
- `tests/sous_messages.rs` checks that tags balance within every branch, that
  `<g>` is the only tag, that every `Glyph` parameter appears inside one, and
  that nothing else does.

What en_ulb renders, verse by verse, with `<g>` as `[…]`:

```text
DEU 7:17   …how can I dispossess them?'— \v 18 do not be afraid…
           ['] is followed directly by [—] here (['—]). When another mark follows ['],
           it is usually ["] (416 of 496 times). ['—] appears 6 times, in 5 of 66 books.
GEN 48:20  …like Ephraim and like Manasseh'." \m In this way…
           ['] is followed directly by [.] here (['.]). When another mark follows ['],
           it is usually ["] (416 of 496 times). ['.] appears 3 times, in 3 of 66 books.
           (No swap claim: a straight quote does not say whether it opens or closes.)
EXO 38:26  …those twenty years old and older—603,550 men in all.
           [—] comes right before a digit here, with nothing between. Usually [—] is followed
           right away by a letter (1,341 of 1,795 times). This happens 2 times, in 2 of 66 books.
ISA 43:6   …I will say to the north, 'Hand them over;'
           [;'] is a group of 2 marks, [;] among them, and this exact group appears 3 times.
           This project usually writes [';] (7 times). [;] usually stands alone (4,878 of
           4,904 times). Unusual groups of this size holding [;] appear 5 times, in 1 of 66
           books.
JER 22:16  …is this not what it means to know me? \q2 —this is Yahweh's declaration.
           [—] comes right after a space here, with nothing between. In JER this happens 86 of
           371 times (23%); in most of the other 50 books that use [—], it never happens.
GEN 22:11  …and said, "Abraham, Abraham!"
           “Abraham” is written twice with only punctuation between here (“Abraham, Abraham”).
           The project does this nowhere else; “Abraham” appears 240 times.
EXO 3:14   God said to Moses, "I AM THAT I AM."
           “THAT” is written in all capitals here. In the middle of a sentence this project
           writes it “that” 8,594 of 8,595 times; this form appears only here.
JOB 41:15  …which are a terror? \q \v 15 his back is made up of rows of shields
           After [?], this project capitalizes the next word 2,162 of 2,165 times. Here “his”
           is lowercase.
JOS 21:5   The rest of Kohath’s descendants…
           [’] appears only 2 times, in 2 of 66 books, in this whole project. This project
           writes ['] elsewhere (6,292 times).
PSA 81:1   Shout joyfullly to God our strength
           [l] is written 3 times in a row here (“joyfullly”). Of the 24,318 places this
           project repeats [l], this is the only one with 3.
MAT 17:21  (against a source that lacks the verse)
           This book has a verse here that the source text does not have.
```

## The four questions

Every squiggle answers them, and a message that cannot is a kitchen gap rather
than a wording problem.

1. **What was seen.** The glyph or word, and what it touches.
2. **How often.** The count against the total it is measured against, named in
   the glyph's own terms: "of 54,723 commas", never "of sites".
3. **What is normal instead.** The usual alternative and its count.
4. **How widespread.** Books with it, of the books in the project.

## Rules

- **The glyph's point of view, touching.** "Comes before a digit" reads as
  anywhere earlier in the sentence; the claim is adjacency, so the catalog
  always says "right before" or "right after", "with nothing between". The id
  names the glyph's relation: `…follows` means the neighbour is before the
  glyph, `…precedes` that it is after.
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

## The mark before a word

A Casing message can say what this project does after the mark in front of
the word, from the publication's terminal section (`codec/README.md`):

```text
JOB 12:23  …and he also destroys them; \q2 He enlarges nations…
           … After [;], the next word is lowercase 4,367 of 4,891 times.
```

The consumer supplies the mark, since only it holds the text, and the
snapshot to look it up in:

```ts
describe(finding, pattern, {
  siteText, bookCount: snapshot.length,
  before: markBefore(verseText, siteOffsetInVerseText),
  snapshot,
});
```

`markBefore(text, at)` walks back from `at` by the engine's ride rule
(`substrate::ride_of`): white space is skipped, a quotation mark rides and
sets `quoted`, a closing bracket rides and sets `bracketed`, an opening
bracket rides and marks nothing, and the first other mark is the glyph. A
letter or digit first means no mark, and the sentence is left out. `text` is
verse text with markers removed, as the engine read it: a raw `\q2` between
`;` and `He` would read as the digit `2`. `hasBefore` is false when no mark
was given or the corpus never handed a cased letter off from that mark in
that context. SentenceStart needs none of this: its row is already the count
after its own mark, bare.

Questions below: **1** seen, **2** how often, **3** normal instead,
**4** how widespread.

| id | means | params (question) | en |
| --- | --- | --- | --- |
| `convention.placement.follows` | the glyph touches a class on its left that it rarely touches | `glyph` `digit` `neighbor` (1); `count` `total` (2); `usual` `usualCount` (3); `books` `bookTotal` (4) | [)] comes right after a digit here, with nothing between. Usually a letter comes right before [)] (196 of 273 times). This happens 5 times, in 2 of 66 books. |
| `convention.placement.precedes` | the same on its right | as above | [—] comes right before a digit here, with nothing between. Usually [—] is followed right away by a letter (1,341 of 1,795 times). This happens 2 times, in 2 of 66 books. |
| `convention.bookRate.follows` | one book puts the glyph after a class far more often than the others | `glyph` `digit` `neighbor` (1); `count` `total` `rate` `book` (2); `baseline` `otherBooks` (3, 4) | [—] comes right after a space here, with nothing between. In JER this happens 86 of 371 times (23%); in most of the other 50 books that use [—], it never happens. |
| `convention.bookRate.precedes` | the same on its right | as above | [,] comes right before a punctuation mark here, with nothing between. In MAL this happens 23 of 138 times (17%); in the other 65 books that use [,], it typically happens 0.5% of the time. |
| `convention.exactNeighbor` | the glyph is directly followed by a mark it rarely precedes | `glyph` `neighbor` `pair` (1); `count` `total` (2); `usual` `usualCount` (3); `books` `bookTotal` (4); `reversed` `reversedPair` unused | [?] is followed directly by [,] here ([?,]). When another mark follows [?], it is usually ["] (895 of 1,073 times). [?,] appears only here. |
| `convention.exactNeighbor.swapped` | the same pair, which the project writes the other way round at least as often and at least 5 times; never when either mark is a directionless quote (`"`, `'`), which cannot say whether it opens or closes | `glyph` `neighbor` `pair` (1); `count` (2); `reversedPair` `reversed` (3) | (curly quotes) [”] comes right before [.] here ([”.]). This project writes them the other way round, [.”], 974 times; [”.] appears 3 times. |
| `convention.pooledNeighbor` | the glyph is directly followed by a kind of mark it rarely precedes (off by default) | `glyph` `pool` (1); `count` `total` (2); `books` `bookTotal` (4) | ['] is followed directly by a dash here. Of the 496 times another mark follows ['], this kind of mark follows it 6 times, in 5 of 66 books. |
| `convention.runShape` | the glyph sits in a group of marks the project rarely writes | `cluster` `size` `atLeast` `sameMark` `glyph` `clusterCount` (1); `count` `total` (2); `usualCluster` `usualClusterCount` `hasUsualCluster` `usualSize` `usualAtLeast` `usualSameMark` `usualShapeCount` `usually` (3); `books` `bookTotal` (4) | [;'] is a group of 2 marks, [;] among them, and this exact group appears 3 times. This project usually writes [';] (7 times). [;] usually stands alone (4,878 of 4,904 times). Unusual groups of this size holding [;] appear 5 times, in 1 of 66 books. |
| `convention.rarity` | a character the project almost never uses | `glyph` (1); `count` (2); `hasUsual` `usual` `usualCount` `lookalike` (3); `books` `bookTotal` (4) | [–] appears only once in this whole project. This project writes [-] elsewhere (847 times). With `lookalike` false the usual is only the pool's most common mark: “The most common mark of the same kind is …”. `lookalike` is true when Unicode's `confusables.txt` draws the two alike (`’`/`'`, `“`/`"`, `–`/`-`; not `—`/`–`). |
| `convention.casing` | a word in a case form the project rarely gives it mid-sentence | `word` `form` (1); `count` `total` (2); `usualForm` `usualWord` `usualCount`, and `hasBefore` `before` `beforeContext` `beforeLower` `beforeCased` from `context.before` (3); `books` `bookTotal` (4) | “On” is capitalized here. In the middle of a sentence this project writes it “on” 4,197 of 4,201 times; this form appears 4 times, in 2 of 66 books. |
| `convention.wordLength` | a word far longer than the project's usual word (off by default) | `word` (1); `count` (2) | “uncircumcised” is much longer than most words in this project. It appears 40 times. |
| `convention.doubled.bare` | a word written twice with only a space between | `word` `text` (1); `count` `total` (2) | “a” is written twice in a row here (“a a”). The project does this nowhere else; “a” appears 9,280 times. |
| `convention.doubled.separated` | a word written twice with punctuation between | as above | “Jacob” is written twice with only punctuation between here (“Jacob, Jacob”). The project does this nowhere else; “Jacob” appears 389 times. |
| `convention.letterRun` | one letter repeated more times than the project ever repeats it | `letter` `length` `atLeast` `run` `word` (1); `count` `total` (2) | [l] is written 3 times in a row here (“joyfullly”). Of the 24,318 places this project repeats [l], this is the only one with 3. |
| `convention.sentenceStart` | a lowercase word after a mark the project almost always capitalizes after | `glyph` `word` (1); `count` `total` (2); `upper` (3); `books` `bookTotal` (4) | After [!], this project capitalizes the next word 1,213 of 1,222 times. Here “for” is lowercase. |
| `hygiene` | a character that does not belong in text | `class` (1); `run` `atLeast` (2) | An invisible control character is here (223 in a row). |
| `presence.missing` | source verses this book lacks | `keys` `atLeast` (1, 2) | The source text has 3 verses in a row here that this book does not have. |
| `presence.extra` | verses the source lacks | as above | This book has a verse here that the source text does not have. |
| `presence.empty` | verses empty here but not in the source | as above | This verse is empty here, but the source text has words in it. |
| `sourceCopy` | words copied exactly from the source verse | `run` (1); `eligible` (2) | 3 words in a row here are spelled exactly as in the source text of this verse, out of 27 words that could be compared. |
| `length.long` | a verse much longer than usual against its source | `inBook` (4); `deviation` unused | Compared with the source text, this verse is much longer than the other verses in this book. |
| `length.short` | a verse much shorter than usual against its source | as above | Compared with the source text, this verse is much shorter than the other verses in this project. |

Enum values: `beforeContext` `bare | quoted | bracketed | both`; `neighbor`
and `usual` on placement and book rate
`letter | space | digit | punctuation`; `form` and `usualForm`
`lowercase | capitalized | allCaps | mixed` (`usualWord` is empty for `mixed`);
`pool` `quote | bracket | dash | terminal | separator | digit | symbol | other`;
`class` `control | delete | replacement | carriageReturn | backslash | conflict |
combiningMark | format | noBreakSpace | noncharacter`. `digit` is true on the
pooled digit row, whose `glyph` is the site's own digit.

`galley/tests/sous_conformance.mjs` describes every finding of the golden
publications and checks each id is in the catalog; `sous_messages.rs` checks
the id list and every catalog parameter against `describe`.
