/**
 * Every concept's frontmatter, read straight from disk. The wiki-link plugin
 * runs inside the markdown pipeline, before any collection exists, and the
 * CLI runs outside Astro, so both read here rather than through
 * `astro:content`.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { parse } from "yaml";
import { parseCite } from "../code-cite.ts";
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

/** `[[…]]` in a page outside fenced and inline code, in order, with line numbers. */
export const wikiLinks = (text: string): { target: string; shown: string | undefined; line: number }[] => {
  const out: { target: string; shown: string | undefined; line: number }[] = [];
  let fenced = false;
  text.split("\n").forEach((line, i) => {
    if (/^\s*(```|~~~)/u.test(line)) fenced = !fenced;
    if (fenced) return;
    for (const match of line.replace(/`[^`]*`/gu, "").matchAll(WIKI)) out.push({ target: match[1]!.trim(), shown: match[2]?.trim(), line: i + 1 });
  });
  return out;
};

const shownOf = (target: string, shown?: string): string =>
  (shown ?? (target.startsWith("code:") ? parseCite(target.slice(5)).shown : target)).trim();

/** Markdown to one line of text: links and wiki-links keep their words. */
export const plain = (markdown: string): string =>
  markdown
    .split(/(`[^`\n]+`)/u)
    .map((part) =>
      part.startsWith("`")
        ? part.slice(1, -1)
        : part
            .replace(WIKI, (_, target: string, shown?: string) => shownOf(target, shown))
            .replace(/!?\[([^\]]*)\]\([^)]*\)/gu, "$1")
            .replace(/[*_]{1,2}([^*_]+)[*_]{1,2}/gu, "$1"),
    )
    .join("")
    .replace(/\s+/gu, " ")
    .trim();

