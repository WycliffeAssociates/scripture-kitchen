# Sous messages

Why every squiggle fired, as data. `describe` in
[`sous-messages.ts`](sous-messages.ts) turns a finding into one message id,
its parameters, and the searches behind it;
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
| the same marks in another order | `convention.exactNeighbor.swapped`, `convention.runShape` (`swap`) | `;'` beside `';`; across a straight quote only when it faces a known way (`'.` closing beside `.'` closing) |
| a lookalike | `convention.rarity` (`lookalike`) | `’` beside `'`, from Unicode's `confusables.txt` |
| the same word in another form | `convention.casing` | `On` beside `on` |

A SentenceStart row counts what follows the mark, never the word, so its
headline names the mark's habit and claims nothing about the word: “his” is
lowercase after `?` here; this project almost always capitalizes the word
after `?`. Its `usualWord` stays a parameter and an `alternative` query.

Otherwise the headline says only how rare it is, and the usual follower, the
usual class, or the most common group moves to the details.

A straight `"` or `'` does not say whether it opens or closes, so the engine
reads its facing from what stands either side of the run and counts a pair
reversed only among runs facing the same way. Under `opening` or `closing`
the reordering reaches the headline, and the headline names the facing:

```text
params    { pair: "'.", reversedPair: ".'", reversed: 974, facing: "closing",
            glyphKind: "quote", neighborKind: "sentenceEnd", neighborName: "full stop", … }
en        Here the full stop comes after the closing quote (['.]); this project puts it
          before the closing quote ([.']) 974 times.
```

Under `inside` or `unknown` it stays a plain fact in the details ("This
project also writes `."`, once"). A group reorders in the headline only when
the site's group and the usual one share a known facing.

## Glyph tags

A mark in quotation marks cannot be read when the mark is one: `“"” is
followed directly by “(”` asks the reader to find three quotes in `“"”`. So
the catalog never quotes a mark, a pair of marks or a cluster. It wraps each
one in a `<g>` rich-text tag, and `intl-messageformat` hands the tag's
contents to a function the consumer supplies:

```text
catalog   <g>{cluster}</g> appears only here.
params    { cluster: "\":" }
html      g: (chunks) => `<kbd>${chunks.join("")}</kbd>`
          <kbd>":</kbd> appears only here.
plain     g: (chunks) => chunks.join("")
          ": appears only here.
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
| `symbol` | `S*` | `©` `+` `$` `^` |
| `space` | `White_Space` | |
| `letter` | `Alphabetic` | `a` `ñ` |
| `other` | anything else | `*` `/` `&` `#` `§` |

The first five are the engine's own neighbour pools, first match wins, so
`«` is a quote and `−` a dash; the engine pools a letter or a space as
`other`. The table is generated into [`sous-unicode.ts`](sous-unicode.ts)
(`kindOf(scalar)`, `rangesOf(kind)`, `CLOSERS`) by
`cargo run -p sous-core --bin gen-unicode`, and a test fails when it is stale.
Kinds are `select` keys, so a translator words every one; a catalog `select`
over a kind names all ten (`sous_messages.rs` checks), `other` included.

## Unicode names and code points

Beside every `…Kind` a `Glyph` parameter also carries `…Name`, its Unicode
name lowercased for prose, and `…Code`, its code point. A lookalike rarity
names both marks, since the two are hard to tell apart on screen:

```text
params    { glyph: "–", glyphName: "en dash", glyphCode: "U+2013",
            usual: "-", usualName: "hyphen-minus", usualCode: "U+002D", named: true, … }
en        [–] en dash (U+2013) and [-] hyphen-minus (U+002D) look alike.
```

- Names come from `UnicodeData.txt` (UCD 17.0.0, pinned beside the other
  extracts) for every punctuation mark and symbol, general category `P*` or
  `S*`, which holds every pooled mark. A letter or a digit has no name here,
  so its `…Name` is empty; `named` says both of a rarity's marks have one.
- [`sous-unicode-names.ts`](sous-unicode-names.ts) is generated with the
  kinds and staleness-tested the same way: `nameOf(scalar)`,
  `codePointOf(scalar)` (`U+` and at least four uppercase hex digits).
- **Names are English only.** Unicode names are English by definition, and
  the catalog shows them untranslated. A consumer that wants localized names
  needs CLDR's character annotations (`common/annotations/*.xml`), which
  kitchen does not ship; such a catalog should leave `…Name` out and keep
  `…Code`.

## What en_ulb renders

`cargo run --release -p sous-cli -- --publish x.sous testData/exampleCorpora/en_ulb`,
headline then details, `<g>` as `[…]`:

```text
NUM 21:14  …the Wars of Yahweh, \q "... Zahab…                 (facing unknown)
           A quotation mark right before a mark that ends a sentence ([".]) appears only 2 times in
           this project.
           ["] is followed directly by another mark 129 times in this project, most often [']
           (77). This project also writes [."], once. [".] appears in NUM and JER.
GEN 48:20  …like Ephraim and like Manasseh'." \m In this way…    (facing closing)
           Here the full stop comes after the closing quote (['.]); this project puts it before
           the closing quote ([.']) 974 times.
           ['] is followed directly by another mark 496 times in this project, most often ["]
           (416). ['.] appears 3 times, in 3 of 66 books.
2KI 13:17  …Then Elisha said, "Shoot!", and he shot…            (facing closing)
           Here the comma comes after the closing quote ([",]); this project puts it before the
           closing quote ([,"]) 226 times.
           ["] is followed directly by another mark 129 times in this project, most often [']
           (77). [",] appears nowhere else.
PRO 30:15  …four that never say, "Enough": \q1 \v 16 Sheol…
           A quotation mark right before a comma or similar mark ([":]) appears only here in this
           project.
           ["] is followed directly by another mark 129 times in this project, most often [']
           (77).
DEU 7:17   …how can I dispossess them?'— \v 18 do not be afraid…
           A quotation mark right before a dash (['—]) appears only 6 times in this project.
           ['] is followed directly by another mark 496 times in this project, most often ["]
           (416). This project also writes [—'], once. ['—] appears in 5 of 66 books.
ISA 30:10  …They say to the seers, "Do not see;" \q2 and to the prophets…
           [;"] appears only 2 times; with a closing quote, this project usually writes [";] (6
           times).
           [;] stands alone 4,878 of 4,904 times. Unusual groups like this appear 5 times, all
           in ISA.
EXO 38:26  …those twenty years old and older—603,550 men in all.
           A dash ([—]) right before a digit appears only 2 times in this project.
           Usually [—] is followed right away by a letter (1,341 of 1,795 times). This happens
           in EXO and JDG.
JOS 21:5   The rest of Kohath’s descendants…
           [’] appears only 2 times in this project; it writes ['] elsewhere (6,292 times).
           It is used in LEV and JOS. [’] right single quotation mark (U+2019) and ['] apostrophe
           (U+0027) look alike.
JOB 3:8    Those who curse the day–may they curse it…
           [–] appears only here in this project; it writes [-] elsewhere (843 times).
           It is used nowhere else. [–] en dash (U+2013) and [-] hyphen-minus (U+002D) look alike.
JOB 12:23  …and he also destroys them; \q2 He enlarges nations…
           “He” is capitalized here; this project writes “he” (6,889 times).
           Of its 6,893 uses in the middle of a sentence, this form appears 4 times, in 3 of 66
           books. After [;], the next word is lowercase 4,367 of 4,891 times.
EXO 3:14   God said to Moses, "I AM THAT I AM."
           “THAT” is written in all capitals here; this project writes “that” (8,593 times).
           Of its 8,594 uses in the middle of a sentence, this form appears only here.
JOB 41:15  …which are a terror? \q \v 15 his back is made up of rows of shields
           “his” is lowercase after [?] here; this project almost always capitalizes the word after
           [?].
           After [?], this project capitalizes the next word 2,162 of 2,165 times. A lowercase
           word follows it 3 times, in 3 of 66 books.
DEU 27:15  'May the man be cursed who makes a a carved image…
           “a” is written twice in a row here (“a a”).
           The project does this nowhere else; “a” appears 9,280 times.
PSA 81:1   Shout joyfullly to God our strength
           [l] is written 3 times in a row here (“joyfullly”).
           Of the 24,318 places this project repeats [l], this is the only one with 3.
```

A mark with a letter on both sides is judged as one joint key, inside a word,
with no usual class; from `corpora/nya.txt`:

```text
ISA 29:16  …Sibanani bumbe ine"pa vintu…
           A quotation mark (["]) inside a word, with letters on both sides, appears only 13
           times in this project.
           Of the 6,805 times this project uses ["], 13 have a letter on both sides. This happens
           in 6 of 62 books.
```

## Queries

`describe` also returns `queries`: searches a consumer may run on demand to
show every occurrence behind a message. They are rich data; consumers choose
what to show their audience, and none is run for them.

```ts
type Query =
  | { kind: "literal"; purpose; needle: string; caseSensitive: boolean; wholeWord: boolean }
  | { kind: "regex"; purpose; source: string; flags: "u" };
// purpose: "this" | "alternative" | "others"
```

```text
JOB 41:15  …which are a terror? \q \v 15 his back…          (SentenceStart, after `?`)
  this         \?[\s\p{Ps}]*his(?![\p{L}\p{M}\p{N}])          1 in en_ulb: this site
  alternative  \?[\s\p{Ps}]*His(?![\p{L}\p{M}\p{N}])          4
  others       \?[\s\p{Ps}]*[\p{Uppercase}\p{Lt}]             2,143
JOB 12:23  …destroys them; \q2 He enlarges nations…          (Casing, `before` is `;`, bare)
  this         ;[\s\p{Ps}]*He(?![\p{L}\p{M}\p{N}])            2
  alternative  ;[\s\p{Ps}]*he(?![\p{L}\p{M}\p{N}])            543
  others       ;[\s\p{Ps}]*\p{Lowercase}                      4,373
GEN 41:45  …priest of On, as a wife.                         (Casing, no mark before)
  this         (?<=(?:^|[\p{L}\p{M}\p{N}])[\s\p{Ps}\p{Pe}\p{Pi}\p{Pf}"'＂＇]*)(?<![\p{L}\p{M}\p{N}])On(?![\p{L}\p{M}\p{N}])
                                                               4
  alternative  the same with on                                4,016
```

JOB 41:15's `this` finds only itself: the corpus's other two lowercase words
after `?` are `you` (JER 49:4) and `says` (ACT 7:49), other words.

| purpose | means | literal | regex |
| --- | --- | --- | --- |
| `this` | the finding's own form, wherever else the project writes it | `;"`, `’`, `a a` (whole word) | `—\p{Nd}` (an em dash right before a digit), the site's word after its mark as written |
| `alternative` | what the reader might write instead | `";` (also across a straight quote), `'` for `’` | the same word after the same mark in the usual form (`His`, `he`) |
| `others` | the comparison the details name | `'"` (the usual follower), `."'"` (the most common group), `"` (the pool's most common mark) | `—\p{Alphabetic}` (the usual class), any word of the other case after the mark (the habit) |

- A literal goes to galley's find, `findAll(needle, { caseSensitive,
  wholeWord, scope })`, which searches the projection, the verse text a
  reader sees, so a needle of marks alone (`;"`) is found like any other;
  `caseSensitive: false` is the simple lowercase fold.
- A regex is for what a literal cannot say. The consumer runs it in
  JavaScript, `new RegExp(source, flags)` (add `g` to iterate), over **verse
  text with markers removed**, a book's or a chapter's at a time: a handoff
  can cross a verse boundary (JOB 41:15's `?` ends verse 14). Sources are
  trusted kitchen output: built only from escaped marks, the site's own word
  with every syntax character escaped, and fixed Unicode classes. Every regex
  is case-sensitive, and a word ends at a Unicode lookahead,
  `(?![\p{L}\p{M}\p{N}])`, since `\b` is ASCII-only even with `u`.
- Where regexes appear:
  - **Placement and book rate**: the glyph beside the class it touches, and
    beside its usual class as `others`: `—\p{Nd}`, `\s—`, `,\p{Alphabetic}`,
    `)[^\p{Alphabetic}\p{Nd}\s]`. The pooled digit lane's glyph is `\p{Nd}`.
    A site that holds its neighbour as well keeps its literal `this` (`),`).
    A mark inside a word has only `this`, the glyph between two letters:
    `\p{L}"\p{L}`.
  - **SentenceStart**: the mark, bare riders, then the site's word as
    `this`, capitalized as `alternative`, and any capital as `others`. The row
    counts bare handoffs only, so between the mark and the word stand only
    white space and opening brackets; a quote or a closing bracket there is
    another context the row never counts (`?" he said`).
  - **Casing** with `context.before`: the mark, the riders of its context
    (`bare`, `quoted`, `bracketed`, `both`), then the site's word as `this`,
    the usual form's spelling as `alternative` (none for `mixed`), and any
    word of the other case as `others`. Without a mark (`markBefore` found a
    word first) the word must follow a word through riders, or open the text,
    as `this` and `alternative`; there is no `others`. Pass `before` whenever
    the site follows a mark, or `this` will not find its own site.
  - A bare word as a literal is never a query: `his` alone finds every `his`
    in the project, not the finding's case.
- The riders are Unicode classes close to the engine's ride rule: an opening
  bracket is `\p{Ps}`, a quote `\p{Pi}\p{Pf}` or a directionless `"'＂＇`, a
  closing bracket `\p{Pe}`. A regex is a lookup aid over verse text, and may
  differ slightly from the engine's pool-based classes in `sous-unicode.ts`:
  `「` and `„` are `Ps` but pool as quotes, and `⸂` is `Pi` but pools as a
  bracket.
- A placement or book-rate site that holds only its glyph has no literal
  `this`: the class it touches is no literal, and the glyph alone would find
  every use. Its regex says it.
- Hygiene, presence, source-copy and length messages carry none.

## Stable identity

**A persisted contract.** A consumer saves suppressions ("this is fine") to
disk, so it needs a name for a finding that survives reloads, republishes,
edits elsewhere in the project, and settings changes.
[`sous-identity.ts`](sous-identity.ts) builds three, as pure functions over a
decoded row and context only the consumer holds:

```ts
import { identityOf, ordinalOf, patternIdentity, siteIdentityOf } from "@wycliffeassociates/scripture-kitchen/sous-identity";

const siteText = text.slice(finding.from, finding.to);
const ordinal = ordinalOf(verseText, siteText, siteInVerse);   // earlier identical starts in the verse
const site = { verseRef: "EXO 38:26", siteText, ordinal };
const bookKey = (index) => snapshot.book(index)!.key;          // BookRate only
patternIdentity(pattern, { bookKey })      // v1:p:Placement:2014:next:Digit
identityOf(finding, pattern, { ...site, bookKey })
                                           // v1:f:p:Placement:2014:next:Digit:EXO 38%3A26:—:0
siteIdentityOf(site)                       // v1:s:EXO 38%3A26:—:0
```

| identity | means | fields after `v1` |
| --- | --- | --- |
| `v1:p:…` | this claim, anywhere | `p`, the channel, the glyph as hex (`digit` for the pooled digit lane) or a word's 16-hex hash, then the key: ExactNeighbor the neighbour's hex; PooledNeighbor the pool; RunShape `pure`/`mixed` and the size bucket; Placement the side (`prev`, `next`, or `both` inside a word) and class; LetterRun the length; Casing the form; Doubled `bare`/`separated`; BookRate the side, class and book key; Rarity, SentenceStart, WordLength nothing more |
| `v1:f:…` | this finding here | `f`, the pattern's fields from `p` on (or `hygiene` and its class, `presence` and its kind, `sourceCopy`, `length`), then the verse, the site's text and the ordinal |
| `v1:s:…` | this text here, whatever flags it | `s`, the verse, the site's text and the ordinal |

- **Never in it:** the pattern's index, its band, counts, shares, a
  WordLength's deviation, a book's position, or anything else a setting or an
  edit elsewhere moves. The pattern index and the book position change with
  every republish; the numbers change with every edit.
- **Why a site identity too:** a settings change can move which rule
  headlines a site (a Placement row fires where a RunShape row did), which
  changes `v1:f`. Save `v1:s` for "this text is fine, whatever flags it", and
  `v1:f` for "this rule is wrong here".
- **Consumer context.** `verseRef` is the verse as the consumer names it
  (`GEN 1:2`), `siteText` the finding's span sliced from the book's text (the
  same string `describe` takes), and `ordinal` how many identical `siteText`s
  start earlier in that verse, 0-based. `ordinalOf(verseText, siteText, at)`
  counts them, overlapping starts included. Whatever text the consumer counts
  in, it must count the same way every time: the ordinal is saved.
- **Escaping.** Fields are `:`-separated; `%`, `:`, and the C0 controls and
  DEL inside a field become `%XX` (`GEN 1%3A2`). The verse, site text and
  ordinal are always the last three fields.
- **Changing it requires a migration.** The `v1` prefix names the format.
  Any change to what goes in, its order, its spelling or its escaping is a new
  version, and every saved identity needs migrating. A word's hash is the
  engine's wire hash, so an engine change to that hash is a format change
  too. `sous-chef/messages.test.mjs` pins the identities of every finding of
  the committed golden publications in `galley/tests/fixtures/sous/identities.txt`
  and fails loudly when they move.

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
| `convention.placement.inside` | the glyph has a letter on both sides, inside a word, which the project rarely writes | `glyph` `digit` (1); `count` `total` (2); spread (4); no usual: the rest of `total` is it |
| `convention.bookRate.follows` | one book puts the glyph after a class far more often than the others | `glyph` `digit` `neighbor` (1); `count` `total` `rate` `book` (2); `baseline` `otherBooks` (3, 4) |
| `convention.bookRate.precedes` | the same on its right | as above |
| `convention.exactNeighbor` | the glyph is directly followed by a mark it rarely precedes | `glyph` `neighbor` `pair` `facing` (1); `count` `total` (2); `usual` `usualCount`, `reversed` `reversedPair` (3); spread (4). `facing`: which way a straight quote in the pair faces, `none` without one; `reversed` then counts only runs facing the same way |
| `convention.exactNeighbor.swapped` | the same pair, which the project writes the other way round at least as often and at least 5 times; with a straight quote (`"`, `'`) only when it faces `opening` or `closing`, and the headline names the facing | as above; `reversedPair` `reversed` are the headline's alternative |
| `convention.pooledNeighbor` | the glyph is directly followed by a kind of mark it rarely precedes (off by default) | `glyph` `pool` (1); `count` `total` (2); spread (4) |
| `convention.runShape` | the glyph sits in a group of marks the project rarely writes | `cluster` `glyph` `clusterCount` (1); `count` `total` (2); `hasUsualCluster` `usualCluster` `usualClusterCount` `reordered` `swap` `usualSize` `usualAtLeast` `usualSameMark` `usualShapeCount` `facing` (3); spread (4). `reordered`: the usual cluster holds the same marks; `swap`: and no straight quote, or the two groups share a known facing, so the headline names it; `facing` is the usual cluster's |
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

Enum values: `beforeContext` `bare | quoted | bracketed | both`; `facing`
`opening | closing | inside | unknown | none`; `neighbor`
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
