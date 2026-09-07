/**
 * The Sous reader against the resident publisher, across the wall.
 *
 *   cd galley
 *   wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
 *   node tests/sous_conformance.mjs pkg-node [corpus-dir]
 *
 * Node 24 strips types, so this imports `sous-chef/reader.ts` as shipped — no
 * build step, and no second copy of the reader to fall out of date.
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

const { Galley } = await import(join(pkg, "usfm_galley.js"));
const { FindingsSnapshot } = await import(resolve(here, "../../sous-chef/reader.ts"));

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

const knobs = galley.config();
check(knobs.casing, "casing ships on");
knobs.casing = false;
knobs.sentence_start_upper_bp = 9990;
knobs.z_short = 2.0;
galley.setConfig(knobs);
const knobsBytes = galley.publish();
check(sameBytes(knobsBytes, golden("knobs.bin")), "the knobs publication equals knobs.bin");

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

const casingRows = (snapshot) =>
  snapshot.patterns().filter((pattern) => pattern.channel === "Casing").length;
eq(casingRows(cold), 1, "cold.bin holds the fixture's casing pattern");
eq(casingRows(knobsSnap), 0, "the knobs publication publishes none");

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
    host.parse(edited, true, true, true);
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
