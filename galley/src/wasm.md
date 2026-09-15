# The JS doorway

```js
import { Galley } from "usfm-galley";
import { FindingsSnapshot } from "usfm-galley/sous-reader";

const galley = new Galley();                            // one opaque handle

galley.update("books/MRK.usfm", text);                  // whole book → "MRK"
galley.updateReference("ref/en_ult/GEN.usfm", ult);     // lengths only, no text
galley.updateReference("ref/en_ult/RUT.usfm", ult, true);  // …and searchable

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
galley.find("books/MRK.usfm", "wept. Then", true, false, 0);  // one book
galley.findAll("God", false, true, 200);                      // every target
galley.findAll("God", false, true, 200, "all");               // and the sources
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
galley.find("ref/RUT.usfm", "Naomi", true, false, 0);   // the source, searched
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

`find` and `findAll` answer with one little-endian `u32` buffer and its
strings. Both doors encode the same layout (`galley::find::wire`), which is also
what the native `Expediter::find` returns, so a host that reads one reads both:

```text
u32   magic              0x444E4946 — "FIND", the four bytes in order
u32   version            1
u32   hitCount
u32   bookCount
hit   × hitCount    bookIndex, projectedFrom, projectedTo, pieceCount,
                    pieceCount × (sourceFrom, sourceTo)
u32   × bookCount   idByteLen
u32   × hitCount    previewByteLen
bytes               every id's UTF-8 in order, then every preview's
```

The two leading words are what the onion and sous buffers lead with too: a
reader a version behind fails on the header rather than on a field it misread.
Check both before reading anything else.

Every offset is **UTF-16** — the unit the editor's coordinates are already in,
the same choice `parse(text, …, utf16: true)` makes. The two length arrays sit
before the byte blob so every `u32` in the buffer stays four-byte aligned and a
reader can take one `Uint32Array` view over the head of it.

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

Which books those are is `findAll`'s fifth argument, a scope string:
`"targets"` (the default when omitted), `"references"`, or `"all"`, which
searches the targets and then the references. A reference registered without
`keepText` is in no scope — it retains nothing to search, so it is neither
searched nor listed. An unknown scope throws rather than falling back.

The preview is the projected text around the hit, ellipsed and trimmed for a
result card. It rides in the buffer because the projection is materialized for
the search and dropped with it: a host that wanted the same string afterwards
would have to mask the whole book again. Display text only — never read an
offset back out of it.

`limit` bounds hits across the whole call rather than per book; `0` means no
bound. `find` searches any registered book that retains text and a projection —
a target, or a reference registered with `keepText` — and errors on one that
retains neither, naming the argument that would fix it (`reference ref/RUT.usfm
retains no text; register it with keepText`), because "no hits" would be a lie;
`findAll` over an empty scope is an empty buffer, not an error. The needle is a LITERAL — `case_sensitive` off is the simple lowercase
fold and `whole_word` is the words rule restated in `find.md`; there is no
regex here and the `regex` crate is not a dependency.

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
`sous-chef/planning/plans/consumer-api-sketch.md` and are not here. Neither is
a structure projection off the retained copy: a book retains the verse-text
mask alone, so `structureTextOf` takes text.
