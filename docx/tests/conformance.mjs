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

const back = docx.importDocx(out.docx);
check(back.books.length === 1 && back.books[0].code === "PSA", "importDocx finds the one book");
check(docx.isExact(back.report), "every paragraph and run was in our styles", back.report);
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

// --- not a .docx -------------------------------------------------------------------
let threw = false;
try {
  docx.importDocx(new TextEncoder().encode("hello"));
} catch (e) {
  threw = String(e).includes("zip");
}
check(threw, "importDocx throws on bytes that are not a zip");

function same(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

if (failures > 0) {
  console.log(`\n${failures} failed`);
  process.exit(1);
}
console.log("\nall passed");
