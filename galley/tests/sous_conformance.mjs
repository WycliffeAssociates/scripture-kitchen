/**
 * The Sous reader against the resident publisher, across the wall.
 *
 *   cd galley
 *   wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
 *   node tests/sous_conformance.mjs pkg-node [corpus-dir]
 *
 * Node 24 strips types, so this imports the reader as shipped — no build step.
 * It reads `galley/sous-reader.ts`, the copy the package exports, which
 * codegen writes from the same schema as `sous-chef/reader.ts`.
 *
 * The Rust half lives in `galley/tests/sous_goldens.rs` (native) and
 * `galley/tests/wasm_wall.rs` (in-module). What is left, and what this
 * asserts, is that the buffer reaching JAVASCRIPT is the same buffer, and that
 * the TypeScript reader agrees about what it says.
 *
 * With a corpus directory it also runs the keystroke lifecycle on that corpus
 * and prints the four steps a host actually pays for. Timing is a report, not
 * an assertion.
 */
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

const pkg = resolve(process.argv[2] ?? "pkg-node");
const corpusDir = process.argv[3];
const here = import.meta.dirname;

// wasm-pack's nodejs target writes CommonJS; imported from ESM, the real
// `module.exports` is the `default` key.
const doors = (await import(join(pkg, "usfm_galley.js"))).default;
const { Galley } = doors;
// The package's own copy of the reader — `usfm-galley/sous-reader`, which
// codegen writes beside `sous-chef/reader.ts` from the one schema.
const { FindingsSnapshot } = await import(resolve(here, "../sous-reader.ts"));
// The census reader, likewise generated and likewise shipped —
// `usfm-galley/toc-reader`.
const { Census, FORMAT_VERSION, HEADER_VERSION_OFFSET } = await import(
  resolve(here, "../toc-reader.ts")
);
// And find's, likewise generated — `usfm-galley/find-reader`.
const { Hits, HitRow } = await import(resolve(here, "../find-reader.ts"));
// And the mask map's — `usfm-galley/mask-reader`.
const { MaskMap, HEADER_VERSION_OFFSET: MASK_VERSION_AT, FORMAT_VERSION: MASK_VERSION } =
  await import(resolve(here, "../mask-reader.ts"));

const fixture = (name) => readFileSync(resolve(here, "fixtures/sous", name), "utf8");
const golden = (name) => new Uint8Array(readFileSync(resolve(here, "goldens/sous", name)));

let failures = 0;
const check = (ok, what) => {
  if (!ok) {
    console.error(`  FAIL  ${what}`);
    failures++;
  }
};
const eq = (a, b, what) => check(a === b, `${what}: ${a} !== ${b}`);
const sameBytes = (a, b) => a.length === b.length && a.every((byte, at) => byte === b[at]);

// --- every door is on the module ------------------------------------------

// The list, not a subset: onion-wasm's shims arrive because galley's cdylib
// links the object they sit in, and a dropped door is otherwise silent.
const EXPORTS = [
  "Edits",
  "Fingerprint",
  "FormatOpts",
  "Galley",
  "SousSettings",
  "Splices",
  "attrResolve",
  "attrs",
  "book",
  "diff",
  "format",
  "formatEdits",
  "formatEditsIn",
  "locate",
  "mask",
  "merge",
  "mergeSplices",
  "parse",
  "toByte",
  "toUtf16",
];
const exported = Object.keys(doors).sort();
console.log(`exports (${exported.length}): ${exported.join(" ")}`);
eq(exported.join(" "), EXPORTS.join(" "), "the module exports exactly the door list");

// One onion door, answered through this module.
eq(doors.toUtf16("\\v 1 a\u{1F600}b", 10), 8, "toUtf16 counts the surrogate pair");

// --- the three publications cross unchanged -------------------------------

const galley = new Galley();
eq(galley.update("books/GEN.usfm", fixture("GEN.usfm")), "GEN", "update returns the book code");
galley.update("books/RUT.usfm", fixture("RUT.usfm"));
galley.update("books/JON.usfm", fixture("JON.usfm"));
galley.updateReference("ref/RUT.usfm", fixture("ref/RUT.usfm"));
galley.updateReference("ref/JON.usfm", fixture("ref/JON.usfm"));

const coldBytes = galley.publish();
check(sameBytes(coldBytes, golden("cold.bin")), "the cold publication equals cold.bin");

galley.update("books/GEN.usfm", fixture("GEN-edited.usfm"));
const editBytes = galley.publish();
check(sameBytes(editBytes, golden("edit.bin")), "the edit publication equals edit.bin");

const settings = galley.config();
check(settings.casing, "casing ships on");
settings.casing = false;
settings.sentence_start_upper_bp = 9990;
settings.z_short = 2.0;
check(!settings.source_copy, "the source-copy lane ships off");
settings.source_copy = true;
galley.setConfig(settings);
// A reference registered before the lane was on kept no word lane; the host
// re-sends its text, and the publication says how many needed it.
galley.publish();
eq(galley.lastWordlessReferences(), 2, "both references need re-sending");
galley.updateReference("ref/RUT.usfm", fixture("ref/RUT.usfm"));
galley.updateReference("ref/JON.usfm", fixture("ref/JON.usfm"));
const knobsBytes = galley.publish();
check(sameBytes(knobsBytes, golden("knobs.bin")), "the settings publication equals knobs.bin");

// --- and the reader agrees about what they say ----------------------------

const cold = FindingsSnapshot.open(coldBytes);
const edit = FindingsSnapshot.open(editBytes);
const knobsSnap = FindingsSnapshot.open(knobsBytes);

const kinds = (snapshot) => {
  const seen = new Set();
  for (let index = 0; index < snapshot.length; index++) {
    const book = snapshot.book(index);
    for (let row = 0; row < book.count; row++) seen.add(book.at(row).kind);
  }
  return seen;
};
const coldKinds = kinds(cold);
for (const kind of ["Hygiene", "Convention", "LengthProportionality", "Presence"]) {
  check(coldKinds.has(kind), `cold.bin holds a ${kind} row`);
}

const rowsOf = (snapshot, id) => JSON.stringify(snapshot.findingsFor(id));
check(
  rowsOf(cold, "books/GEN.usfm") !== rowsOf(edit, "books/GEN.usfm"),
  "one changed word moves GEN's rows",
);
for (const id of ["books/RUT.usfm", "books/JON.usfm"]) {
  eq(rowsOf(cold, id), rowsOf(edit, id), `${id} is untouched by an edit to GEN`);
}

check(kinds(knobsSnap).has("SourceCopy"), "the settings publication holds a SourceCopy row");
check(!coldKinds.has("SourceCopy"), "and the default publication holds none");

const casingRows = (snapshot) =>
  snapshot.patterns().filter((pattern) => pattern.channel === "Casing").length;
eq(casingRows(cold), 1, "cold.bin holds the fixture's casing pattern");
eq(casingRows(knobsSnap), 0, "the settings publication publishes none");

// --- the find buffer, header first ----------------------------------------

// `FIND` little-endian, then the layout version; a stale reader has to fail
// on these two words rather than on a field it misread.
const FIND_MAGIC = 0x444e4946;
const FIND_VERSION = 1;

/** The buffer's head: the two header words, the hits, and the id table. */
const decodeFind = (bytes) => {
  const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength >> 2);
  eq(words[0], FIND_MAGIC, "the find buffer leads with FIND");
  eq(words[1], FIND_VERSION, "the find buffer names its version");
  const hits = words[2];
  const books = words[3];
  let at = 4;
  const spans = [];
  for (let hit = 0; hit < hits; hit++) {
    spans.push({ book: words[at], from: words[at + 1], to: words[at + 2] });
    at += 4 + 2 * words[at + 3];
  }
  const idLens = Array.from(words.subarray(at, at + books));
  let cursor = (at + books + hits) * 4;
  const decoder = new TextDecoder();
  const ids = idLens.map((len) => {
    const id = decoder.decode(bytes.subarray(cursor, cursor + len));
    cursor += len;
    return id;
  });
  return { hits, ids, spans };
};

const targets = decodeFind(galley.findAll("the", { caseSensitive: true }));
check(targets.hits > 0, "findAll hits the fixtures' verse text");
eq(targets.ids.join(" "), "books/GEN.usfm books/RUT.usfm books/JON.usfm", "the id table");
check(
  targets.spans.every((span) => span.book < targets.ids.length && span.from < span.to),
  "every hit names a listed book and a forward span",
);

// A lengths-only reference cannot be searched, and says which argument fixes it.
let unsearchable = "";
try {
  galley.find("ref/JON.usfm", "the", { caseSensitive: true });
} catch (error) {
  unsearchable = String(error.message ?? error);
}
check(unsearchable.includes("keepText"), `a lengths-only reference: ${unsearchable}`);

// Re-sent with the text, it joins the "references" and "all" scopes.
galley.updateReference("ref/JON.usfm", fixture("ref/JON.usfm"), true);
eq(decodeFind(galley.find("ref/JON.usfm", "the", { caseSensitive: true })).ids.length, 1, "one book");
const references = decodeFind(galley.findAll("the", { caseSensitive: true, scope: "references" }));
eq(references.ids.join(" "), "ref/JON.usfm", "only the reference that kept its text");
const all = decodeFind(galley.findAll("the", { caseSensitive: true, scope: "all" }));
eq(all.ids.length, 4, "three targets and the one kept reference");
eq(all.hits, targets.hits + references.hits, "the scopes partition the hits");

let badScope = "";
try {
  galley.findAll("the", { caseSensitive: true, scope: "elsewhere" });
} catch (error) {
  badScope = String(error.message ?? error);
}
check(badScope.includes("unknown find scope"), `an unknown scope errors: ${badScope}`);

// A wrong-typed option errors by NAME, not by silently coercing.
let badType = "";
try {
  galley.findAll("the", { caseSensitive: "yes" });
} catch (error) {
  badType = String(error.message ?? error);
}
check(badType.includes("caseSensitive"), `a wrong-typed option names itself: ${badType}`);

// `scope` names more than one book, so `find` refuses it rather than reading
// and discarding it.
let scopeOnFind = "";
try {
  galley.find("ref/JON.usfm", "the", { scope: "all" });
} catch (error) {
  scopeOnFind = String(error.message ?? error);
}
check(scopeOnFind.includes("scope"), `scope on find is refused: ${scopeOnFind}`);

// The SHIPPED reader over the same bytes. `decodeFind` above is hand-written
// against the documented layout, which makes it the independent oracle for the
// generated one: if the reader and the writer agreed with each other but not
// with the format, this is what notices.
const findBuffer = galley.findAll("the", { caseSensitive: true, scope: "all" });
const shipped = Hits.open(findBuffer);
const byHand = decodeFind(findBuffer);
eq(shipped.hitCount, byHand.hits, "the reader counts the hits the decoder does");
eq(shipped.bookCount, byHand.ids.length, "and the books");
eq(shipped.ids().join(" "), byHand.ids.join(" "), "and names them in the same order");
for (let n = 0; n < shipped.hitCount; n++) {
  const hit = shipped.hit(n);
  const span = byHand.spans[n];
  check(
    hit.bookIndex === span.book && hit.projectedFrom === span.from && hit.projectedTo === span.to,
    `hit ${n} reads the same through both`,
  );
  check(hit.pieces().length === hit.pieceCount, `hit ${n}'s pieces match its count`);
  check(typeof shipped.preview(n) === "string", `hit ${n} has a preview`);
}

// A tailed cursor has no arithmetic to fall back on: a row past the end would
// read offset 0 silently, so it throws. `HitRow` is exported, so this is not
// covered by `Hits.hit`'s own range check.
let pastTheEnd = "";
try {
  new HitRow(new DataView(new ArrayBuffer(0))).seek(0);
} catch (error) {
  pastTheEnd = String(error.message ?? error);
}
check(pastTheEnd.includes("row 0 of 0"), `seeking past the end throws: ${pastTheEnd}`);

// The same promise the census reader keeps: a buffer this reader does not know
// fails at `open`, naming both versions.
const shiftedFind = new Uint8Array(findBuffer);
new DataView(shiftedFind.buffer).setUint32(4, FIND_VERSION + 1, true);
let staleFind = "";
try {
  Hits.open(shiftedFind);
} catch (error) {
  staleFind = String(error.message ?? error);
}
check(
  staleFind.includes(`v${FIND_VERSION + 1}`) && staleFind.includes("update usfm-galley"),
  `a newer find buffer is refused, not misread: ${staleFind}`,
);

// --- the mask map: where the projection came from -------------------------

// The text behind each id at this point in the script — GEN was re-sent with
// the edit, and ref/JON.usfm was re-sent with its text.
const TEXT_OF = {
  "books/GEN.usfm": "GEN-edited.usfm",
  "books/RUT.usfm": "RUT.usfm",
  "books/JON.usfm": "JON.usfm",
  "ref/JON.usfm": "ref/JON.usfm",
};

/** Laws 1, 2, 3 and 5, by hand, over one opened map. */
const maskLaws = (map, what) => {
  let at = 0;
  let previous = -1;
  let ok = true;
  for (let n = 0; n < map.rangeCount; n++) {
    const r = map.range(n);
    if (!(r.sourceFrom < r.sourceTo)) ok = false;
    if (!(previous < r.sourceFrom)) ok = false;
    if (map.starts[n] !== at) ok = false;
    if (r.sourceTo > map.sourceLen) ok = false;
    previous = r.sourceTo;
    at += r.sourceTo - r.sourceFrom;
  }
  check(ok, `${what}: the ranges are sorted, disjoint, non-empty and prefix-summed`);
  eq(at, map.projectedLen, `${what}: the prefix sum is the projection`);
};

/** The ranges joined out of `source`, which law 4 says IS the projection. */
const joinMask = (map, source) => {
  let out = "";
  for (let n = 0; n < map.rangeCount; n++) {
    const r = map.range(n);
    out += source.slice(r.sourceFrom, r.sourceTo);
  }
  return out;
};

const genText = fixture(TEXT_OF["books/GEN.usfm"]);
const genVerseText = galley.verseText("books/GEN.usfm");

const byteMap = MaskMap.open(galley.mask("books/GEN.usfm"));
maskLaws(byteMap, "mask(id)");
check(!byteMap.utf16, "mask(id) answers in bytes unless asked otherwise");
eq(byteMap.recipe, "verseText", "and in the default recipe");
check(byteMap.rangeCount > 1, `the projection is made of ${byteMap.rangeCount} spans`);

// The UTF-16 buffer is the one a JavaScript host can slice with, because a JS
// string is counted in exactly those units.
const wideMap = MaskMap.open(galley.mask("books/GEN.usfm", { utf16: true }));
maskLaws(wideMap, "mask(id, { utf16: true })");
check(wideMap.utf16, "the UTF-16 buffer says so in its flags");
eq(wideMap.rangeCount, byteMap.rangeCount, "one map, two units");
eq(wideMap.sourceLen, genText.length, "sourceLen is the text the map was cut from");
eq(wideMap.projectedLen, genVerseText.length, "projectedLen is the reading's length");
eq(joinMask(wideMap, genText), genVerseText, "the joined slices ARE verseText(id)");

// A structure map is a different map of the same text, and says which it is.
const structureMap = MaskMap.open(
  galley.mask("books/GEN.usfm", { recipe: "structure", utf16: true }),
);
maskLaws(structureMap, 'mask(id, { recipe: "structure" })');
eq(structureMap.recipe, "structure", "the buffer names its recipe");
eq(
  joinMask(structureMap, genText),
  galley.structureTextOf(genText),
  "and joins to the structure text",
);

// The loose door over the same bytes writes the same buffer.
const looseBytes = galley.maskOf(genText);
check(
  sameBytes(looseBytes, galley.mask("books/GEN.usfm")),
  "maskOf(text) equals mask(id) byte for byte",
);
check(
  sameBytes(
    galley.maskOf(genText, { utf16: true }),
    galley.mask("books/GEN.usfm", { utf16: true }),
  ),
  "and in UTF-16",
);
maskLaws(MaskMap.open(looseBytes), "maskOf(text)");

// THE cross-check: the map answers what a find hit already carries. Both are
// UTF-16, both name source spans, and they are computed by different code.
const maps = new Map();
let compared = 0;
for (let n = 0; n < shipped.hitCount; n++) {
  const hit = shipped.hit(n);
  const id = shipped.id(hit.bookIndex);
  if (!maps.has(id)) maps.set(id, MaskMap.open(galley.mask(id, { utf16: true })));
  const map = maps.get(id);
  const mine = map.pieces(hit.projectedFrom, hit.projectedTo);
  const theirs = hit.pieces();
  let same = mine.length === theirs.length;
  for (let p = 0; same && p < mine.length; p++) {
    const piece = theirs.seek(p);
    same = mine[p][0] === piece.sourceFrom && mine[p][1] === piece.sourceTo;
  }
  check(same, `hit ${n} in ${id}: the map's pieces are the hit's`);
  compared++;
}
check(compared > 0, `${compared} hits cross-checked against their book's map`);

// And every id the map was built for joins back to its own verse text.
for (const [id, map] of maps) {
  eq(joinMask(map, fixture(TEXT_OF[id])), galley.verseText(id), `${id}: the map rebuilds it`);
}

// A lengths-only reference has no projection to map, and says which argument
// fixes it — the same sentence find uses.
let unmappable = "";
try {
  galley.mask("ref/RUT.usfm");
} catch (error) {
  unmappable = String(error.message ?? error);
}
check(unmappable.includes("keepText"), `a lengths-only reference: ${unmappable}`);

let unknownMaskBook = "";
try {
  galley.mask("books/NOPE.usfm");
} catch (error) {
  unknownMaskBook = String(error.message ?? error);
}
check(unknownMaskBook.includes("no book is registered"), `an unknown id: ${unknownMaskBook}`);

let badRecipe = "";
try {
  galley.mask("books/GEN.usfm", { recipe: "prose" });
} catch (error) {
  badRecipe = String(error.message ?? error);
}
check(
  badRecipe.includes("prose") && badRecipe.includes("verseText") && badRecipe.includes("structure"),
  `an unknown recipe names the two that exist: ${badRecipe}`,
);

let badMaskType = "";
try {
  galley.mask("books/GEN.usfm", { utf16: "yes" });
} catch (error) {
  badMaskType = String(error.message ?? error);
}
check(badMaskType.includes("utf16"), `a wrong-typed option names itself: ${badMaskType}`);

// The same promise every generated reader keeps.
const shiftedMask = new Uint8Array(galley.mask("books/GEN.usfm"));
new DataView(shiftedMask.buffer).setUint32(MASK_VERSION_AT, MASK_VERSION + 1, true);
let staleMask = "";
try {
  MaskMap.open(shiftedMask);
} catch (error) {
  staleMask = String(error.message ?? error);
}
check(
  staleMask.includes(`v${MASK_VERSION + 1}`) && staleMask.includes("update usfm-galley"),
  `a newer mask buffer is refused, not misread: ${staleMask}`,
);

// --- the census: what the project holds, read through its own reader -------

// The oracle is the fixture TEXT, counted in JavaScript: if the buffer and the
// reader agreed with each other but not with the file, this is what notices.
const marks = (text, pattern) => (text.match(pattern) ?? []).length;
// The text each id currently holds — GEN was re-sent with the edit above.
const REGISTERED = {
  "books/GEN.usfm": "GEN-edited.usfm",
  "books/RUT.usfm": "RUT.usfm",
  "books/JON.usfm": "JON.usfm",
};

const targetCensus = Census.open(galley.tocAll());
eq(targetCensus.bookCount, 3, "every target is in the census");
check(!targetCensus.utf16, "offsets are bytes unless asked otherwise");
for (const book of targetCensus) {
  const text = fixture(REGISTERED[book.id]);
  eq(book.chapters, marks(text, /^\\c /gm), `${book.id}: \\c markers`);
  eq(book.verseCount, marks(text, /\\v /g), `${book.id}: \\v markers`);
  eq(book.chapterCount, book.chapters + 1, `${book.id}: the front-matter row`);
  // The rows a sidebar actually draws, off the cursor rather than a count.
  const rows = book.chapterRows;
  eq(rows.length, book.chapterCount, `${book.id}: the cursor sees every row`);
  eq(rows.seek(0).number, 0, `${book.id}: row 0 is the front matter`);
  let anchors = 0;
  for (let row = 0; row < rows.length; row++) anchors += rows.seek(row).anchors;
  eq(anchors, book.verseCount, `${book.id}: the rows account for every anchor`);
}

// One book by id says exactly what the project-wide call said about it.
const alone = Census.open(galley.toc("books/GEN.usfm"));
eq(alone.bookCount, 1, "one book, asked for by id");
eq(alone.book(0).code, targetCensus.find("books/GEN.usfm").code, "the same book");
eq(
  alone.book(0).chapterRows.seek(1).lastVerse,
  targetCensus.find("books/GEN.usfm").chapterRows.seek(1).lastVerse,
  "and the same rows",
);

// A reference keeps its Toc whatever else it drops, so the census reaches
// further than find: ref/RUT.usfm kept no text and is still listed.
eq(Census.open(galley.tocAll("all")).bookCount, 5, "three targets and both references");
eq(Census.open(galley.tocAll("references")).bookCount, 2, "both references");
check(Census.open(galley.tocAll("targets", true)).utf16, "the targets answer in UTF-16");

let noTable = "";
try {
  galley.tocAll("all", true);
} catch (error) {
  noTable = String(error.message ?? error);
}
check(
  noTable.includes("ref/RUT.usfm") && noTable.includes("keepText"),
  `a textless reference cannot answer utf16: ${noTable}`,
);

let unknownBook = "";
try {
  galley.toc("books/NOPE.usfm");
} catch (error) {
  unknownBook = String(error.message ?? error);
}
check(unknownBook.includes("no book is registered"), `an unknown id: ${unknownBook}`);

let badCensusScope = "";
try {
  galley.tocAll("elsewhere");
} catch (error) {
  badCensusScope = String(error.message ?? error);
}
check(
  badCensusScope.includes("unknown census scope"),
  `an unknown scope errors: ${badCensusScope}`,
);

// The promise the generated reader exists to keep: a buffer this reader does
// not know fails at `open`, naming both versions — never one field misread.
const shifted = new Uint8Array(galley.tocAll());
new DataView(shifted.buffer).setUint32(HEADER_VERSION_OFFSET, FORMAT_VERSION + 1, true);
let stale = "";
try {
  Census.open(shifted);
} catch (error) {
  stale = String(error.message ?? error);
}
check(
  stale.includes(`v${FORMAT_VERSION + 1}`) && stale.includes("update usfm-galley"),
  `a newer buffer is refused, not misread: ${stale}`,
);

// --- the overlay: six doors, and the JSON they answer with -----------------

// The six read as methods on the handle, like find: they need the resident
// Pantry, so they are not free functions.
const OVERLAY_DOORS = [
  "overlay",
  "overlayReport",
  "overlayText",
  "skeleton",
  "sourceNodeFor",
  "targetNodeFor",
];
for (const door of OVERLAY_DOORS) {
  check(typeof galley[door] === "function", `galley.${door} is on the handle`);
}

const overlayFixture = (name) =>
  readFileSync(resolve(here, "fixtures/overlay", name), "utf8");
galley.update("books/OVL.usfm", overlayFixture("gen-target.usfm"));
galley.updateReference("ref/OVL.usfm", overlayFixture("gen-source.usfm"), true);

// The one JSON parse: the reader is `JSON.parse`, because a skeleton is a
// modal-open shape and not a keystroke one.
const skeleton = JSON.parse(galley.skeleton("ref/OVL.usfm"));
eq(skeleton.verses.length, 4, "the source names four verses");
eq(
  skeleton.blocks.map((b) => `${b.sid} ${b.where} ${b.ordinal} ${b.marker}`).join(" | "),
  "GEN 2:21 leading 1 p | GEN 2:23 inside 1 q1 | GEN 2:23 inside 2 q2 | " +
    "GEN 2:23 inside 3 q1 | GEN 2:23 inside 4 q2 | GEN 2:24 leading 1 p",
  "every block, addressed",
);

const edits = galley.overlay("books/OVL.usfm", "ref/OVL.usfm");
eq(edits.lens.length, 6, "six blocks cross");
eq(edits.spans.length, 12, "one from/to pair each");
check(
  edits.spans.every((span, at) => at % 2 === 1 || span <= edits.spans[at + 1]),
  "every span is forward",
);

const report = JSON.parse(galley.overlayReport("books/OVL.usfm", "ref/OVL.usfm"));
eq(report.removed.length, 0, "the target has nothing the source lacks");
eq(report.unpaired.length, 0, "every verse pairs");
eq(report.inserted.filter((row) => row.empty).length, 4, "four inside blocks await text");

const applied = galley.overlayText("books/OVL.usfm", "ref/OVL.usfm");
check(applied.includes("\\q1\n\\q2\n\\q1\n\\q2\n\\p\n\\v 24"), "the poetry lands");
check(!applied.includes("\\f "), "the source's footnotes stay home");

const where = { sid: "GEN 2:23", where: "inside", ordinal: 1, marker: "q1" };
const inSource = JSON.parse(galley.sourceNodeFor("books/OVL.usfm", "ref/OVL.usfm", where));
eq(inSource.found.marker, "q1", "the source block the address names");
const inTarget = JSON.parse(galley.targetNodeFor("books/OVL.usfm", "ref/OVL.usfm", where));
check(inTarget.absent === true, "the target has no such block yet");
eq(inTarget.where, "inside", "and the overlay would put it after the verse text");

// The marker is a check, not a key: the same position, misnamed, throws.
let staleAddress = "";
try {
  galley.targetNodeFor("books/OVL.usfm", "ref/OVL.usfm", { ...where, marker: "q2" });
} catch (error) {
  staleAddress = String(error.message ?? error);
}
check(staleAddress.includes("the address is stale"), `a stale address: ${staleAddress}`);

// UTF-16 is the same opt-in every other door takes.
const units = galley.overlay("books/OVL.usfm", "ref/OVL.usfm", { utf16: true });
check(
  units.spans.every((span, at) => span <= edits.spans[at]),
  "UTF-16 offsets never exceed their byte offsets",
);
check(units.spans.some((span, at) => span < edits.spans[at]), "the fixture is not ASCII");

galley.remove("books/OVL.usfm");
galley.remove("ref/OVL.usfm");

// --- the onion doors read the retained copy -------------------------------

// GEN is the edited fixture by now: the id door plates what the handle holds.
const byId = galley.parse("books/GEN.usfm", true, true, true);
const byText = galley.parseText(fixture("GEN-edited.usfm"), true, true, true);
check(sameBytes(byId, byText), "parse(id) equals parse(text) byte for byte");
eq(
  galley.verseText("books/GEN.usfm"),
  galley.verseTextOf(fixture("GEN-edited.usfm")),
  "verseText(id) equals verseTextOf(text)",
);

// A reference keeps no text, so the text-needing doors refuse by name.
let refused = "";
try {
  galley.lint("ref/RUT.usfm");
} catch (error) {
  refused = String(error.message ?? error);
}
check(refused.includes("retains no text"), `a reference refuses lint: ${refused}`);

// Dirty is positional, rework is set membership.
const baseline = galley.fingerprint(fixture("GEN.usfm"));
const current = galley.fingerprint(fixture("GEN-edited.usfm"));
check(baseline.differsFrom(current), "one edited word makes the file dirty");
eq(baseline.changedChunks(current).length, 2, "one chunk to re-derive, as a from/to pair");
eq(galley.changedSinceUpdate("books/JON.usfm", fixture("JON.usfm")).length, 0, "JON is current");
eq(galley.changedSinceUpdate("books/NUM.usfm", fixture("JON.usfm")), undefined, "unregistered");
baseline.free();
current.free();

// --- the keystroke lifecycle, if a corpus is here -------------------------

if (corpusDir && existsSync(corpusDir)) {
  const median = (samples) => samples.slice().sort((a, b) => a - b)[samples.length >> 1];
  const us = (start) => Number(process.hrtime.bigint() - start) / 1000;
  const now = () => process.hrtime.bigint();

  const books = readdirSync(corpusDir)
    .filter((name) => /\.usfm$/i.test(name))
    .sort()
    .map((name) => [name, readFileSync(join(corpusDir, name), "utf8").replace(/\r\n/g, "\n")]);

  const host = new Galley();
  const coldStart = now();
  for (const [name, text] of books) host.update(name, text);
  const coldBuffer = host.publish();
  const coldMs = us(coldStart) / 1000;
  console.log(`corpus: ${books.length} books, cold publish ${coldMs.toFixed(1)} ms`);
  console.log(`  publication ${coldBuffer.length} bytes, residentBytes ${host.residentBytes()}`);

  // One book, one chapter, one character — twenty times, warm.
  const [editedId, original] = books.find(([name]) => /MRK/i.test(name)) ?? books[0];
  const seam = original.indexOf("\\c ", original.length >> 1);
  const at = original.indexOf("\n", seam) + 1;
  const steps = { marshal: [], update: [], publish: [], read: [] };
  const idle = { publish: [], read: [] };

  for (let n = 0; n < 20; n++) {
    const edited = `${original.slice(0, at)}\\p ${"aeiou"[n % 5]}${n}\n${original.slice(at)}`;

    let start = now();
    host.parseText(edited, true, true, true);
    steps.marshal.push(us(start));

    start = now();
    host.update(editedId, edited);
    steps.update.push(us(start));

    start = now();
    const bytes = host.publish();
    steps.publish.push(us(start));

    start = now();
    FindingsSnapshot.open(bytes).findingsFor(editedId);
    steps.read.push(us(start));

    start = now();
    const same = host.publish();
    idle.publish.push(us(start));

    start = now();
    FindingsSnapshot.open(same).findingsFor(editedId);
    idle.read.push(us(start));
  }

  const line = (label, value) => console.log(`  ${label.padEnd(22)} ${value.toFixed(1)} µs`);
  console.log(`keystroke lifecycle (median of 20, ${editedId}):`);
  line("1 text marshal", median(steps.marshal));
  line("2 update", median(steps.update));
  line("3 publish", median(steps.publish));
  line("4 open + findingsFor", median(steps.read));
  line(
    "sum",
    median(steps.marshal) + median(steps.update) + median(steps.publish) + median(steps.read),
  );
  console.log("unchanged publish (median of 20):");
  line("3 publish", median(idle.publish));
  line("4 open + findingsFor", median(idle.read));
  line("sum", median(idle.publish) + median(idle.read));
} else if (corpusDir) {
  console.log(`corpus: ${corpusDir} is absent — timing skipped`);
}

console.log(failures === 0 ? "sous conformance: OK" : `sous conformance: ${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
