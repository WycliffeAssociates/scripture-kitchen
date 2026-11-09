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
    const got = citations.read(c.text, { grammar: c.grammar, ...(c.held ? { held: c.held } : {}) });
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
    citations.read("  Mateo 5:3-7 ", { grammar: "prose" }),
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
  // No peripheral or XXA–XXG unless the project holds it or asks.
  eq(citations.booksMatching("ma"), ["MAL", "MAT", "MRK", "1MA", "2MA", "3MA", "4MA", "MAN"], "no peripherals");
  eq(new Citations({ books: ["FRT"] }).booksMatching("frt"), ["FRT"], "a held peripheral");
  eq(new Citations({ peripherals: true }).booksMatching("xxa"), ["XXA"], "peripherals asked for");
  eq(citations.read("FRT 1").ok, true, "reading still knows them");
  // A heart language's names arrive from the host, in any script.
  const am = new Citations({ names: { MAT: ["ማቴዎስ"] }, intro: ["መግቢያ"] });
  eq(formatReference(am.read("ማቴዎስ 5:3").addresses), "MAT 5:3", "an Amharic name");
  eq(formatReference(am.read("ማቴዎስ መግቢያ").addresses), "MAT 0", "an Amharic intro word");
  throws(() => citations.read("Mat 1", { grammer: "prose" }), "unknown option", "a misspelled option");
  throws(() => citations.read("Mat 1", { grammar: "poetry" }), "grammar", "an unknown grammar");
  throws(() => new Citations({ names: { MATT: ["x"] } }), "book code", "a four-letter code");
}

// --- the machine spelling -------------------------------------------------
{
  const read = (text, opts) => parseReference(text, opts);
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
  eq(formatReference(lk), "LUK 11:2a-3", "one address, no list");
  // Every structured answer is an object, never a JSON string.
  eq(typeof parseReference("MAT 1:1"), "object", "an object");
  eq(showReference({ kind: "intro", book: "LUK" }, "Lucas", "introducción"), "Lucas introducción", "an intro shown");
  throws(() => formatReference([{ kind: "book" }]), "book", "an address without its book");
  throws(() => formatReference("MAT"), "an address or an array", "neither an address nor a list");
}

// --- the place at a cursor --------------------------------------------------
{
  const galley = new Galley();
  const text = "\\id PSA\n\\c 7\n\\s1 Prayer\n\\d A Shiggaion\n\\q1\n\\v 1 O Yahweh\n\\v 2a lest \\v 2b they\n";
  const at = (needle, opts) => galley.addressAtOf(text, text.indexOf(needle), opts);
  eq(at("\\id"), { kind: "intro", book: "PSA" }, "the front matter");
  eq(at("\\c 7"), { kind: "chapters", from: 7, to: 7, book: "PSA" }, "a chapter's head");
  eq(at("\\s1").from, { chapter: 7, verse: 0 }, "a heading above the title is verse 0's");
  eq(at("Shiggaion").from, { chapter: 7, verse: 0 }, "the title");
  eq(at("\\q1").from, { chapter: 7, verse: 1 }, "verse 1's tile starts at its paragraph");
  eq(at("they").from, { chapter: 7, verse: 2, segment: "b" }, "a part");

  // UTF-16: the offset of `b` after an astral character is one less in bytes' terms.
  const astral = "\\id GEN\n\\c 1\n\\p \\v 1 𝔄 \\v 2 b\n";
  const unit = astral.indexOf("b");
  eq(galley.addressAtOf(astral, unit, { utf16: true }).from, { chapter: 1, verse: 2 }, "a UTF-16 offset");

  // An offset outside the text is refused, not wrapped or clamped: a u32
  // once read -1 as the book's last verse.
  galley.update("PSA", text);
  eq(galley.addressAt("PSA", text.length).from, { chapter: 7, verse: 2, segment: "b" }, "the end itself");
  throws(() => galley.addressAt("PSA", -1), "outside the text", "a negative offset");
  throws(() => galley.addressAt("PSA", text.length + 1), "outside the text", "past the end");
  throws(() => galley.addressAt("PSA", 1.5), "outside the text", "a fraction");
  throws(() => galley.addressAt("PSA", NaN), "outside the text", "NaN");
  throws(() => galley.addressAtOf(text, -1), "outside the text", "addressAtOf too");
  // The end in UTF-16 units is shorter than in bytes; one past it is refused.
  throws(() => galley.addressAtOf(astral, astral.length + 1, { utf16: true }), "outside the text", "past the UTF-16 end");
  throws(() => galley.srsAt("PSA", -1, 3), "outside the text", "srsAt");
  throws(() => galley.srsAtOf(text, 5, 2), "after", "a backwards span");
  galley.free();
}

// --- a book's places, and the stale-id guard ------------------------------
{
  const { xxh3Text } = doors;
  const galley = new Galley();
  const psalm = "\\id PSA\n\\c 7\n\\s1 Prayer\n\\d A Shiggaion\n\\q1\n\\v 1 O 𝔄 Yahweh\n\\s1 Rise\n\\p\n\\v 2 lest\n";
  galley.update("PSA", psalm);
  const places = galley.places("PSA", { utf16: true });
  eq(
    places.map((p) => [formatReference(p.address), psalm.slice(p.opening, p.to)]),
    [
      ["PSA 0", "\\id PSA\n"],
      ["PSA 7", "\\c 7\n"],
      ["PSA 7:0", "\\s1 Prayer\n\\d A Shiggaion\n"],
      ["PSA 7:1", "\\q1\n\\v 1 O 𝔄 Yahweh\n"],
      ["PSA 7:2", "\\s1 Rise\n\\p\n\\v 2 lest\n"],
    ],
    "places tile the book",
  );
  eq(psalm.slice(places[3].from, places[3].from + 4), "\\v 1", "from is the place's own marker");
  // Each place's address is what addressAt answers inside it.
  for (const p of places) eq(galley.addressAt("PSA", p.opening, { utf16: true }), p.address, `addressAt at ${p.opening}`);
  eq(galley.placesOf(psalm, { utf16: true }), places, "placesOf answers the same");

  // The guard: a door refuses when the id holds other text than the host expects.
  const hash = xxh3Text(psalm);
  eq(galley.sourceHash("PSA") === hash, true, "sourceHash is xxh3Text of the text");
  eq(galley.places("PSA", { expect: hash }).length, 5, "the expected text answers");
  galley.update("PSA", psalm.replace("lest", "lest we"));
  for (const [door, call] of [
    ["places", (o) => galley.places("PSA", o)],
    ["resolve", (o) => galley.resolve("PSA", { kind: "book" }, o)],
    ["addressAt", (o) => galley.addressAt("PSA", 0, o)],
    ["srsLocate", (o) => galley.srsLocate("PSA", "PSA 7:1", o)],
    ["srsAt", (o) => galley.srsAt("PSA", 0, 1, o)],
  ]) {
    throws(() => call({ expect: hash }), "is stale", `${door} refuses a stale id`);
    call({ expect: galley.sourceHash("PSA") });
  }
  throws(() => galley.places("PSA", { expect: "abc" }), "bigint", "expect is a bigint");

  // What a full book costs, by id and through the text.
  const psalms = readFileSync(resolve(here, "../../testData/exampleCorpora/examples.bsb/19PSABSB.usfm"), "utf8");
  galley.update("PSALMS", psalms);
  const time = (call, n = 20) => {
    call();
    const start = performance.now();
    for (let i = 0; i < n; i += 1) call();
    return (performance.now() - start) / n;
  };
  const byId = time(() => galley.places("PSALMS", { utf16: true }));
  const byText = time(() => galley.placesOf(psalms, { utf16: true }));
  const count = galley.places("PSALMS").length;
  console.log(`places: ${count} in BSB Psalms, ${byId.toFixed(2)} ms by id, ${byText.toFixed(2)} ms through the text`);
  eq(galley.placesOf(psalms, { utf16: true }), galley.places("PSALMS", { utf16: true }), "Psalms: placesOf and places agree");
  galley.free();
}

// --- the standard in full ----------------------------------------------------
{
  const { parseSrs } = doors;
  const galley = new Galley();
  const jonah = readFileSync(resolve(here, "../../testData/exampleCorpora/examples.bsb/32JONBSB.usfm"), "utf8");
  const at = (ref, opts) => {
    const got = galley.srsLocateOf(jonah, ref, opts);
    return got.found ? jonah.slice(got.found.from, got.found.to) : got;
  };
  // usfmtc's cases; the file is ASCII apart from its quotes, so offsets are UTF-16 too.
  eq(at("JON 2:8!2-4", { utf16: true }), "who cling to", "JON 2:8!2-4");
  eq(at("JON 1:4!s1!2", { utf16: true }), "Great", "a heading's word");
  eq(at("JON 1:13!f!5", { utf16: true }), "dug", "a footnote's word");
  eq(at("JON 1:99"), { missing: "verse" }, "a verse the text lacks");
  eq(at("GEN 1:1"), { unsupported: "book" }, "another book");
  eq(at("JON 1:1!"), { error: "expected-step", at: 8 }, "a parse error");

  const found = galley.srsLocateOf(jonah, "JON 2:8!2-4", { utf16: true }).found;
  eq(galley.srsAtOf(jonah, found.from, found.to, { utf16: true }), "JON 2:8!2-4", "srsAt spells it back");
  eq(galley.srsAtOf(jonah, 0, 3), undefined, "outside any verse");

  eq(parseSrs("en+ulb.ROM 9:1!3-5"), {
    ok: true,
    canonical: "en+ulb.ROM 9:1!3-5",
    within: [{ kind: "verses", book: "ROM", from: { chapter: 9, verse: 1 }, to: { chapter: 9, verse: 1 } }],
  }, "parseSrs");
  eq(parseSrs("NEH t:Israel/6/1").within, [null], "a named reference lies in no basic one");
  galley.free();
}

console.log(failures === 0 ? "reference conformance: OK" : `reference conformance: ${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
