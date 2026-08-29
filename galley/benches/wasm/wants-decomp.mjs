import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const onion = require(process.argv[2] + '/onion_wasm.js');
const CORPUS = process.argv[3];

// Same bit layout as onion::analyze::wants.
const W = { CHAPTERS:1, BLOCKS:2, NOTES:4, TOKEN_SPANS:8, TEXT_RUNS:16,
            VERSE_ANCHORS:32, DIAGNOSTICS:64, LINES:128 };
const ALL = onion.wantsAll();

const best = (runs, f) => {
  let min = Infinity;
  for (let i = 0; i < runs; i++) { const t = performance.now(); f(); min = Math.min(min, performance.now() - t); }
  return min;
};

for (const rel of ['en_ult/19-PSA.usfm', 'en_ulb/19-PSA.usfm']) {
  const text = readFileSync(`${CORPUS}/${rel}`, 'utf8');
  const bytes = Buffer.byteLength(text, 'utf8');
  const runs = bytes > 1_000_000 ? 9 : 20;
  const t = (w) => { for (let i=0;i<3;i++) onion.analyze(text, w, undefined, undefined);
                     return best(runs, () => onion.analyze(text, w, undefined, undefined)); };

  const floor = t(0);
  const all = t(ALL);
  const noDiag = t(ALL & ~W.DIAGNOSTICS);
  console.log(`\n${rel}  (${bytes} bytes)`);
  console.log(`  wants=0   (lex + header + len_utf16 + empty wall) ${floor.toFixed(2)} ms`);
  console.log(`  wants=ALL                                        ${all.toFixed(2)} ms`);
  console.log(`  ALL minus DIAGNOSTICS                            ${noDiag.toFixed(2)} ms`);
  console.log(`  -> DIAGNOSTICS marginal                          ${(all-noDiag).toFixed(2)} ms`);
  for (const [name, bit] of Object.entries(W)) {
    const m = all - t(ALL & ~bit);
    console.log(`     ${name.padEnd(15)} marginal ${m.toFixed(2)} ms`);
  }
}
