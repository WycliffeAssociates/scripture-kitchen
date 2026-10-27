/**
 * A bare ```mermaid fence becomes `<Mermaid code="…">`, so a concept page or
 * an item draws with plain markdown and both go through one renderer (Pele, at
 * build). Fences already inside `<Mermaid>` were taken by remark-fence-props.
 */
import { visit } from "unist-util-visit";

interface Node { type: string; lang?: string | null; value?: string; name?: string; attributes?: unknown[]; children?: Node[] }

export default function remarkMermaid() {
  return (tree: Node) => {
    visit(tree as never, "code", (node: Node) => {
      if (node.lang !== "mermaid") return;
      const code = node.value ?? "";
      Object.assign(node, {
        type: "mdxJsxFlowElement",
        name: "Mermaid",
        attributes: [{ type: "mdxJsxAttribute", name: "code", value: code }],
        children: [],
      });
      delete node.lang;
      delete node.value;
    });
  };
}
