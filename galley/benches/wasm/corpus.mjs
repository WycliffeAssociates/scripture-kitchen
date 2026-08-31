/**
 * What a COLD PROJECT LOAD costs in wasm — single-threaded, because the web
 * has no worker pool without SharedArrayBuffer and COOP/COEP headers.
 *
 *   wasm-pack build --target nodejs --release --out-dir pkg-node   # in onion-wasm/
 *   node corpus.mjs <pkg-dir> <corpus-dir>
 *
 * Two decisions turn on these numbers:
 *
 *   PERSIST THE PARSE?  No. A plated corpus is 1.22x its source (5.23 MB
 *   against 4.30 MB for en_ulb), and the editor needs the source anyway — so a
 *   persisted dish is ADDITIVE I/O, not a substitute, and the source still has
 *   to be hashed to know the dish is valid. What is worth persisting is
 *   validated findings: human judgement, not derivable, tiny.
 *
 *   WHAT DOES THE COLD DOOR RETURN?  Diagnostics are most of the cost. If a
 *   project panel needs per-book counts at load it pays for them; if counts can
 *   be lazy, the cold load is about one frame.
 *
 * Reported as `fastest` for the same reason the other harnesses here do: the
 * floor is the signal, everything above it is the machine.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const pkg = resolve(process.argv[2] ?? "pkg-node");
const dir = resolve(process.argv[3] ?? "../../../testData/exampleCorpora/en_ulb");
const { parse } = await import(join(pkg, "onion_wasm.js"));

const texts = readdirSync(dir)
  .filter((n) => /\.usfm$/i.test(n))
  .sort()
  .map((n) => readFileSync(join(dir, n), "utf8").replace(/\r\n/g, "\n"));
const bytes = texts.reduce((a, t) => a + Buffer.byteLength(t), 0);

function time(label, fn) {
  for (let i = 0; i < 2; i++) fn();
  let best = Infinity;
  for (let r = 0; r < 9; r++) {
    const t = performance.now();
    fn();
    best = Math.min(best, performance.now() - t);
  }
  const frames = best / 16.67;
  console.log(
    `  ${label.padEnd(44)} ${best.toFixed(1).padStart(7)} ms   ${frames.toFixed(1).padStart(4)} frames` +
      `   ${(bytes / 1048576 / (best / 1000)).toFixed(0).padStart(4)} MB/s`,
  );
  return best;
}

const run = (diagnostics, toc, utf16) => () => {
  let n = 0;
  for (const t of texts) n += parse(t, diagnostics, toc, utf16).length;
  return n;
};

console.log(`wasm, single-threaded — ${texts.length} books, ${(bytes / 1048576).toFixed(2)} MB\n`);
// One option at a time off the full set, so a difference names ONE thing.
// Comparing "everything" against "tree only" moves three variables at once and
// attributes their sum to whichever is named first.
const all = time("everything (diagnostics + toc + utf16)", run(true, true, true));
const noLint = time("…minus diagnostics", run(false, true, true));
const noToc = time("…minus toc", run(true, false, true));
const noUtf16 = time("…minus utf16 (byte offsets)", run(true, true, false));
const tree = time("tree only (none of the three)", run(false, false, false));

const share = (without) => (((all - without) / all) * 100).toFixed(0);
console.log(
  `\n  of a full cold load: diagnostics ${share(noLint)}%, toc ${share(noToc)}%, ` +
    `utf16 ${share(noUtf16)}%.\n  the tree alone is ${(tree / 16.67).toFixed(1)} frames.`,
);
