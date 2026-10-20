/**
 * Reads `roadmap.mdx` (beside `items/`) into phases and steps, for the
 * roadmap page's counts, the item page's "on the roadmap" line, and
 * `pnpm plans roadmap` / `check`. Plain Node, like lift.ts.
 *
 *   <Phase id="now" title="Now" goal="…">
 *     <Step item="<uuid or filename>" with="tests" />
 *     <Together> <Step item="…" /> <Step item="…" /> </Together>
 *     <Gate>You iterate on feedback here.</Gate>
 *   </Phase>
 */

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { toString } from "mdast-util-to-string";
import remarkFrontmatter from "remark-frontmatter";
import remarkMdx from "remark-mdx";
import remarkParse from "remark-parse";
import { unified } from "unified";

export interface RoadmapStep {
  readonly kind: "Step" | "Gate";
  /** An item's id or filename, as written. */
  readonly item: string | null;
  readonly text: string;
  readonly extras: readonly string[];
  /** 1-based, across the whole roadmap; gates are not numbered. */
  readonly n: number | null;
  /** Steps in the same <Together> share a group number. */
  readonly group: number | null;
  readonly line: number;
}

export interface RoadmapPhase {
  readonly id: string | null;
  readonly title: string;
  readonly goal: string | null;
  readonly line: number;
  readonly steps: readonly RoadmapStep[];
}

/** Beside `items/`, in the planning folder (where the site and the CLI run). */
export const roadmapPath = (): string => resolve("./roadmap.mdx");

interface Jsx {
  type: string;
  name?: string | null;
  attributes?: { type: string; name?: string; value?: unknown }[];
  children?: Jsx[];
  position?: { start: { line: number } };
}

const attr = (node: Jsx, name: string): string | null => {
  const found = node.attributes?.find((a) => a.type === "mdxJsxAttribute" && a.name === name);
  return typeof found?.value === "string" ? found.value : null;
};
const isJsx = (node: Jsx): boolean => node.type === "mdxJsxFlowElement" || node.type === "mdxJsxTextElement";

const processor = unified().use(remarkParse).use(remarkFrontmatter, ["yaml"]).use(remarkMdx);

export const parseRoadmap = (source: string): RoadmapPhase[] => {
  const tree = processor.parse(source) as unknown as Jsx;
  const phases: RoadmapPhase[] = [];
  let n = 0;
  let groups = 0;

  const collect = (node: Jsx, steps: RoadmapStep[], group: number | null): void => {
    for (const child of node.children ?? []) {
      if (isJsx(child) && child.name === "Together") {
        groups += 1;
        collect(child, steps, groups);
      } else if (isJsx(child) && (child.name === "Step" || child.name === "Gate")) {
        const kind = child.name;
        if (kind === "Step") n += 1;
        steps.push({
          kind,
          item: attr(child, "item"),
          text: toString(child as never).replace(/\s+/gu, " ").trim(),
          extras: (attr(child, "with") ?? "").split(",").map((s) => s.trim()).filter(Boolean),
          n: kind === "Step" ? n : null,
          group,
          line: child.position?.start.line ?? 0,
        });
      } else collect(child, steps, group);
    }
  };

  const walk = (node: Jsx): void => {
    for (const child of node.children ?? []) {
      if (isJsx(child) && child.name === "Phase") {
        const steps: RoadmapStep[] = [];
        collect(child, steps, null);
        phases.push({
          id: attr(child, "id"),
          title: attr(child, "title") ?? attr(child, "id") ?? "Phase",
          goal: attr(child, "goal"),
          line: child.position?.start.line ?? 0,
          steps,
        });
      } else walk(child);
    }
  };
  walk(tree);
  return phases;
};

export const readRoadmap = (): RoadmapPhase[] | null => {
  const path = roadmapPath();
  return existsSync(path) ? parseRoadmap(readFileSync(path, "utf8")) : null;
};

/** Does a step's `item` name this item? By id, by filename, or by filename without its date. */
export const refersTo = (ref: string, item: { id: string; slug: string }): boolean =>
  ref === item.id || ref === item.slug || item.slug.replace(/^\d{4}-\d{2}-\d{2}-/u, "") === ref;
