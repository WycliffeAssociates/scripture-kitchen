/**
 * The repo-root `package.json`, derived from this one.
 *
 *     node onion-wasm/package-root.mjs > package.json
 *
 * npm cannot install a subdirectory of a git repo, so the root file is what
 * makes `npm i github:org/usfm_onion#v0.1.0` reach these builds. It is this
 * package's map with every relative path pushed down one directory — derived
 * rather than written twice because `sideEffects` drifting is silent: a bundler
 * that does not see it tree-shakes the wasm glue out of a consumer's app.
 */
import { readFileSync } from "node:fs";

const DIR = "onion-wasm";
const here = import.meta.dirname;
const inner = JSON.parse(readFileSync(`${here}/package.json`, "utf8"));

const push = (v) =>
  typeof v === "string"
    ? v.replace(/^\.\//, `./${DIR}/`)
    : Array.isArray(v)
      ? v.map(push)
      : Object.fromEntries(Object.entries(v).map(([k, x]) => [k, push(x)]));

export const root = {
  name: "@wycliffeassociates/usfm-onion",
  version: inner.version,
  description: inner.description,
  type: inner.type,
  // Never published to a registry: consumers install from a git tag or from the
  // tarball on the Release. `private` blocks `npm publish` and nothing else —
  // a git install and `npm pack` both work through it.
  private: true,
  license: "UNLICENSED",
  // `files` takes bare paths, not "./"-prefixed ones.
  files: inner.files.map((f) => `${DIR}/${f}`),
  sideEffects: push(inner.sideEffects),
  exports: push(inner.exports),
};

if (import.meta.filename === process.argv[1]) {
  process.stdout.write(JSON.stringify(root, null, 2) + "\n");
}
