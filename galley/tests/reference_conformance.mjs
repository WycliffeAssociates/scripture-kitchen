/**
 * The reference doors from JavaScript, as Sefer calls them:
 *
 *   wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
 *   node tests/reference_conformance.mjs pkg-node
 *
 * `Citations` runs reference/fixtures/cite.json (Sefer's citation cases) with
 * the names it carries; `parseReference`, `formatReference`, `showReference`
 * and `Galley.addressAtOf` are checked against hand-written cases. Offsets
 * cross in UTF-16.
 */

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const pkg = resolve(process.argv[2] ?? "pkg-node");
const here = import.meta.dirname;
const doors = (await import(join(pkg, "usfm_galley.js"))).default;
const { Citations, Galley, parseReference, formatReference, showReference } = doors;

let failures = 0;
// Key order is not part of the shape: compare with keys sorted.
const canonical = (value) =>
  Array.isArray(value)
    ? value.map(canonical)
    : value !== null && typeof value === "object"
      ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]))
      : value;
const eq = (got, want, what) => {
  const a = JSON.stringify(canonical(got));
  const b = JSON.stringify(canonical(want));
  if (a !== b) {
    failures += 1;
    console.error(`✗ ${what}\n    got  ${a}\n    want ${b}`);
  }
};
const throws = (call, says, what) => {
  let message = "";
  try {
    call();
  } catch (error) {
    message = String(error.message ?? error);
  }
  if (!message.includes(says)) {
    failures += 1;
    console.error(`✗ ${what}: ${message || "did not throw"}`);
  }
};

// --- Citations: Sefer's cases --------------------------------------------
{
  const fixture = JSON.parse(readFileSync(resolve(here, "../../reference/fixtures/cite.json"), "utf8"));
  const citations = new Citations({ names: fixture.catalogue.names, intro: fixture.catalogue.intro });
  for (const c of fixture.cases) {
    const got = JSON.parse(citations.read(c.text, { grammar: c.grammar, ...(c.held ? { held: c.held } : {}) }));
    if (c.refs) {
      eq(got.ok, true, `${JSON.stringify(c.text)} reads`);
      if (got.ok) eq(formatReference(got.addresses).split("; ").filter(Boolean), c.refs, JSON.stringify(c.text));
      if (got.ok && c.assumed) {
        eq(got.assumed.map((a) => (a.kind === "prefix" ? `prefix:${a.book}` : a.kind)), c.assumed, `${c.text} assumed`);
      }
    } else {
      eq(got.problem, c.problem, JSON.stringify(c.text));
      if (c.word) eq(got.word, c.word, `${c.text} word`);
    }
  }
  console.log(`citations: ${fixture.cases.length} cases`);

  // The shape Sefer reads: a book on every address, the text trimmed.
  eq(
    JSON.parse(citations.read("  Mateo 5:3-7 ", { grammar: "prose" })),
    {
      ok: true,
      text: "Mateo 5:3-7",
      addresses: [{ kind: "verses", from: { chapter: 5, verse: 3 }, to: { chapter: 5, verse: 7 }, book: "MAT" }],
      assumed: [],
    },
    "a citation's JSON",
  );
  // REV too: canon's English aliases include "Revelation of John".
  eq(citations.booksMatching("john 3:16"), ["JHN", "1JN", "2JN", "3JN", "REV"], "booksMatching");
  // A heart language's names arrive from the host, in any script.
  const am = new Citations({ names: { MAT: ["ማቴዎስ"] }, intro: ["መግቢያ"] });
  eq(formatReference(JSON.parse(am.read("ማቴዎስ 5:3")).addresses), "MAT 5:3", "an Amharic name");
  eq(formatReference(JSON.parse(am.read("ማቴዎስ መግቢያ")).addresses), "MAT 0", "an Amharic intro word");
  throws(() => citations.read("Mat 1", { grammer: "prose" }), "unknown option", "a misspelled option");
  throws(() => citations.read("Mat 1", { grammar: "poetry" }), "grammar", "an unknown grammar");
  throws(() => new Citations({ names: { MATT: ["x"] } }), "book code", "a four-letter code");
}

// --- the machine spelling -------------------------------------------------
{
  const read = (text, opts) => JSON.parse(parseReference(text, opts));
  eq(
    read("MAT 1:20-2:3; MRK 1:1").addresses,
    [
      { kind: "verses", from: { chapter: 1, verse: 20 }, to: { chapter: 2, verse: 3 }, book: "MAT" },
      { kind: "verses", from: { chapter: 1, verse: 1 }, to: { chapter: 1, verse: 1 }, book: "MRK" },
    ],
    "a list",
  );
  eq(read("PSA 7:0").addresses[0].from, { chapter: 7, verse: 0 }, "a title");
  eq(read("LUK 11:2b").addresses[0].from, { chapter: 11, verse: 2, segment: "b" }, "a part");
  eq(read("MRK 0").addresses, [{ kind: "intro", book: "MRK" }], "the intro");
  eq(read("5:3-12", { book: "MAT" }).addresses[0].book, "MAT", "a book from context");
  eq(read("rom 9:1"), { ok: false, error: "expected-book", at: 0 }, "strict");
  // The offset counts UTF-16 units: `é` is one, and so are the bytes before it.
  eq(read("ROM 9:1; é"), { ok: false, error: "expected-book", at: 9 }, "an error's offset");
  eq(read("ROM 3:21!3").error, "not-basic", "srs's");

  for (const text of ["MAT 1:20-2:3; MRK 1:1", "PSA 7:0", "LUK 11:2a-3", "MRK 0", "GEN", "MAT 2-4"]) {
    eq(formatReference(read(text).addresses), text, `${text} round trips`);
  }
  const lk = read("LUK 11:2a-3").addresses[0];
  eq(showReference(lk, "Lucas", "introducción"), "Lucas 11:2a-3", "showReference");
  eq(showReference({ kind: "intro", book: "LUK" }, "Lucas", "introducción"), "Lucas introducción", "an intro shown");
  throws(() => formatReference([{ kind: "book" }]), "book", "an address without its book");
  throws(() => formatReference("MAT"), "array", "not an array");
}

// --- the place at a cursor --------------------------------------------------
{
  const galley = new Galley();
  const text = "\\id PSA\n\\c 7\n\\s1 Prayer\n\\d A Shiggaion\n\\q1\n\\v 1 O Yahweh\n\\v 2a lest \\v 2b they\n";
  const at = (needle, opts) => JSON.parse(galley.addressAtOf(text, text.indexOf(needle), opts));
  eq(at("\\id"), { kind: "intro", book: "PSA" }, "the front matter");
  eq(at("\\c 7"), { kind: "chapters", from: 7, to: 7, book: "PSA" }, "a chapter's head");
  eq(at("\\s1").from, { chapter: 7, verse: 0 }, "a heading above the title is verse 0's");
  eq(at("Shiggaion").from, { chapter: 7, verse: 0 }, "the title");
  eq(at("\\q1").from, { chapter: 7, verse: 1 }, "verse 1's tile starts at its paragraph");
  eq(at("they").from, { chapter: 7, verse: 2, segment: "b" }, "a part");

  // UTF-16: the offset of `b` after an astral character is one less in bytes' terms.
  const astral = "\\id GEN\n\\c 1\n\\p \\v 1 𝔄 \\v 2 b\n";
  const unit = astral.indexOf("b");
  eq(JSON.parse(galley.addressAtOf(astral, unit, { utf16: true })).from, { chapter: 1, verse: 2 }, "a UTF-16 offset");
  galley.free();
}

console.log(failures === 0 ? "reference conformance: OK" : `reference conformance: ${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
