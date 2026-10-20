/**
 * Reads an item file and lifts out what the site and the scripts both need:
 * the frontmatter, and every <Question>, <Followup> and <Tradeoff> in the body.
 *
 * These live in the body, where they are said, rather than in the frontmatter.
 * This file is the one place that turns them back into data, so the inbox and
 * `pnpm plans open` never disagree about what is open.
 *
 * Plain Node (run directly by `node scripts/plans.ts`), so: explicit `.ts`
 * imports, no enums, nothing Astro-specific.
 */

import { readFileSync } from "node:fs";
import { toString } from "mdast-util-to-string";
import remarkFrontmatter from "remark-frontmatter";
import remarkMdx from "remark-mdx";
import remarkParse from "remark-parse";
import { unified } from "unified";
import { visit } from "unist-util-visit";
import { parse as parseYaml } from "yaml";

export const LIFTED = ["Question", "Followup", "Tradeoff"] as const;
export type LiftedKind = (typeof LIFTED)[number];

export interface Lifted {
  readonly kind: LiftedKind;
  /** The `id` attribute, when the author gave one. */
  readonly id: string | null;
  /** The text inside the component. */
  readonly text: string;
  /** Literal attributes. A bare attribute (`done`) is `true`. */
  readonly attrs: Readonly<Record<string, string | true>>;
  /** 1-based line in the file, frontmatter included. */
  readonly line: number;
  /** A Question with no `answer`, or a Followup that is not `done`. */
  readonly open: boolean;
}

export interface Parsed {
  readonly frontmatter: Record<string, unknown>;
  readonly lifted: readonly Lifted[];
}

const isOpen = (kind: LiftedKind, attrs: Readonly<Record<string, string | true>>): boolean => {
  if (kind === "Question") return attrs["answer"] === undefined;
  if (kind === "Followup") return attrs["done"] === undefined;
  return false;
};

const processor = unified().use(remarkParse).use(remarkFrontmatter, ["yaml"]).use(remarkMdx);

export const parseSource = (source: string): Parsed => {
  const tree = processor.parse(source);
  let frontmatter: Record<string, unknown> = {};
  const lifted: Lifted[] = [];

  visit(tree, (node) => {
    if (node.type === "yaml") {
      const data: unknown = parseYaml((node as { value: string }).value);
      if (typeof data === "object" && data !== null) frontmatter = data as Record<string, unknown>;
      return;
    }
    if (node.type !== "mdxJsxFlowElement" && node.type !== "mdxJsxTextElement") return;
    const element = node as unknown as {
      name: string | null;
      attributes: { type: string; name?: string; value?: unknown }[];
      position?: { start: { line: number } };
    };
    const kind = LIFTED.find((name) => name === element.name);
    if (kind === undefined) return;

    const attrs: Record<string, string | true> = {};
    for (const attribute of element.attributes) {
      if (attribute.type !== "mdxJsxAttribute" || attribute.name === undefined) continue;
      // `answer="x"` is a string; `done` is null (bare). Expressions are skipped:
      // a lifted attribute has to be readable without running the file.
      if (attribute.value === null || attribute.value === undefined) attrs[attribute.name] = true;
      else if (typeof attribute.value === "string") attrs[attribute.name] = attribute.value;
    }

    const id = typeof attrs["id"] === "string" ? attrs["id"] : null;
    lifted.push({
      kind,
      id,
      text: toString(node).replace(/\s+/gu, " ").trim(),
      attrs,
      line: element.position?.start.line ?? 0,
      open: isOpen(kind, attrs),
    });
  });

  return { frontmatter, lifted };
};

export const parseFile = (path: string): Parsed => parseSource(readFileSync(path, "utf8"));
