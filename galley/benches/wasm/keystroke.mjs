// The Rust galley bench, across the wall. Same shape: a keystroke ring so every
// measured call is a NEVER-SEEN text, one dirty chapter, all chapters but one
// served from the cache.
//
// In-process best-of-N, not hyperfine: node's ~40ms startup would swamp the
// operation. Matches Divan's `fastest` column, which is how the native numbers
// in this report were taken.
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const g = require(process.argv[2] + '/usfm_galley.js');
const CORPUS = process.argv[3];

const BOOKS = ['en_ult/19-PSA.usfm', 'en_ulb/19-PSA.usfm', 'en_ulb/41-MAT.usfm', 'en_ulb/66-JUD.usfm'];
const WANTS = g.wantsAll();
const RING = 40;

const best = (runs, f) => {
  let min = Infinity;
  for (let i = 0; i < runs; i++) { const t = performance.now(); f(); min = Math.min(min, performance.now() - t); }
  return min;
};

console.log(`node ${process.version}   wasm ${process.argv[2].split('/').pop()}\n`);
console.log(`${'book'.padEnd(22)}${'bytes'.padStart(9)}${'fresh ms'.padStart(10)}${'folded ms'.padStart(11)}${'ratio'.padStart(8)}${'entries'.padStart(9)}${'miss/key'.padStart(10)}`);

for (const rel of BOOKS) {
  const text = readFileSync(`${CORPUS}/${rel}`, 'utf8');
  const bytes = Buffer.byteLength(text, 'utf8');
  const mid = Math.floor(text.length / 2);
  const at = mid + text.slice(mid).indexOf(' ');
  const typed = Array.from({ length: RING }, (_, n) =>
    text.slice(0, at) + 'x'.repeat(n + 1) + text.slice(at));

  // Parity across the wall before timing anything.
  const cache = new g.Galley();
  cache.analyze(text, WANTS);
  const a = JSON.stringify(cache.analyze(typed[0], WANTS));
  const b = JSON.stringify(g.analyze(typed[0], WANTS, undefined, undefined));
  if (a !== b) { console.error(`PARITY FAILED for ${rel}`); process.exit(1); }

  // One keystroke, one miss — the invariant the Rust bench asserts.
  const before = cache.misses();
  cache.analyze(typed[1], WANTS);
  const missPerKey = cache.misses() - before;

  const runs = bytes > 1_000_000 ? 9 : 20;
  for (let i = 0; i < 3; i++) g.analyze(typed[0], WANTS, undefined, undefined);
  const fresh = best(runs, () => g.analyze(typed[0], WANTS, undefined, undefined));

  const warm = new g.Galley();
  warm.analyze(text, WANTS);
  let i = 2;
  const folded = best(runs, () => { warm.analyze(typed[i % RING], WANTS); i++; });

  console.log(
    `${rel.padEnd(22)}${String(bytes).padStart(9)}${fresh.toFixed(2).padStart(10)}` +
    `${folded.toFixed(2).padStart(11)}${(fresh / folded).toFixed(2).padStart(7)}x` +
    `${String(warm.entryCount()).padStart(9)}${String(missPerKey).padStart(10)}`);
}
