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

const targets = decodeFind(galley.findAll("the", true, false, 0));
check(targets.hits > 0, "findAll hits the fixtures' verse text");
eq(targets.ids.join(" "), "books/GEN.usfm books/RUT.usfm books/JON.usfm", "the id table");
check(
  targets.spans.every((span) => span.book < targets.ids.length && span.from < span.to),
  "every hit names a listed book and a forward span",
);

// A lengths-only reference cannot be searched, and says which argument fixes it.
let unsearchable = "";
try {
  galley.find("ref/JON.usfm", "the", true, false, 0);
} catch (error) {
  unsearchable = String(error.message ?? error);
}
check(unsearchable.includes("keepText"), `a lengths-only reference: ${unsearchable}`);

// Re-sent with the text, it joins the "references" and "all" scopes.
galley.updateReference("ref/JON.usfm", fixture("ref/JON.usfm"), true);
eq(decodeFind(galley.find("ref/JON.usfm", "the", true, false, 0)).ids.length, 1, "one book");
const references = decodeFind(galley.findAll("the", true, false, 0, "references"));
eq(references.ids.join(" "), "ref/JON.usfm", "only the reference that kept its text");
const all = decodeFind(galley.findAll("the", true, false, 0, "all"));
eq(all.ids.length, 4, "three targets and the one kept reference");
eq(all.hits, targets.hits + references.hits, "the scopes partition the hits");

let badScope = "";
try {
  galley.findAll("the", true, false, 0, "elsewhere");
} catch (error) {
  badScope = String(error.message ?? error);
}
check(badScope.includes("unknown find scope"), `an unknown scope errors: ${badScope}`);

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
