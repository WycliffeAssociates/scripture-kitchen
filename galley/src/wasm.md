# The JS doorway

```js
import { Galley } from "usfm-galley";
import { FindingsSnapshot } from "sous-chef/reader.ts";

const galley = new Galley();                            // one opaque handle

galley.update("books/MRK.usfm", text);                  // whole book → "MRK"
galley.updateReference("ref/en_ult/GEN.usfm", ult);     // lengths only, no text

const snap = FindingsSnapshot.open(galley.publish());   // one complete snapshot
snap.findingsFor("books/MRK.usfm");                     // by id, via the string table
snap.patterns();                                        // what each row means

const knobs = galley.config();                          // a copy
knobs.casing = false;
galley.setConfig(knobs);                                // re-judge, no re-map

galley.remove("books/MRK.usfm");
galley.residentBytes();                                 // Pantry + caches
```

Find runs over the retained projection, and answers in one buffer:

```js
galley.find("books/MRK.usfm", "wept. Then", true, false, 0);  // one book
galley.findAll("God", false, true, 200);                      // every target
```

The onion door is on the same handle and reads the same warm chunks:

```js
deserialize(galley.parse(text, true, true, true));      // the plated book
galley.verseText(text);
galley.structureText(text);
```

## The claim

The bytes `publish()` returns in JavaScript are the bytes
`Expediter::<Brigade>::publish` returns natively — the same publication, not an
equivalent one. Three tests hold that:

| where | what it drives | against |
| --- | --- | --- |
| `tests/sous_goldens.rs` | the native `Expediter` | `tests/goldens/sous/*.bin` |
| `tests/wasm_wall.rs` | the `Galley` handle, in Node | the same three files |
| `tests/sous_conformance.mjs` | `pkg-node` plus `sous-chef/reader.ts` | the same three files |

The fixtures under `tests/fixtures/sous/` are written so the published bytes
carry a row of every wire code — hygiene, convention, and length — so a wall
that silently dropped a lane could not pass.

## Runbook

```
cd galley
wasm-pack test --node --features wasm --test wasm_wall
wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
node tests/sous_conformance.mjs pkg-node [corpus-dir]
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
u32   hitCount
u32   bookCount
hit   × hitCount    bookIndex, projectedFrom, projectedTo, pieceCount,
                    pieceCount × (sourceFrom, sourceTo)
u32   × bookCount   idByteLen
u32   × hitCount    previewByteLen
bytes               every id's UTF-8 in order, then every preview's
```

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

`bookIndex` indexes the buffer's own id table, which names every target
searched whether or not it matched — so `findAll`'s answer needs no second
call, and a hit's book cannot be misattributed by a corpus that changed in
between.

The preview is the projected text around the hit, ellipsed and trimmed for a
result card. It rides in the buffer because the projection is materialized for
the search and dropped with it: a host that wanted the same string afterwards
would have to mask the whole book again. Display text only — never read an
offset back out of it.

`limit` bounds hits across the whole call rather than per book; `0` means no
bound. `find` errors on an id that is not a registered target (a reference
retains neither text nor projection, so it cannot be searched and "no hits"
would be a lie); `findAll` over an empty corpus is an empty buffer, not an
error. The needle is a LITERAL — `case_sensitive` off is the simple lowercase
fold and `whole_word` is the words rule restated in `find.md`; there is no
regex here and the `regex` crate is not a dependency.

## Knobs

`Knobs` is every plain scalar of `JudgingConfig`, flat, with `pub` fields so
bindgen writes the accessors and JS assigns them by name. `config()` hands back
a copy of the current values; `setConfig` writes them into BOTH judging slots of
`Brigade`'s `((), JudgingConfig, JudgingConfig)`, as the CLI does, and leaves
every field that is not a knob at the value it had.

`knobs.source_copy` is the one knob a `setConfig` cannot fully apply on its
own. It ships **off**, and a `Reference` registered while it was off kept no
word lane to walk (`pantry.md`), so turning it on and republishing produces no
code-3 row for those books. The handle says so rather than going quiet:

```js
knobs.source_copy = true;
galley.setConfig(knobs);
galley.publish();
galley.lastWordlessReferences();          // 2 — re-send those two sources
galley.updateReference("ref/RUT.usfm", rutText);   // the same bytes is enough
```

`updateReference` with byte-identical text is a real update here, not a no-op:
the lane set is part of what the Pantry serves a book from. Turn the knob on
BEFORE loading the sources and none of this arises.

Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated ladder,
not a plain field), `letters` and `doubles` (tri-state policies), and the two
roster bounds that only make sense beside `letters`. None has a plain-field
shape bindgen can carry, and no consumer has asked for one; because `apply`
touches only the knobs, adding them later changes nothing that already works.

## What the handle does not do yet

`fingerprint`, `changedSinceUpdate`, and `lint` are in
`sous-chef/planning/plans/consumer-api-sketch.md` and are not here. The onion methods still take
their text per call rather than reading the Pantry's retained copy — the
The chunk cache keys on content, so they hit, but they marshal a string that the
handle already holds.
