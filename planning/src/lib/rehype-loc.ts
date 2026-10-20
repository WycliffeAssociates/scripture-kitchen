/**
 * Stamps `data-loc="items/foo.mdx:42"` on every block the MDX file produced,
 * including components. The annotator reads the nearest one, so a comment on
 * any paragraph, list item or call row carries the line it was written on.
 *
 * Paths are relative to the repo the plans sit in (see root.ts), so an agent
 * in that repo can open them as written.
 */

import { relative } from "node:path";
import { repoRoot as root } from "./root.ts";
import { visit } from "unist-util-visit";

const BLOCKS = new Set([
  "p", "li", "h1", "h2", "h3", "h4", "h5", "h6",
  "pre", "blockquote", "table", "tr", "dt", "dd", "figure",
]);


interface Node {
  type: string;
  tagName?: string;
  name?: string | null;
  properties?: Record<string, unknown>;
  attributes?: { type: string; name: string; value: string }[];
  position?: { start: { line: number } };
}

export default function rehypeLoc() {
  return (tree: Node, file: { path?: string }) => {
    if (file.path === undefined) return;
    const path = relative(root, file.path);
    visit(tree as never, (node: Node) => {
      const line = node.position?.start.line;
      if (line === undefined) return;
      const loc = `${path}:${String(line)}`;
      if (node.type === "element" && node.tagName !== undefined && BLOCKS.has(node.tagName)) {
        node.properties = { ...node.properties, dataLoc: loc };
      } else if (
        (node.type === "mdxJsxFlowElement" || node.type === "mdxJsxTextElement") &&
        node.name !== null && node.name !== undefined && /^[A-Z]/u.test(node.name)
      ) {
        // Components take it as a prop and put it on their root element.
        node.attributes = [...(node.attributes ?? []), { type: "mdxJsxAttribute", name: "data-loc", value: loc }];
      }
    });
  };
}
