/**
 * bttwriter across the wall: `bttwriter.ts` as shipped, over the built module.
 *
 *   ./bttwriter/build.sh                    (or ./wasm-build.sh bttwriter)
 *   node bttwriter/tests/conformance.mjs
 *
 * Reads the five fixture repos through `isomorphicGitSource`, so the async
 * driver loop and the adapter run as Sefer will run them. The `git` handed to
 * it is a stand-in with isomorphic-git's two functions (`resolveRef`,
 * `readObject`), backed by the `git` command: the package doesn't depend on
 * isomorphic-git, and neither does this test. Then: each book at HEAD is the
 * stitched fixture, each replay is Sefer's head, and a consolidated pack is
 * one git accepts. Needs the `git` command; a missing fixture fails.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const here = import.meta.dirname;
const fixtures = resolve(here, "../fixtures");
const bttwriter = await import(resolve(here, "../bttwriter.ts"));
const bento = await import(resolve(here, "../../bento/bento.ts"));
bttwriter.initSync({ module: readFileSync(resolve(here, "../pkg-web/bttwriter_bg.wasm")) });
bento.initSync({ module: readFileSync(resolve(here, "../../bento/pkg-web/bento_bg.wasm")) });

let failures = 0;
const check = (ok, what, detail) => {
  if (ok) console.log(`ok    ${what}`);
  else {
    failures += 1;
    console.log(`FAIL  ${what}${detail === undefined ? "" : `\n      ${JSON.stringify(detail)}`}`);
  }
};

const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("GIT_")));
const git = (args, opts = {}) => execFileSync("git", args, { env, maxBuffer: 1 << 28, ...opts });

// Sefer's own replay at 326f02c8 (tests/replay.rs pins the same ids).
const REPOS = [
  ["kpass_tmf_3jn_text_reg", "kpass", "350ebd4bbf30573ec4b0f253b239261476f3dece"],
  ["bahasatech.indotimur_kps-x-kaigar_3jn_text_reg", "bahasatech.indotimur", "710cac6f2a667b1bb57af3a4db7d979b4ef6549f"],
  ["chunga_major_bem-x-ngumbo_2ch_text_reg", "chunga_major", "cc2a00c7e2c86a2bc3a11e35f5be004776efdb18"],
  ["andriatiana_wa_msh_job_text_udb", "andriatiana_wa", "ffe1509052ab8fd71dba2135fea9a367f10a55a3"],
  ["bahasatech.indotimur_saw_isa_text_reg", "bahasatech.indotimur", "903023b30335d7d1ddf3c232f524bcd714ba1793"],
];
const FOUR_BOOKS = "c9f5bd89dc8e082bed8f05a5aa80e70fdf70de8d";
const NOW = 1_793_836_800;

/** isomorphic-git's two functions, over a clone, from one `cat-file` of every object. */
const standIn = (dir) => {
  const objects = new Map();
  const all = git(["-C", dir, "cat-file", "--batch-all-objects", "--batch"]);
  for (let at = 0; at < all.length; ) {
    const nl = all.indexOf(10, at);
    const [oid, type, size] = all.subarray(at, nl).toString().split(" ");
    const start = nl + 1;
    objects.set(oid, { type, object: new Uint8Array(all.subarray(start, start + Number(size))) });
    at = start + Number(size) + 1;
  }
  let reads = 0;
  return {
    reads: () => reads,
    git: {
      resolveRef: async ({ ref }) => git(["-C", dir, "rev-parse", ref]).toString().trim(),
      readObject: async ({ oid, format }) => {
        reads += 1;
        if (format !== "content") throw new Error(`format ${format}`);
        const found = objects.get(oid);
        if (found === undefined) throw new Error(`NotFoundError: ${oid}`);
        return found;
      },
    },
  };
};

const work = mkdtempSync(join(tmpdir(), "bttwriter-conformance-"));
const histories = [];
try {
  for (const [repo, owner, seferHead] of REPOS) {
    const bundle = join(fixtures, "repos", `${repo}.bundle`);
    if (!existsSync(bundle)) throw new Error(`missing ${bundle}`);
    const dir = join(work, repo);
    git(["-c", "advice.detachedHead=false", "clone", "--quiet", bundle, dir], { stdio: "ignore" });
    const { git: stand, reads } = standIn(dir);
    const source = bttwriter.isomorphicGitSource({ git: stand, fs: { readFile: async () => "" }, dir });
    const history = await bttwriter.readHistory(source, { source: `${owner}/${repo.slice(owner.length + 1)}` });
    // The fixtures were stitched by the hub, whose fallback title is the book
    // code; Sefer's (and so replay's) is canon's name. Only kpass has no
    // front/title.txt, so only its title lines differ.
    const book = history.project.book;
    const stitched = readFileSync(join(fixtures, "stitched", `${repo}.usfm`), "utf8").replace(
      new RegExp(`^(\\\\(?:h|toc1|toc2|mt1?) )${book}$`, "gm"),
      `$1${history.title}`,
    );
    check(history.usfm() === stitched, `${repo}: read through isomorphicGitSource, the book at HEAD is the stitched fixture`);
    // No history: the checked-out files alone stitch to the same book.
    const files = new Map(
      git(["-C", dir, "ls-files", "-z"]).toString().split("\0").filter(Boolean)
        .map((path) => [path, new Uint8Array(readFileSync(join(dir, path)))]),
    );
    check(bttwriter.stitch(files, book) === history.usfm(), `${repo}: stitch over the files equals the history's last save`);
    // bento answers the same project from the manifest and the folder name alone.
    const { name: _, ...project } = history.project;
    const fromManifest = bento.writerProject(files.get("manifest.json"), repo);
    check(JSON.stringify(fromManifest) === JSON.stringify(project), `${repo}: bento.writerProject is the history's project`, fromManifest);
    check(history.saves > 0 && reads() > 0, `${repo}: ${history.saves} saves, ${reads()} objects read`);
    const progress = history.progress(NOW);
    check(progress.length === history.saves && progress.every(([at]) => at > 1_420_070_400), `${repo}: progress per save`);

    // Each alone: a second read, since replay uses a history up.
    const again = await bttwriter.readHistory(bttwriter.isomorphicGitSource({ git: stand, fs: {}, dir }), {
      source: history.source,
    });
    const alone = bttwriter.replay([again], { now: NOW });
    check(alone?.head === seferHead, `${repo}: replayed alone, Sefer's head`, alone?.head);
    let usedUp;
    try {
      again.usfm();
    } catch (e) {
      usedUp = e;
    }
    check(usedUp !== undefined, `${repo}: a replayed history is used up, and says so`);
    check(again.spent && !history.spent, `${repo}: spent says which`);
    histories.push(history);
  }

  check(histories[0].project.book === "3JN" && histories[0].project.resource === "reg", "project from manifest.json");

  // stitch's own edges: text keys, a fallback title, a title given, junk ignored.
  const tiny = [["front/title.txt", ""], ["01/01.txt", "\\v 1 a"], ["manifest.json", "{}"], ["01/02.txt", new Uint8Array([0xff])]];
  const romans = bttwriter.stitch(tiny, "ROM");
  check(romans.startsWith("\\id ROM\n") && romans.includes("\\h Romans\n") && romans.includes("\\v 1 a"), "stitch: canon's name stands in", romans);
  check(bttwriter.stitch(tiny, "ROM", { title: "Warumi" }).includes("\\h Warumi\n"), "stitch: a title given");
  let refused;
  try { bttwriter.stitch(tiny, "rom"); } catch (e) { refused = String(e.message ?? e); }
  check(refused?.includes("book code"), "stitch: a book is a code", refused);

  // free: a staged history that will never be replayed lets go of its memory.
  histories[1].free();
  check(histories[1].spent, "free leaves a history spent");
  histories[1].free();
  check(histories[2].title.length > 0, "a title", histories[2].title);

  histories.splice(1, 1); // one history per book
  const project = {
    title: "Baibel",
    abbreviation: "reg",
    language: { tag: "tpi", name: "Tok Pisin", direction: "ltr" },
    sources: [],
    contributors: ["Mary Kila"],
    publisher: "Wycliffe Associates",
    creator: "Bible translation community",
    rights: "CC BY-SA 4.0",
    checkingLevel: "1",
    copyright: "Licensed under CC BY-SA 4.0.",
    generator: { name: "bttwriter conformance", version: "0" },
    createdAt: "2026-11-05T00:00:00Z",
    repository: "Tech_Advance/tpi_reg",
  };
  const who = { name: "Mary Kila", email: "mary@example.org", at: NOW, offset: 600 };
  const done = bttwriter.consolidate(histories, { now: NOW }, project, { author: who, message: "Four books from Writer\n" });
  check(done.commits === 1484 && done.books.length === 4, "consolidated: 1484 commits, four books", done.books);
  check([...done.manifests.keys()].join() === "metadata.json,manifest.yaml", "and both manifests");

  const bare = join(work, "consolidated.git");
  git(["init", "--quiet", "--bare", bare], { stdio: "ignore" });
  writeFileSync(join(bare, "objects/pack", `${done.name}.pack`), done.pack);
  writeFileSync(join(bare, "objects/pack", `${done.name}.idx`), done.idx);
  git(["--git-dir", bare, "update-ref", "refs/heads/main", done.head]);
  git(["--git-dir", bare, "fsck", "--full", "--strict", "--no-dangling"]);
  check(true, "git fsck --strict accepts the consolidated pack");
  const parent = git(["--git-dir", bare, "rev-parse", "main~1"]).toString().trim();
  check(parent === FOUR_BOOKS, "the closing commit sits on Sefer's four-book line", parent);
  const metadata = new Uint8Array(git(["--git-dir", bare, "cat-file", "blob", "main:metadata.json"]));
  const read = bento.read("metadata.json", metadata);
  check(
    read.findings.filter((f) => f.check !== "book-title-translated").length === 0 &&
      read.manifest.books.map((b) => b.book).join() === "2CH,JOB,ISA,3JN",
    "bento reads the consolidated Burrito clean",
    read.findings,
  );
  check(bttwriter.replay([], { now: NOW }) === undefined, "replay of nothing is undefined");
} finally {
  rmSync(work, { recursive: true, force: true });
}

console.log(failures === 0 ? "\nall passed" : `\n${failures} failed`);
process.exit(failures === 0 ? 0 : 1);
