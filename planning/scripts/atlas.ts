/**
 * The atlas's own checks, so its links cannot rot silently. `pnpm plans check`
 * runs them too.
 *
 *   pnpm atlas check     concept frontmatter, related slugs, cited files,
 *                        unresolved [[links]], [[code:…]] citations that no
 *                        longer resolve, map.json's references,
 *                        boxes on screen together that overlap, and the
 *                        /features rows (atlas/features.ts)
 */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { parse } from "yaml";
import { lookup, wikiLinks } from "../src/lib/atlas/glossary.ts";
import { conceptsDir, mapPath } from "../src/lib/atlas/paths.ts";
import { parseCite, resolveCite } from "../src/lib/code-cite.ts";
import { checkFeatures } from "../src/lib/features.ts";
import { overlaps, place } from "../src/lib/atlas/place.ts";
import { conceptSchema, mapSchema } from "../src/lib/atlas/schema.ts";
import { repoRoot as root } from "../src/lib/root.ts";

const FRONT = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/u;

const mdx = (dir: string): string[] =>
  existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith(".mdx")).map((n) => join(dir, n)) : [];

/** Prints each problem; returns how many there were. */
export const checkAtlas = (itemsDir: string): number => {
  let bad = 0;
  const fail = (message: string): void => {
    bad += 1;
    console.log(`✗ ${message}`);
  };
  const names = lookup();
  const slugs = new Set<string>();
  for (const file of mdx(conceptsDir)) {
    const where = relative(root, file);
    const text = readFileSync(file, "utf8");
    const match = FRONT.exec(text);
    const result = conceptSchema.safeParse(match ? parse(match[1]!) : {});
    if (!result.success) {
      fail(`${where}: ${result.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")}`);
      continue;
    }
    const data = result.data;
    if (data.slug !== basename(file, ".mdx")) fail(`${where}: slug "${data.slug}" is not the filename`);
    if (slugs.has(data.slug)) fail(`${where}: slug "${data.slug}" is used twice`);
    slugs.add(data.slug);
    for (const path of data.files) if (!existsSync(join(root, path))) fail(`${where}: cites ${path}, which does not exist`);
  }
  for (const file of mdx(conceptsDir)) {
    const match = FRONT.exec(readFileSync(file, "utf8"));
    const related = (match ? (parse(match[1]!) as { related?: unknown }).related : undefined) ?? [];
    for (const slug of related as string[]) if (!slugs.has(slug)) fail(`${relative(root, file)}: related "${slug}" is not a concept`);
  }
  const pages = [...mdx(conceptsDir), ...mdx(join(conceptsDir, "..")), ...mdx(itemsDir)];
  for (const file of pages) {
    for (const { target, line } of wikiLinks(readFileSync(file, "utf8"))) {
      if (target.startsWith("code:")) {
        const cite = parseCite(target.slice(5));
        const found = resolveCite(root, cite.path, cite.symbol);
        if (!found.ok) fail(`${relative(root, file)}:${line}: [[${target}]] ${found.error}`);
      } else if (!names.has(target.toLowerCase())) fail(`${relative(root, file)}:${line}: [[${target}]] names no concept`);
    }
  }
  const items = new Set(
    existsSync(itemsDir)
      ? readdirSync(itemsDir, { recursive: true, encoding: "utf8" }).filter((n) => n.endsWith(".mdx")).map((n) => n.slice(0, -4))
      : [],
  );
  for (const problem of checkFeatures(slugs, items)) fail(problem);
  if (existsSync(mapPath)) {
    const where = relative(root, mapPath);
    const result = mapSchema.safeParse(JSON.parse(readFileSync(mapPath, "utf8")));
    if (!result.success) {
      fail(`${where}: ${result.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")}`);
    } else {
      const map = result.data;
      const ids = new Set([...map.nodes.map((n) => n.id), ...map.groups.map((g) => g.id)]);
      const groups = new Set(map.groups.map((g) => g.id));
      for (const n of map.nodes) {
        if (n.slug !== undefined && !slugs.has(n.slug)) fail(`${where}: node "${n.id}" links to "${n.slug}", which is not a concept`);
        if (n.group !== undefined && !groups.has(n.group)) fail(`${where}: node "${n.id}" is in group "${n.group}", which does not exist`);
      }
      for (const g of map.groups) if (g.parent !== undefined && !groups.has(g.parent)) fail(`${where}: group "${g.id}" has parent "${g.parent}", which does not exist`);
      for (const e of map.edges) for (const end of [e.from, e.to]) if (!ids.has(end)) fail(`${where}: edge ${e.from} → ${e.to} names "${end}", which does not exist`);
      for (const clash of overlaps(map, place(map))) fail(`${where}: ${clash}`);
    }
  }
  return bad;
};

if (import.meta.url === `file://${process.argv[1]}`) {
  const [command = "check"] = process.argv.slice(2);
  if (command !== "check") throw new Error(`Unknown command "${command}". Try: check.`);
  const bad = checkAtlas(process.env["PLANS_ITEMS"] ?? "./items");
  console.log(bad === 0 ? "Atlas valid." : `${bad} atlas problem(s).`);
  process.exitCode = bad === 0 ? 0 : 1;
}
