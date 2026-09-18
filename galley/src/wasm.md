# The JS doorway

```js
import { Galley } from "usfm-galley";
import { FindingsSnapshot } from "usfm-galley/sous-reader";
import { Census } from "usfm-galley/toc-reader";

const galley = new Galley();                            // one opaque handle

galley.update("books/MRK.usfm", text);                  // whole book → "MRK"
galley.updateReference("ref/en_ult/GEN.usfm", ult);     // lengths only, no text
galley.updateReference("ref/en_ult/RUT.usfm", ult, true);  // …and searchable

const census = Census.open(galley.tocAll());            // every book's chapters and verses
const snap = FindingsSnapshot.open(galley.publish());   // one complete snapshot
snap.findingsFor("books/MRK.usfm");                     // by id, via the string table
snap.patterns();                                        // what each row means

const settings = galley.config();                          // a copy
settings.casing = false;
galley.setConfig(settings);                                // re-judge, no re-map

galley.remove("books/MRK.usfm");
galley.residentBytes();                                 // Pantry + caches
```

Find runs over the retained projection, and answers in one buffer:

```js
galley.find("books/MRK.usfm", "wept. Then", { caseSensitive: true });  // one book
galley.findAll("God", { wholeWord: true, limit: 200 });                // every target
galley.findAll("God", { wholeWord: true, limit: 200, scope: "all" });  // and the sources
```

## Two doors, one engine: by id, or by text

The onion products are on the same handle and read the same warm chunks. The
ID doors run off the RETAINED copy — nothing crosses the wall but the id:

```js
deserialize(galley.parse("books/MRK.usfm", true, true, true));  // the plated book
deserialize(galley.lint("books/MRK.usfm"));                     // diagnostics alone
galley.verseText("books/MRK.usfm");                             // off the retained mask
```

Use the TEXT doors for text the host has not registered — a preview pane, a
file not yet in the project:

```js
deserialize(galley.parseText(text, true, true, true));
galley.verseTextOf(text);
galley.structureTextOf(text);                 // no book retains a structure mask
galley.maskOf(text, { recipe: "structure" }); // and the map of the same cut
```

The chunk cache keys on CONTENT, so a text door over a registered book's own
bytes hits the same products; what it costs is the string crossing the wall.

`lint` is not a door onion itself has: a lint report crosses as the
`diagnostics` section of a parse buffer, so `lint(id)` is `parse(id, true,
false, false)` and `reader.ts` reads it the same way.

A book that retains no text refuses the text-needing doors by name rather than
answering from nothing — `updateReference` registers `ProductsOnly` unless the
host asks otherwise, so:

```js
galley.lint("ref/RUT.usfm");        // throws: book ref/RUT.usfm retains no text
galley.verseText("ref/RUT.usfm");   // throws: retains no verse-text projection
galley.parse("books/NUM.usfm", …);  // throws: no book is registered as books/NUM.usfm
```

## A searchable source: `updateReference(id, text, keepText)`

The third argument — omitted is `false` — makes a reference keep its text AND
build the projection a target builds, so every door above answers on it and
`find` can search it. It costs what a target costs minus the resident analysis
(`pantry.md`: 7.29 MB per Bible against 1.17 MB of lengths alone), so it is the
host's call per source, not a default:

```js
galley.updateReference("ref/RUT.usfm", ult, true);
galley.find("ref/RUT.usfm", "Naomi", { caseSensitive: true });   // the source, searched
galley.verseText("ref/RUT.usfm");                       // and its projection
```

Re-sending a reference with a different `keepText` is a real update, not a
no-op: `Retain` is part of what the Pantry serves a book from, the same way
`SourceLanes` is.

## Dirty and rework

Two questions, two answers, and a `Fingerprint` is ~1 KB of chunk starts and
checksums with no text in it — the baseline user land keeps beside a file on
disk:

```js
const baseline = galley.fingerprint(diskText);      // a HANDLE: free() it
const current = galley.fingerprint(editorText);
baseline.differsFrom(current);                      // dirty — POSITIONAL
baseline.changedChunks(current);                    // rework — SET MEMBERSHIP
baseline.free();

galley.changedSinceUpdate("books/MRK.usfm", editorText);  // vs the retained copy
```

`changedChunks` and `changedSinceUpdate` answer one `Uint32Array` of `from,
to` byte pairs in the CANDIDATE text — the chunks a host would have to
re-derive. A chapter that only MOVED keeps its checksum, so it is dirty and
needs no rework. `changedSinceUpdate` answers `undefined` for an id that is
not registered, which is the question a host asks before registering it.

## Every onion door, on the same module

One import, both engines. These are `onion-wasm`'s own exports, under their own
names and signatures — the same shims its package ships, not wrappers:

```js
import { parse, mask, format, formatEdits, formatEditsIn, FormatOpts, Edits,
         diff, merge, mergeSplices, Splices, toByte, toUtf16, locate, attrs,
         attrResolve, book } from "usfm-galley";
```

`galley`'s cdylib links the object those shims sit in — `galley::wasm::onion`
names one symbol out of it, and because `onion-wasm` is one module the rest
arrive with it. Without that reference the module would export `Galley` and
`SousSettings` and nothing else, silently: a cdylib keeps a dependency's exports only
where something wants them. `tests/sous_conformance.mjs` therefore asserts the
EXACT export list, not a subset, and prints it:

```text
exports (20): Edits Fingerprint FormatOpts Galley SousSettings Splices attrResolve attrs
              book diff format formatEdits formatEditsIn locate mask merge
              mergeSplices parse toByte toUtf16
```

The doors cost 202 KB of `.wasm` (744,333 → 946,665 bytes, release, wasm-opt
`-O`), which is the engine's write path — the formatter, the differ, the
attribute interpreter — none of which the analysis path pulls in on its own.

A Rust caller across the wall reaches the same doors as
`usfm_galley::wasm::onion::to_utf16`, which is what `tests/wasm_wall.rs` runs.

## The claim

The bytes `publish()` returns in JavaScript are the bytes
`Expediter::<Brigade>::publish` returns natively — the same publication, not an
equivalent one. Three tests hold that:

| where | what it drives | against |
| --- | --- | --- |
| `tests/sous_goldens.rs` | the native `Expediter` | `tests/goldens/sous/*.bin` |
| `tests/wasm_wall.rs` | the `Galley` handle, in Node | the same three files |
| `tests/sous_conformance.mjs` | `pkg-node` plus `galley/sous-reader.ts` | the same three files |

The fixtures under `tests/fixtures/sous/` are written so the published bytes
carry a row of every wire code — hygiene, convention, and length — so a wall
that silently dropped a lane could not pass.

## Runbook

```
cd galley
wasm-pack test --node --features wasm --test wasm_wall
wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
node tests/sous_conformance.mjs pkg-node [corpus-dir]
./build.sh                      # the two COMMITTED packages; galley/README.md
```

`--features` is a cargo argument, so it rides after `--` on `build` and before
it on `test`. With a corpus directory the last command also prints the
keystroke lifecycle — marshal, update, publish, read — step by step.

`cargo check --target wasm32-unknown-unknown -p usfm_galley --features wasm` is
the cheap gate; nothing on the publish path touches clocks, randomness, or
threads, and `rayon` is behind `parallel`, which a wasm build never asks for.

## The find buffer

`find` and `findAll` answer with one buffer and its strings. Both doors encode
the same layout (`galley::find::wire`), which is also what the native
`Expediter::find` returns, so a host that reads one reads both:

```js
import { Hits } from "usfm-galley/find-reader";

const hits = Hits.open(galley.findAll("God", { wholeWord: true, limit: 200 }));
for (let n = 0; n < hits.hitCount; n++) {
  const hit = hits.hit(n);
  hits.id(hit.bookIndex);                  // which book
  hit.projectedFrom, hit.projectedTo;      // what to highlight
  const pieces = hit.pieces();             // where an edit lands, one per piece
  hits.preview(n);                         // the result card's line
}
```

**Read it through the reader, never by hand.** `find-reader.ts` is GENERATED
from the same declaration the writer is (`galley/src/find/wire/schema.rs`), so
the two ends cannot disagree and no consumer learns a layout. `Hits.open`
validates the magic and the version and throws naming both — a consumer a
version behind fails at its first call instead of misreading a field. The
layout itself is in `galley/src/find/wire/mod.rs`; it is written down to be
reviewed, not to be implemented twice.

The reader is lazy where laziness pays: the hit rows are walked once at open,
because a hit's length is a value it carries, and the id and preview strings
are located only when something asks for one.

Every offset is **UTF-16** — the unit the editor's coordinates are already in,
the same choice `parse(text, …, utf16: true)` makes.

Two coordinate spaces per hit, because they are not the same interval
(`find.md`): `projectedFrom..projectedTo` is in the projection — what a reader
sees and what a highlighter wants — and the source pieces are where an edit has
to land. **A hit crossing a masked gap comes back as one source range per
contiguous piece**, in order; the bytes between two pieces are exactly the
markup the projection dropped, and whether that markup survives a replacement
is the caller's decision to make, never Find's.

`bookIndex` indexes the buffer's own id table, which names every book searched
whether or not it matched — so `findAll`'s answer needs no second call, and a
hit's book cannot be misattributed by a corpus that changed in between.

Which books those are is `findAll`'s `scope` option:
`"targets"` (the default when omitted), `"references"`, or `"all"`, which
searches the targets and then the references. A reference registered without
`keepText` is in no scope — it retains nothing to search, so it is neither
searched nor listed. An unknown scope throws rather than falling back.

The preview is the projected text around the hit, ellipsed and trimmed for a
result card. It rides in the buffer because the projection is materialized for
the search and dropped with it: a host that wanted the same string afterwards
would have to mask the whole book again. Display text only — never read an
offset back out of it.

`limit` bounds hits across the whole call rather than per book; `0` (the
default) means no bound. `find` searches any registered book that retains text
and a projection — a target, or a reference registered with `keepText` — and
errors on one that retains neither, naming the argument that would fix it
(`reference ref/RUT.usfm retains no text; register it with keepText`), because
"no hits" would be a lie; `findAll` over an empty scope is an empty buffer, not
an error. The needle is a LITERAL — `caseSensitive` off is the simple lowercase
fold and `wholeWord` is the words rule restated in `find.md`; there is no
regex here and the `regex` crate is not a dependency.

## The mask map

`mask(id, opts?)` and `maskOf(text, opts?)` answer with the map the projection
is made of — which source spans it concatenates, in order:

```js
import { MaskMap } from "usfm-galley/mask-reader";

const map = MaskMap.open(galley.mask("books/MRK.usfm", { utf16: true }));
let reading = "";
for (let n = 0; n < map.rangeCount; n++) {
  const r = map.range(n);
  reading += source.slice(r.sourceFrom, r.sourceTo);   // === galley.verseText(id)
}
map.pieces(from, to);                    // a projected span, as source spans
```

```ts
interface MaskOptions {
  recipe?: "verseText" | "structure" | "text";   // default "verseText"
  utf16?: boolean;                               // default false
}
```

Each recipe is named for what SURVIVES it: `verseText` text inside verse
extents only, `structure` the paragraph/chapter/verse skeleton, `text` every
text byte anywhere with nothing removed — the cut a diff run's non-markup bytes
are in. `galley/src/mask.md` has the table.

**The projection is a pure concatenation** — nothing is inserted between two
ranges — so a host holding the source rebuilds the reading from the map alone
and maps a projected offset back to the byte an edit lands on. `\add
one\add*two` reads `onetwo`, and that is correct: `\add*` is a delimiter and
the space is the author's to write (`mask.md`).

`mask(id)` with every default is served off the retained projection with no cut
at all. `recipe: "structure"` and `recipe: "text"` cut the retained text, since
no book retains either projection; `maskOf` cuts the text handed in, and under `utf16`
builds a table over it. Over a registered book's own bytes the two doors write
the same buffer.

**Read it through the reader, never by hand.** `mask-reader.ts` is GENERATED
from the same declaration the writer is (`galley/src/mask/schema.rs`);
`MaskMap.open` validates the magic, the version, and that the row block fits
the count, and throws naming both versions. `starts` is not on the wire — the
reader builds the prefix sum in one pass at open — and `toSource` and `pieces`
ride along, `pieces` answering exactly what a find hit's `pieces()` carries.

`sourceLen` says which text the map was cut from, so a consumer whose copy has
moved on learns it before joining slices out of the wrong string;
`projectedLen` is its checksum after the join.

The scope is find's: a target, or a reference registered with `keepText`. One
that retains neither errors by name, naming the argument that would fix it
(`reference ref/RUT.usfm retains no text; register it with keepText`), and an
unknown id errors as `no book is registered as X`. An unknown recipe throws
naming the three that exist rather than falling back to any of them.

## The census buffer

What a project HOLDS, without a parse per book: `toc(id, utf16?)` for one
registered book, `tocAll(scope?, utf16?)` for every book of a scope in one
crossing — which is the call that belongs on a project's open.

```js
import { Census } from "usfm-galley/toc-reader";

const census = Census.open(galley.tocAll());       // "targets" by default
for (const book of census) {
  book.code;          // "GEN"
  book.id;            // the id it was registered under
  book.chapters;      // `\c` markers; `chapterCount` counts the front-matter row too
  book.verseCount;    // `\v` markers — NOT a verse count; see galley/src/toc.md
  const rows = book.chapterRows;
  rows.seek(1).number;    // 1
  rows.seek(1).anchors;   // 31
  rows.seek(1).lastVerse; // 31
}
```

**Read it through the reader, never by hand.** `toc-reader.ts` is GENERATED
from the same declaration the writer is (`galley/src/toc/schema.rs`), so the
two ends cannot disagree and no consumer learns a layout. `Census.open`
validates the magic, the version and both row strides, and throws naming the
mismatch — a consumer a version behind fails at its first call instead of
misreading a field. The layout itself is in `galley/src/toc.md`; it is
documented to be reviewed, not to be implemented twice.

Nothing is derived by either door: a registered book's `Toc` is built by
`update` and pinned, so this reads resident state. Offsets are bytes unless
`utf16` asks otherwise, and a reference registered without `keepText` kept no
table to rebase through — it answers by name rather than handing back bytes
labelled as code units:

```js
galley.tocAll("all", true);   // throws: ref/RUT.usfm retains no UTF-16 table;
                              //         register it with keepText, or ask for byte offsets
```

The scope reaches further than `findAll`'s on purpose: every registered book
has a `Toc`, including a reference that kept no text, so every one of them is
listed.

## The overlay doors

Six methods on the handle, because they read the resident Pantry: `skeleton`,
`overlay`, `overlayText`, `overlayReport`, `targetNodeFor`, `sourceNodeFor`.
A target's block structure made equal to a declared source's — the shape a
verse-only Bible needs to be typeset against a source's paragraphing and
poetry. `galley/src/overlay.md` is the contract; this is the wire.

```js
galley.update("books/GEN.usfm", target);
galley.updateReference("ref/GEN.usfm", source, true);   // keepText: a skeleton needs the text

const skeleton = JSON.parse(galley.skeleton("ref/GEN.usfm"));
//   { verses: [{ sid, from, to, textFrom, textTo }],
//     blocks: [{ sid, where: "leading" | "inside", ordinal, marker, from, to, empty }] }

const edits = galley.overlay("books/GEN.usfm", "ref/GEN.usfm");
//   Edits — spans, lens, text: onion-wasm's own class, the shape a fix crosses in
const report = JSON.parse(galley.overlayReport("books/GEN.usfm", "ref/GEN.usfm"));
//   { inserted, removed, collapsed, unpaired }
galley.overlayText("books/GEN.usfm", "ref/GEN.usfm");   // the same edits, applied

const at = { sid: "GEN 2:23", where: "inside", ordinal: 1, marker: "q1" };
JSON.parse(galley.sourceNodeFor("books/GEN.usfm", "ref/GEN.usfm", at));  // { found: SkeletonRow }
JSON.parse(galley.targetNodeFor("books/GEN.usfm", "ref/GEN.usfm", at));  // { absent: true, insertAt, where }
```

An address is `{ sid, where, ordinal, marker }` and every field is required.
The position is the key; `marker` is the spelling that position held when the
address was taken, checked against the side the address came from. A position
that still exists but now spells something else THROWS (`… names q1 but the
node there is q2 — the address is stale`) instead of answering about a
different node.

Every door takes the same optional `{ markers?, scope?, utf16? }`; `skeleton`
and the two `*NodeFor` doors take `utf16` as a trailing boolean as well, the
way `parse` does. A misspelled key reads as absent, a wrong TYPE throws, and
an unknown marker name throws rather than filtering nothing.

**Edits, not the string, is the door a host wants.** `overlay` returns
`onion-wasm`'s own `Edits` — the class `formatEdits` answers with — so an
editor applies an overlay exactly as it applies a fix: through the document,
as ONE undo step. Highlighting falls out of the edits for free and scope is
natural. `overlayText` is for a caller that only needs the string.

**The one difference is the unit.** `formatEdits` and `formatEditsIn` always
answer in UTF-16; `overlay` answers in BYTES unless `utf16` asks otherwise,
like every other door on the `Galley` handle. `Edits` itself names no unit —
its producer does. `spans` AND `lens` carry the unit that was asked for: `lens`
slices `text` at the offsets `spans` place, so a host that reads `text` as a
JS string under `utf16: true` gets lengths in code units too. Every insert an
overlay writes is ASCII today, where the two counts agree; the conversion is
there so that stays true of a marker that is not.

**JSON for the three read-only doors.** They answer structured rows on a
cold, modal-open path — the same kind of path onion-wasm already carries serde
for. `serde` and `serde_json` are galley dependencies behind the `wasm`
feature ONLY: `galley::overlay` computes plain Rust structs, `galley::wasm::
overlay` serializes them, and the native crate links no serializer at all.
`sous-core` and `onion` stay serde-free. JSON now; an array buffer later only
if a measurement asks for one.

An overlay is a suggestion applied on request, never a finding: nothing here
is on the publication path.

## SousSettings

`SousSettings` is every plain scalar of `JudgingConfig`, flat, with `pub` fields so
bindgen writes the accessors and JS assigns them by name. `config()` hands back
a copy of the current values; `setConfig` writes them into BOTH judging slots of
`Brigade`'s `((), JudgingConfig, JudgingConfig)`, as the CLI does, and leaves
every field that is not a knob at the value it had.

`settings.source_copy` is the one knob a `setConfig` cannot fully apply on its
own. It ships **off**, and a `Reference` registered while it was off kept no
word lane to walk (`pantry.md`), so turning it on and republishing produces no
code-3 row for those books. The handle says so rather than going quiet:

```js
settings.source_copy = true;
galley.setConfig(settings);
galley.publish();
galley.lastWordlessReferences();          // 2 — re-send those two sources
galley.updateReference("ref/RUT.usfm", rutText);   // the same bytes is enough
```

`updateReference` with byte-identical text is a real update here, not a no-op:
the lane set is part of what the Pantry serves a book from, as `keepText` is.
Turn the knob on BEFORE loading the sources and none of this arises.

Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated ladder,
not a plain field), `letters` and `doubles` (tri-state policies), and the two
roster bounds that only make sense beside `letters`. None has a plain-field
shape bindgen can carry, and no consumer has asked for one; because `apply`
touches only the settings, adding them later changes nothing that already works.

## What the handle does not do yet

`detail(handle)` and `suppress(rule, anchor)` are in
`sous-chef/planning/plans/consumer-api-sketch.md` and are not here. A structure
projection is still cut rather than retained — a book retains the verse-text
mask alone — so `structureTextOf` takes text and `mask(id, { recipe:
"structure" })` cuts the retained copy on the way past.
