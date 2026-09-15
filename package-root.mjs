/**
 * The repo-root `package.json`, derived from the two inner ones.
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
 * `./onion` for a consumer that wants the engine alone.
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
  // Never published to a registry: consumers install from a git tag or from the
  // tarball on the Release. `private` blocks `npm publish` and nothing else —
  // a git install and `npm pack` both work through it.
  private: true,
  license: "UNLICENSED",
  // `files` takes bare paths, not "./"-prefixed ones.
  files: PACKAGES.flatMap(({ dir, pkg }) => pkg.files.map((f) => `${dir}/${f}`)),
  sideEffects: PACKAGES.flatMap(({ dir, pkg }) => push(dir)(pkg.sideEffects)),
  exports,
};

if (import.meta.filename === process.argv[1]) {
  process.stdout.write(JSON.stringify(root, null, 2) + "\n");
}
