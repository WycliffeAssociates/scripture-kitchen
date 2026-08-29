// In-process wasm timing. hyperfine is unusable here: node's ~40ms startup
// swamps a 17ms operation. Best-of-N with performance.now(), matching how the
// native Divan numbers were taken (Divan's `fastest` column).
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const onion = require(process.argv[2] + '/onion_wasm.js');

const CORPUS = process.argv[3];
const BOOKS = [
  ['en_ult/19-PSA.usfm', 5122298],
  ['en_ulb/19-PSA.usfm', 272592],
  ['en_ulb/41-MAT.usfm', 134023],
  ['en_ulb/66-JUD.usfm', 3946],
];

const best = (runs, f) => {
  let min = Infinity;
  for (let i = 0; i < runs; i++) {
    const t = performance.now();
    f();
    min = Math.min(min, performance.now() - t);
  }
  return min;
};

const wants = onion.wantsAll();
console.log(`node ${process.version}  wants=${wants}`);
console.log(`${'book'.padEnd(22)}${'bytes'.padStart(10)}${'wasm ms'.padStart(11)}${'MB/s'.padStart(9)}`);
for (const [rel] of BOOKS) {
  const text = readFileSync(`${CORPUS}/${rel}`, 'utf8');
  const bytes = Buffer.byteLength(text, 'utf8');
  // Warm the JIT and the wasm instance before measuring.
  for (let i = 0; i < 3; i++) onion.analyze(text, wants, undefined, undefined);
  const runs = bytes > 1_000_000 ? 9 : 20;
  const ms = best(runs, () => onion.analyze(text, wants, undefined, undefined));
  const rate = (bytes / 1048576) / (ms / 1000);
  console.log(`${rel.padEnd(22)}${String(bytes).padStart(10)}${ms.toFixed(2).padStart(11)}${rate.toFixed(1).padStart(9)}`);
}
