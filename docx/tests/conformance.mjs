/**
 * docx across the wall: `docx.ts` as shipped, over the built module.
 *
 *   ./docx/build.sh                        (or ./wasm-build.sh docx)
 *   node docx/tests/conformance.mjs
 *
 * Node strips the types, so this imports `docx.ts` itself, no build step.
 * The Rust tests pin the round trip over the whole test tier; this asserts
 * the same things reach JavaScript, in the shapes `docx.ts` declares.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const here = import.meta.dirname;
const root = resolve(here, "../..");
const docx = await import(resolve(here, "../docx.ts"));
docx.initSync({ module: readFileSync(resolve(here, "../pkg-web/docx_bg.wasm")) });

let failures = 0;
const check = (ok, what, detail) => {
  if (ok) console.log(`ok    ${what}`);
  else {
    failures += 1;
    console.log(`FAIL  ${what}${detail === undefined ? "" : `\n      ${JSON.stringify(detail)}`}`);
  }
};
const text = (path) => readFileSync(resolve(root, path), "utf8");

// --- one book, there and back ----------------------------------------------------
const psalms = text("testData/exampleCorpora/examples.bsb/19PSABSB.usfm");
const out = docx.exportBook(psalms);
check(out.name === "PSA.docx" && out.book === "PSA", "exportBook names the file by its \\id", out.name);
check(out.docx instanceof Uint8Array && out.docx[0] === 0x50 && out.docx[1] === 0x4b, "the .docx is a zip");
check(out.unsupported.length === 0, "BSB Psalms has nothing Word cannot carry", out.unsupported);

const back = docx.importDocument(out.docx);
check(back.books.length === 1 && back.books[0].code === "PSA", "importDocument finds the one book");
check(back.report.exact && back.report.guesses.length === 0, "every paragraph and run was in our styles", back.report);
check(back.books[0].source?.gitBlobSha1?.length === 40, "the source's git blob id comes back", back.books[0].source);
check(back.books[0].usfm.includes("\\c 150\n"), "the USFM comes back to its last chapter");
check(Array.isArray(back.books[0].comments) && back.books[0].comments.length === 0, "no comments, an empty list");

// --- what Word cannot carry is said, not hidden -----------------------------------
const aligned = docx.exportBook('\\id MRK\n\\c 1\n\\p\n\\v 1 \\w word|lemma="x"\\w*\n');
check(
  aligned.unsupported.some((u) => u.what.includes("lemma") && u.at === "MRK 1:1"),
  "an attribute is reported where it sits",
  aligned.unsupported,
);

// --- many books, one file each -----------------------------------------------------
const many = docx.exportBooks(["\\id GEN\n\\c 1\n\\p\n\\v 1 a\n", "\\id EXO\n\\c 1\n\\p\n\\v 1 b\n"], { rtl: true, font: "Scheherazade" });
check(same([...many.files.keys()], ["GEN.docx", "EXO.docx"]), "exportBooks: one .docx per book, by name", [...many.files.keys()]);

// --- a draft in no style of ours: document-import's rules ---------------------------
const fixture = (name) => new Uint8Array(readFileSync(resolve(here, "../fixtures", name)));
const golden = text("docx/fixtures/titus.document-import.usfm");
const reading = docx.readDocument(fixture("master-titus.docx"));
const titus = reading.render({ book: "TIT" });
check(titus.books[0].usfm === golden, "a Word draft reads as document-import reads it");
check(titus.report.detected === "superscript" && !titus.report.exact, "the report says how verses were found, and that it guessed", {
  detected: titus.report.detected,
  exact: titus.report.exact,
});
check(
  same(titus.report.chapters.map((c) => [c.chapter, c.verses, c.missing.length]), [[1, 16, 0], [2, 15, 0], [3, 15, 0]]),
  "every verse of Titus is found",
);
const rem = reading.render({ book: "TIT", front: "drop" });
check(!rem.books[0].usfm.includes("\\rem"), "render reruns with other settings on the same reading");
reading.free();
const odt = docx.importDocument(fixture("titus.odt"));
check(
  same(odt.books[0].comments.map((t) => [t.ref, t.start.ref, t.start.offset, t.end.offset, t.anchorText]), [
    ["1:1", "1:1", 23, 40, "and an apostle of"],
  ]),
  "a Writer comment lands on its words",
  odt.books[0].comments,
);
let badSetting = false;
try {
  docx.importDocument(fixture("titus.odt"), { verses: "sometimes" });
} catch (e) {
  badSetting = String(e).includes("verses");
}
check(badSetting, "an unknown setting value is an error that names the setting");

// --- the template -----------------------------------------------------------------
const dotx = docx.template();
const fromTemplate = docx.importDocument(dotx);
check(fromTemplate.report.exact && fromTemplate.books[0].code === "XXX", "the template reads back as its starter book, exactly");

// --- not a document -----------------------------------------------------------------
let threw = false;
try {
  docx.importDocument(new TextEncoder().encode("hello"));
} catch (e) {
  threw = String(e).includes("zip");
}
check(threw, "importDocument throws on bytes that are not a zip");

function same(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

if (failures > 0) {
  console.log(`\n${failures} failed`);
  process.exit(1);
}
console.log("\nall passed");
