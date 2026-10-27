/**
 * Puts the deploy base in front of every root-relative `href`/`src` an MDX
 * file renders: markdown links, `[[wiki]]` links (remark-wiki writes
 * `/atlas/<slug>`) and plain `<a href="/…">` written in MDX.
 *
 *   [the wire](/atlas/wire)  → <a href="/scripture-kitchen/atlas/wire">
 *
 * A relative link to a file in the repo (paths are from the repo root) goes to
 * GitHub at the build's commit, since the site does not serve repo files:
 *
 *   [rules](sous-chef/rules/hygiene.md)  → https://github.com/…/blob/<sha>/sous-chef/rules/hygiene.md
 */
import { visit } from "unist-util-visit";
import { githubBlob, inRepo } from "./atlas/github.ts";
import { withBase } from "./url.ts";

const REPO_FILE = /^(?![a-z][a-z0-9+.-]*:|[#/.?])([^#?]+)(#.*)?$/iu;

const rewrite = (base: string, value: string): string => {
  const file = REPO_FILE.exec(value);
  if (file !== null && inRepo(file[1]!)) return `${githubBlob(file[1]!)}${file[2] ?? ""}`;
  return base === "" || base === "/" ? value : withBase(base, value);
};

interface Node {
  type: string;
  name?: string | null;
  properties?: Record<string, unknown>;
  attributes?: { type: string; name?: string; value?: unknown }[];
}

const ATTRS = new Set(["href", "src"]);

export default function rehypeBase(options: { base: string }) {
  const { base } = options;
  return (tree: Node) => {
    visit(tree as never, (node: Node) => {
      if (node.type === "element" && node.properties !== undefined) {
        for (const name of ATTRS) {
          const value = node.properties[name];
          if (typeof value === "string") node.properties[name] = rewrite(base, value);
        }
      } else if ((node.type === "mdxJsxFlowElement" || node.type === "mdxJsxTextElement") && node.attributes !== undefined && /^[a-z]/u.test(node.name ?? "")) {
        // Plain HTML written in MDX; a component's props are its own business.
        for (const attr of node.attributes) {
          if (attr.type === "mdxJsxAttribute" && attr.name !== undefined && ATTRS.has(attr.name) && typeof attr.value === "string") {
            attr.value = rewrite(base, attr.value);
          }
        }
      }
    });
  };
}
