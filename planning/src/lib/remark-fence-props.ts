/**
 * Lets whitespace-sensitive components take a fenced code block as their body:
 *
 *   <Diff path="src/x.ts" lines="10-12">
 *   ```ts
 *     indented: "exactly as written",
 *   ```
 *   </Diff>
 *
 * MDX strips up to two leading spaces from every line of an attribute
 * expression (`after={`…`}`), which is fatal for code. A fence keeps every
 * space. This moves the first fence's text into the component's prop, as a
 * plain string attribute, and drops the fence.
 *
 * It also records `data-body`, the file line where the text starts, so a
 * component can give each row of the text its own line for comments.
 */

import { visit } from "unist-util-visit";

/** Which prop the fence fills, per component. A `diff` fence on <Diff> is a patch. */
const PROPS: Readonly<Record<string, (lang: string | null) => string>> = {
  Diff: (lang) => (lang === "diff" || lang === "patch" ? "patch" : "after"),
  Calls: () => "stack",
  Files: () => "tree",
  Mermaid: () => "code",
  Modules: () => "edges",
};

interface Attribute { type: string; name: string; value: string }
interface Node {
  type: string;
  name?: string | null;
  lang?: string | null;
  value?: string;
  attributes?: Attribute[];
  children?: Node[];
  position?: { start: { line: number } };
}

export default function remarkFenceProps() {
  return (tree: Node) => {
    visit(tree as never, (node: Node) => {
      if (node.type !== "mdxJsxFlowElement" || node.name == null) return;
      const prop = PROPS[node.name];
      if (prop === undefined) return;
      const index = (node.children ?? []).findIndex((child) => child.type === "code");
      if (index < 0) return;
      const code = node.children![index]!;
      const name = prop(code.lang ?? null);
      const attributes = (node.attributes ?? []).filter((a) => a.name !== name);
      attributes.push({ type: "mdxJsxAttribute", name, value: code.value ?? "" });
      if (code.position) {
        // The fence line is `start`; its text begins on the next line.
        attributes.push({ type: "mdxJsxAttribute", name: "data-body", value: String(code.position.start.line + 1) });
      }
      node.attributes = attributes;
      node.children = (node.children ?? []).filter((_, i) => i !== index);
    });
  };
}
