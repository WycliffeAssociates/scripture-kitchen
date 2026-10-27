/**
 * The concept pages as the site uses them: each entry with the pages it
 * names in `related` (reads from) and the pages that name it (read by), and a
 * plain-text "why" for the canvas's side panel.
 */
import { getCollection, type CollectionEntry } from "astro:content";
import { CONCEPT_KINDS, type ConceptKind } from "./schema.ts";
import { plain } from "./glossary.ts";

export { plain };

export interface Loaded {
  readonly entry: CollectionEntry<"concepts">;
  readonly slug: string;
  readonly problem: string;
  readonly why: string;
  readonly readsFrom: readonly string[];
  readonly readBy: readonly string[];
}

const paragraphs = (body: string): string[] =>
  body
    .replace(/```[\s\S]*?```/gu, "")
    .split(/\n\s*\n/u)
    .map((block) => block.trim())
    .filter((block) => block !== "" && !/^(import |export |<|\||>|- |\* |\d+\. )/u.test(block));

const under = (body: string, heading: RegExp): string | null => {
  const at = body.search(heading);
  if (at < 0) return null;
  const rest = body.slice(at).replace(/^.*\n/u, "");
  const first = paragraphs(rest.split(/^#{2,3}\s/mu)[0] ?? "")[0];
  return first ? plain(first) : null;
};

const problemOf = (body: string): string => under(body, /^#{2,3}\s+the problem\b/imu) ?? "";

/** The Solution section, else the older "Why", else the first paragraph after the opening one. */
const whyOf = (body: string): string => {
  const found = under(body, /^#{2,3}\s+the solution\b/imu) ?? under(body, /^#{2,3}\s+why\b/imu);
  if (found) return found;
  const prose = paragraphs(body).filter((p) => !p.startsWith("#"));
  return plain(prose[1] ?? prose[0] ?? "");
};

let cached: Promise<Loaded[]> | null = null;
export const loadConcepts = (): Promise<Loaded[]> => (import.meta.env.DEV ? load() : (cached ??= load()));

const load = async (): Promise<Loaded[]> => {
  const entries = await getCollection("concepts");
  const slugs = new Set(entries.map((e) => e.data.slug));
  return entries
    .map((entry) => ({
      entry,
      slug: entry.data.slug,
      problem: problemOf(entry.body ?? ""),
      why: whyOf(entry.body ?? ""),
      readsFrom: entry.data.related.filter((s) => slugs.has(s)),
      readBy: entries.filter((other) => other.data.related.includes(entry.data.slug)).map((other) => other.data.slug),
    }))
    .sort((a, b) => a.entry.data.term.localeCompare(b.entry.data.term));
};

export const KIND_ORDER: readonly ConceptKind[] = ["product", "crate", "primitive", "wire", "workflow", "concept"];
export const KIND_LABEL: Readonly<Record<ConceptKind, string>> = {
  product: "Products", crate: "Crates", primitive: "Primitives", wire: "Wires", workflow: "Workflows", concept: "Concepts",
};
export const byKind = (all: readonly Loaded[]) =>
  KIND_ORDER.filter((kind) => CONCEPT_KINDS.includes(kind))
    .map((kind) => ({ kind, label: KIND_LABEL[kind], items: all.filter((c) => c.entry.data.kind === kind) }))
    .filter((group) => group.items.length > 0);
