/**
 * The repo-root `package.json`, derived from the inner ones.
 *
 *     node package-root.mjs > package.json
 *
 * npm cannot install a subdirectory of a git repo, so the root file is what
 * makes `npm i github:org/usfm_onion#v0.1.0` reach these builds. It is both
 * inner maps with every relative path pushed down one directory — derived
 * rather than written twice because `sideEffects` drifting is silent: a
 * bundler that does not see it tree-shakes the wasm glue out of a consumer's
 * app.
 *
 * `usfm-galley` is the default export: it is the SUPERSET module, every onion
 * door plus the resident sous handle. `onion-wasm` keeps its own subtree under
 * `./onion` for a consumer that wants the engine alone, and `canon` sits at
 * `./canon`: plain TypeScript with no wasm, so a book table costs no `init()`.
 * `bento` (manifests), `bttwriter` (Writer migration) and `docx` (USFM in Word)
 * are TypeScript over a wasm module each, at `./bento`, `./bttwriter` and
 * `./docx`; a bundler fetches a module only where it's imported, so Sefer
 * pays for bttwriter only on the migration screen.
 */
import { readFileSync } from "node:fs";

const here = import.meta.dirname;
const inner = (dir) => JSON.parse(readFileSync(`${here}/${dir}/package.json`, "utf8"));

/** `./web/wasm` under `onion-wasm` is `./onion/web/wasm` at the root. */
const under = (prefix) => (key) =>
  key === "." ? prefix : key.startsWith("./web") ? `${prefix}${key.slice(1)}` : key;

const PACKAGES = [
  { dir: "galley", pkg: inner("galley"), subpath: (key) => key },
  { dir: "onion-wasm", pkg: inner("onion-wasm"), subpath: under("./onion") },
  { dir: "canon", pkg: inner("canon"), subpath: under("./canon") },
  { dir: "bento", pkg: inner("bento"), subpath: under("./bento") },
  { dir: "bttwriter", pkg: inner("bttwriter"), subpath: under("./bttwriter") },
  // Every docx key sits under ./docx, the template file included.
  { dir: "docx", pkg: inner("docx"), subpath: (key) => (key === "." ? "./docx" : `./docx${key.slice(1)}`) },
];

const push = (dir) => {
  const one = (v) =>
    typeof v === "string"
      ? v.replace(/^\.\//, `./${dir}/`)
      : Array.isArray(v)
        ? v.map(one)
        : Object.fromEntries(Object.entries(v).map(([k, x]) => [k, one(x)]));
  return one;
};

const [{ pkg: galley }] = PACKAGES;
const versions = new Set(PACKAGES.map(({ pkg }) => pkg.version));
if (versions.size !== 1) throw new Error(`inner versions disagree: ${[...versions]}`);

const exports = {};
for (const { dir, pkg, subpath } of PACKAGES) {
  for (const [key, target] of Object.entries(pkg.exports)) {
    const at = subpath(key);
    if (at in exports) throw new Error(`two packages export ${at}`);
    exports[at] = push(dir)(target);
  }
}

export const root = {
  name: "@wycliffeassociates/scripture-kitchen",
  version: galley.version,
  description: galley.description,
  type: galley.type,
  license: "MIT",
  // Published only to the org Nexus (ci.yml's publish job); pinning the registry
  // here keeps a stray `npm publish` off the public one.
  publishConfig: { registry: "https://nexus-registry.walink.org/repository/wa-npm/" },
  // `files` takes bare paths, not "./"-prefixed ones.
  files: PACKAGES.flatMap(({ dir, pkg }) => pkg.files.map((f) => `${dir}/${f}`)),
  sideEffects: PACKAGES.flatMap(({ dir, pkg }) => push(dir)(pkg.sideEffects)),
  exports,
};

if (import.meta.filename === process.argv[1]) {
  process.stdout.write(JSON.stringify(root, null, 2) + "\n");
}
