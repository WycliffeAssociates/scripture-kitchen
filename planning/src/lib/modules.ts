/**
 * Reads relative imports under a folder, at build time, for <Modules>.
 * Regex, not a parser: enough for `import … from`, `export … from`,
 * `import()` and `require()`. Bare package imports are skipped (they are not
 * this repo's modules); aliases can be mapped to folders.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";

const CODE = /\.(?:[cm]?[jt]sx?|astro|svelte|vue)$/u;
const SKIP = /(?:^|\/)(?:node_modules|dist|build|\.git|\.astro)(?:\/|$)|\.(?:test|spec)\.[^.]+$/u;
const IMPORT =
  /(?:import|export)\s[^'"`;]*?from\s*["']([^"']+)["']|import\s*\(\s*["']([^"']+)["']\s*\)|require\s*\(\s*["']([^"']+)["']\s*\)|import\s+["']([^"']+)["']/gu;
const EXTENSIONS = ["", ".ts", ".tsx", ".mts", ".js", ".jsx", ".mjs", ".astro", ".svelte", ".vue", "/index.ts", "/index.tsx", "/index.js"];

const walk = (dir: string, skip: RegExp | null): string[] =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (SKIP.test(path) || (skip?.test(path) ?? false)) return [];
    if (statSync(path).isDirectory()) return walk(path, skip);
    return CODE.test(name) ? [path] : [];
  });

const find = (base: string): string | null => {
  for (const extension of EXTENSIONS) {
    const candidate = base + extension;
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
  }
  // `./x.js` written for a `./x.ts` file.
  const swapped = base.replace(/\.js$/u, ".ts");
  return swapped !== base && existsSync(swapped) ? swapped : null;
};

export interface Scanned {
  /** Repo-relative paths. */
  readonly files: readonly string[];
  readonly imports: readonly { from: string; to: string }[];
}

export const scanModules = (
  root: string,
  folders: readonly string[],
  aliases: Readonly<Record<string, string>> = {},
  exclude?: string,
): Scanned => {
  const skip = exclude ? new RegExp(exclude, "u") : null;
  const files = folders.flatMap((folder) => {
    const start = resolve(root, folder);
    return existsSync(start) ? walk(start, skip) : [];
  });
  const inside = new Set(files);
  const imports: { from: string; to: string }[] = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(IMPORT)) {
      const spec = match[1] ?? match[2] ?? match[3] ?? match[4];
      if (spec === undefined) continue;
      let base: string | null = null;
      if (spec.startsWith(".")) base = resolve(dirname(file), spec);
      else {
        const alias = Object.keys(aliases).find((prefix) => spec.startsWith(prefix));
        if (alias !== undefined) base = resolve(root, aliases[alias]!, spec.slice(alias.length));
      }
      if (base === null) continue;
      const target = find(base);
      if (target === null || !inside.has(target) || target === file) continue;
      imports.push({ from: relative(root, file), to: relative(root, target) });
    }
  }
  return { files: files.map((file) => relative(root, file)), imports };
};
