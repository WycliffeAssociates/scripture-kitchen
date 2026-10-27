/**
 * Every concept's frontmatter, read straight from disk. The wiki-link plugin
 * runs inside the markdown pipeline, before any collection exists, and the
 * CLI runs outside Astro, so both read here rather than through
 * `astro:content`.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { parse } from "yaml";
import { conceptsDir } from "./paths.ts";
import { conceptSchema, type Concept } from "./schema.ts";

export interface Entry extends Concept {
  /** Path of the source file. */
  readonly file: string;
  /** The body, frontmatter removed. */
  readonly body: string;
}

const FRONT = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/u;

let cache: { key: string; entries: Entry[] } | null = null;

/** Concepts that parse; a broken file is `pnpm plans check`'s to report. */
export const glossary = (): Entry[] => {
  const names = readdirSync(conceptsDir).filter((name) => name.endsWith(".mdx")).sort();
  const key = names.map((name) => `${name}@${statSync(join(conceptsDir, name)).mtimeMs}`).join("|");
  if (cache?.key === key) return cache.entries;
  const entries = names.flatMap((name) => {
    const file = join(conceptsDir, name);
    const text = readFileSync(file, "utf8");
    const match = FRONT.exec(text);
    const parsed = conceptSchema.safeParse(match ? parse(match[1]!) : {});
    return parsed.success ? [{ ...parsed.data, file, body: text.slice(match![0].length) }] : [];
  });
  cache = { key, entries };
  return entries;
};

/** `[[x]]` targets, lower-cased: slug, term and every alias. */
export const lookup = (): Map<string, Entry> => {
  const map = new Map<string, Entry>();
  for (const entry of glossary()) {
    for (const name of [entry.term, ...entry.aliases]) map.set(name.toLowerCase(), entry);
  }
  // A slug wins over another page's alias of the same spelling.
  for (const entry of glossary()) map.set(entry.slug.toLowerCase(), entry);
  return map;
};

/** `[[target]]` and `[[target|shown]]`, outside code. */
export const WIKI = /\[\[([^\]|\n]+?)(?:\|([^\]\n]+?))?\]\]/gu;

/** Markdown to one line of text: links and wiki-links keep their words. */
export const plain = (markdown: string): string =>
  markdown
    .replace(WIKI, (_, target: string, shown?: string) => (shown ?? target).trim())
    .replace(/!?\[([^\]]*)\]\([^)]*\)/gu, "$1")
    .replace(/[*_]{1,2}([^*_]+)[*_]{1,2}/gu, "$1")
    .replace(/\s+/gu, " ")
    .trim();

