/**
 * The verify-kitchen feature map, read at build time from the skill's own
 * files so they stay the source of truth.
 *
 *   README.md table  | Lint diagnostics | [lint.md](lint.md) | none yet |
 *   lint.md          # Lint diagnostics / ## Sub-features / ## How to get to it …
 *     → { title, file, helper, sections: { doors, user, proof, gotchas }, missing }
 */
import { createMarkdownProcessor } from "@astrojs/markdown-remark";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { repoRoot } from "./root.ts";

export const FEATURES_DIR = ".claude/skills/verify-kitchen/features";

/** The four required H2s, matched by prefix: "Driving it with findings.mjs" counts. */
const REQUIRED = {
  doors: "Sub-features",
  user: "How to get to it",
  proof: "Driving it with",
  gotchas: "Gotchas",
} as const;
type Key = keyof typeof REQUIRED;

export interface Feature {
  name: string;
  title: string;
  file: string;
  helper: string;
  html: Partial<Record<Key, string>>;
  /** The heading each section had in the file. */
  headings: Partial<Record<Key, string>>;
  missing: string[];
  exists: boolean;
}

const processor = createMarkdownProcessor({
  shikiConfig: { themes: { light: "github-light", dark: "github-dark" } },
});
const render = async (md: string): Promise<string> => (await (await processor).render(md)).code;

/** `## X` headings outside code fences, with the text under each. */
const split = (md: string): { title: string; sections: [string, string][] } => {
  let title = "";
  const sections: [string, string][] = [];
  let fence = false;
  for (const line of md.split("\n")) {
    if (/^\s*(```|~~~)/u.test(line)) fence = !fence;
    if (!fence && /^# /u.test(line) && title === "") title = line.slice(2).trim();
    else if (!fence && /^## /u.test(line)) sections.push([line.slice(3).trim(), ""]);
    else if (sections.length > 0) {
      const last = sections[sections.length - 1];
      if (last) last[1] += `${line}\n`;
    }
  }
  return { title, sections };
};

/** `galley.toc(id)` in a code span → marked as a galley door. */
const markDoors = (html: string): string => html.replace(/<code>(galley\.[\w.]+\()/gu, '<code class="fdoor">$1');

const strip = (cell: string): string => cell.replace(/^`|`$/gu, "").trim();

export const loadFeatures = async (): Promise<{ intro: string; features: Feature[] }> => {
  const dir = join(repoRoot, FEATURES_DIR);
  const readme = existsSync(join(dir, "README.md")) ? readFileSync(join(dir, "README.md"), "utf8") : "";
  const rows = readme
    .split("\n")
    .filter((l) => /^\|/u.test(l) && !/^\|\s*-/u.test(l))
    .map((l) => l.split("|").slice(1, -1).map((c) => c.trim()))
    .slice(1);
  const intro = readme.split("\n").find((l) => l.trim() !== "" && !l.startsWith("#") && !l.startsWith("|")) ?? "";
  const features = await Promise.all(rows.map(async ([name = "", link = "", helper = ""]): Promise<Feature> => {
    const fileName = /\(([^)]+)\)/u.exec(link)?.[1] ?? link;
    const file = `${FEATURES_DIR}/${fileName}`;
    const path = join(repoRoot, file);
    if (!existsSync(path)) {
      return { name, title: name, file, helper: strip(helper), html: {}, headings: {}, missing: Object.values(REQUIRED), exists: false };
    }
    const { title, sections } = split(readFileSync(path, "utf8"));
    const html: Partial<Record<Key, string>> = {};
    const headings: Partial<Record<Key, string>> = {};
    const missing: string[] = [];
    for (const [key, prefix] of Object.entries(REQUIRED) as [Key, string][]) {
      const found = sections.find(([h]) => h.startsWith(prefix));
      if (!found) {
        missing.push(prefix);
        continue;
      }
      headings[key] = found[0];
      html[key] = markDoors(await render(found[1]));
    }
    return { name, title: title || name, file, helper: strip(helper), html, headings, missing, exists: true };
  }));
  return { intro, features };
};
