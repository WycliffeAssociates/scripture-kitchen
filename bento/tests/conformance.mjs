/**
 * bento across the wall: `bento.ts` as shipped, over the built module.
 *
 *   ./bento/build.sh                       (or ./wasm-build.sh bento)
 *   node bento/tests/conformance.mjs
 *
 * Node strips the types, so this imports `bento.ts` itself, no build step.
 * The Rust tests pin what each rule does; this asserts the same things reach
 * JavaScript, in the shapes `bento.ts` declares, and that its own code
 * (readOrThrow, the Standard Schema objects, the argument packing) holds.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const here = import.meta.dirname;
const root = resolve(here, "../..");
const bento = await import(resolve(here, "../bento.ts"));
bento.initSync({ module: readFileSync(resolve(here, "../pkg-web/bento_bg.wasm")) });

let failures = 0;
const check = (ok, what, detail) => {
  if (ok) console.log(`ok    ${what}`);
  else {
    failures += 1;
    console.log(`FAIL  ${what}${detail === undefined ? "" : `\n      ${JSON.stringify(detail)}`}`);
  }
};
const bytes = (path) => new Uint8Array(readFileSync(resolve(root, path)));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const enc = new TextEncoder();

// --- find and read -----------------------------------------------------------
check(
  same(bento.find(["README.md", "org-manifest.json", "manifest.yaml", "metadata.json"]), {
    name: "metadata.json",
    format: "burrito",
  }),
  "find prefers the Burrito and ignores look-alikes",
);
check(bento.find(["LICENSE"]) === undefined, "find: no manifest is undefined");

const ulb = bento.read("manifest.yaml", bytes("testData/exampleCorpora/en_ulb/manifest.yaml"));
check(ulb.manifest?.books.length === 66 && ulb.findings.length === 0, "the catalog's en_ulb reads clean, 66 books", ulb.findings);
check(ulb.manifest?.books[39].book === "MAT" && ulb.manifest.books[39].sort === 40, "MAT is 40");
check(ulb.detail?.format === "rc" && ulb.detail.document.dublin_core?.identifier === "ulb", "detail is the file's own value");
check(ulb.manifest?.kind.subject === "Bible" && ulb.manifest.kind.domain === "scripture", "kind");

const sefer = bento.read("metadata.json", bytes("bento/fixtures/burrito/sefer_written_tpi_reg.json"));
check(
  same(
    sefer.findings.map((f) => `${f.check} ${f.at}`),
    [
      "manifest-valid /meta/dateCreated",
      "manifest-valid /meta/generator/softwareVersion",
      "manifest-valid /identification/primary",
      "manifest-valid /copyright",
    ],
  ),
  "a Burrito as Sefer writes it: four findings",
  sefer.findings,
);

const broken = bento.read("manifest.yaml", enc.encode("a: [1, 2\n"));
check(broken.manifest === undefined && broken.findings[0]?.check === "manifest-unreadable", "unreadable: no manifest, one error");
let threw;
try {
  bento.readOrThrow("manifest.yaml", enc.encode("a: [1, 2\n"));
} catch (e) {
  threw = e;
}
check(threw instanceof bento.ManifestError && threw.findings.length === 1, "readOrThrow throws a ManifestError");
check(bento.readOrThrow("metadata.json", bytes("bento/fixtures/burrito/sefer_written_tpi_reg.json")).manifest.language.tag === "tpi", "readOrThrow passes warnings");

// --- create, check, edit -----------------------------------------------------
const book = (code, title) => ({ book: code, title, bytes: enc.encode(`\\id ${code}\n\\c 1\n\\v 1 …\n`) });
const project = (books) => ({
  title: "Baibel",
  abbreviation: "reg",
  language: { tag: "tpi", name: "Tok Pisin", direction: "ltr" },
  books,
  sources: [{ language: "en", resource: "ulb", version: "12" }],
  contributors: ["Mary Kila"],
  publisher: "Wycliffe Associates",
  creator: "Bible translation community",
  rights: "CC BY-SA 4.0",
  checkingLevel: "1",
  copyright: "Licensed under CC BY-SA 4.0.",
  generator: { name: "Sefer", version: "1.4.0" },
  createdAt: "2026-11-05T14:30:00Z",
  repository: "Tech_Advance/tpi_reg",
});
const created = bento.createBible(project([book("ROM", "Rom"), book("MAT", "Matyu")]));
check(same([...created.keys()], ["metadata.json", "manifest.yaml"]), "createBible writes both");
const back = bento.read("metadata.json", created.get("metadata.json"));
check(back.findings.length === 0 && same(back.manifest.books.map((b) => b.book), ["MAT", "ROM"]), "created reads back clean, in canon order", back.findings);
check(same(back.manifest.name, { en: "Baibel", tpi: "Baibel" }), "names under en and the tag");

const files = new Map([
  ["41-MAT.usfm", book("MAT", "").bytes],
  ["46-ROM.usfm", book("ROM", "").bytes],
]);
check(bento.check("metadata.json", created.get("metadata.json"), { name: "Tech_Advance/tpi_reg", files }).length === 0, "check: the files match");
const stale = bento.check("metadata.json", created.get("metadata.json"), { files: new Map([["41-MAT.usfm", enc.encode("x")]]) });
check(same(stale.map((f) => f.check), ["ingredient-checksum", "ingredient-exists"]), "check: a changed file and a missing one", stale);

const edited = bento.edit(created, { addBooks: [book("MRK", "Mak")] });
const three = bento.createBible(project([book("MAT", "Matyu"), book("MRK", "Mak"), book("ROM", "Rom")]));
check(
  same([...edited.files.get("metadata.json")], [...three.get("metadata.json")]),
  "create + edit(add MRK) == create with MRK, byte for byte",
);
const saved = enc.encode("\\id MAT changed\n");
const summed = bento.edit(created, { checksums: new Map([["41-MAT.usfm", saved]]) });
check(same([...summed.files.keys()], ["metadata.json"]), "checksums change only the Burrito");
check(
  bento.read("metadata.json", summed.files.get("metadata.json")).detail.document.ingredients["41-MAT.usfm"].checksum.md5 ===
    bento.fingerprint(saved).md5,
  "the md5 is the saved file's",
);
let refused;
try {
  bento.edit(created, { addBooks: [book("ROM", "Rom")] });
} catch (e) {
  refused = e;
}
check(refused !== undefined && /listed already/.test(String(refused)), "adding a listed book throws");
check(same(bento.fingerprint(enc.encode("abc")), { md5: "900150983cd24fb0d6963f7d28e17f72", size: 3 }), "fingerprint");

// --- Standard Schema ---------------------------------------------------------
const doc = JSON.parse(new TextDecoder().decode(created.get("metadata.json")));
const ok = bento.schemas.burrito["~standard"].validate(doc);
check(ok.issues === undefined && ok.value === doc, "schemas.burrito: a created Burrito is valid");
const bad = bento.schemas.burrito["~standard"].validate(JSON.parse(new TextDecoder().decode(bytes("bento/fixtures/burrito/sefer_written_tpi_reg.json"))));
check(
  bad.issues?.length === 4 && same(bad.issues[0].path, ["meta", "dateCreated"]),
  "schemas.burrito: Sefer's has four issues, with paths",
  bad.issues,
);
const rcValue = { dublin_core: { identifier: "reg", title: "Baibel", language: { identifier: "tpi" } }, projects: [] };
check(bento.schemas.rc["~standard"].validate(rcValue).value === rcValue, "schemas.rc reads a value through YAML");
check(bento.schemas.rc["~standard"].validate(7).issues?.length === 1, "schemas.rc: a number is an issue");

// --- writerProject: a Writer manifest's book project ------------------------
{
  const manifest = (fields) => enc.encode(JSON.stringify({
    target_language: { id: "tmf", direction: "ltr" },
    project: { id: "3jn", name: "3 John" },
    source_translations: [{ language_id: "en", resource_id: "ulb", version: 12 }],
    translators: ["kpass"],
    ...fields,
  }));
  check(
    same(bento.writerProject(manifest({ resource: { id: "reg" } })), {
      book: "3JN",
      language: { id: "tmf", direction: "ltr" },
      resource: "reg",
      sources: [{ relation: "source", language: "en", resource: "ulb", version: "12" }],
      translators: ["kpass"],
    }),
    "writerProject: book, language, resource, sources, translators",
    bento.writerProject(manifest({ resource: { id: "reg" } })),
  );
  check(bento.writerProject(manifest({}), "tmf_3jn_text_udb")?.resource === "udb", "writerProject: the folder name supplies a missing resource");
  check(bento.writerProject(manifest({})) === undefined, "writerProject: no resource and no folder is undefined");
  check(bento.writerProject(manifest({ project: { id: "zzz" }, resource: "reg" })) === undefined, "writerProject: a book outside the table is undefined");
  check(bento.writerProject(enc.encode("[]")) === undefined, "writerProject: not a manifest is undefined");
}

console.log(failures === 0 ? "\nall passed" : `\n${failures} failed`);
process.exit(failures === 0 ? 0 : 1);
