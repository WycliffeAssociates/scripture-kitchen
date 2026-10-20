/**
 * Syntax colour for <Source> and <Diff>, at build time, in both themes.
 *
 * One highlighter for the whole build. Each line can carry attributes — a line
 * number for the gutter, a diff sign, a highlight — so the gutter and the
 * colours are CSS (`site.css`, `.code`), not markup per component.
 */

import { createHighlighter, type Highlighter, type ShikiTransformer } from "shiki";

const THEMES = { light: "github-light", dark: "github-dark" } as const;

const BY_EXTENSION: Readonly<Record<string, string>> = {
  ts: "ts", mts: "ts", cts: "ts", tsx: "tsx", js: "js", mjs: "js", cjs: "js", jsx: "jsx",
  json: "json", jsonc: "jsonc", css: "css", html: "html", md: "md", mdx: "mdx",
  rs: "rust", py: "python", sh: "shell", bash: "shell", zsh: "shell", sql: "sql",
  toml: "toml", yaml: "yaml", yml: "yaml", astro: "astro", svelte: "svelte", vue: "vue",
  go: "go", swift: "swift", kt: "kotlin", java: "java", rb: "ruby", usfm: "text",
};

export const langOf = (path: string): string =>
  BY_EXTENSION[path.split(".").pop()?.toLowerCase() ?? ""] ?? "text";

let shared: Promise<Highlighter> | null = null;
const highlighter = (): Promise<Highlighter> =>
  (shared ??= createHighlighter({ themes: Object.values(THEMES), langs: [] }));

/**
 * Per-line attributes, by 1-based index into the code passed in. The special
 * `gutter` key becomes a leading, unselectable span (line numbers, diff signs).
 */
export type LineAttributes = (index: number) => Readonly<Record<string, string | undefined>>;

export const highlight = async (
  code: string,
  lang: string,
  attributes: LineAttributes = () => ({}),
): Promise<string> => {
  const shiki = await highlighter();
  let use = lang;
  if (use !== "text" && !shiki.getLoadedLanguages().includes(use)) {
    try {
      await shiki.loadLanguage(use as never);
    } catch {
      use = "text";
    }
  }
  const lines: ShikiTransformer = {
    line(node, index) {
      for (const [name, value] of Object.entries(attributes(index))) {
        if (value === undefined) continue;
        if (name === "gutter") {
          node.children.unshift({
            type: "element",
            tagName: "span",
            properties: { class: "gutter", "aria-hidden": "true" },
            children: [{ type: "text", value }],
          });
        } else node.properties[name] = value;
      }
    },
  };
  return shiki.codeToHtml(code, {
    lang: use,
    themes: THEMES,
    defaultColor: "light",
    transformers: [lines],
  });
};

/** "3-5,9" → a test for absolute line numbers. */
export const lineSet = (spec: string | undefined): ((line: number) => boolean) => {
  if (spec === undefined || spec.trim() === "") return () => false;
  const ranges = spec.split(",").map((part) => {
    const [a, b] = part.trim().split("-").map(Number);
    return [a ?? 0, b ?? a ?? 0] as const;
  });
  return (line) => ranges.some(([a, b]) => line >= a && line <= b);
};

/**
 * The coloured HTML of each line on its own, for layouts that place lines
 * themselves (the rows of a <Diff>). Shiki puts one `<span class="line">` per
 * line, separated by newlines, so the split is exact.
 */
export const highlightLines = async (code: string, lang: string): Promise<string[]> => {
  const html = await highlight(code, lang);
  const inner = html.slice(html.indexOf("<code>") + 6, html.lastIndexOf("</code>"));
  return inner.split("\n").map((line) => line.replace(/^<span class="line">/u, "").replace(/<\/span>$/u, ""));
};
