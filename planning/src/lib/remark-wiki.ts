/**
 * `[[slug]]` and `[[slug|shown text]]` in any MDX file become a link to the
 * concept page, with a hover card (term + summary):
 *
 *   [[cst]] reads [[token|tokens]]
 *     → <a class="wiki" href="/atlas/cst">cst<span class="wiki-card">…</span></a> reads …
 *
 * The target matches a slug, a term or an alias, ignoring case. One that
 * matches nothing renders as its shown text, and `pnpm plans check` lists it.
 */
import { visit, SKIP } from "unist-util-visit";
import { lookup, plain, WIKI } from "./atlas/glossary.ts";

interface Node {
  type: string;
  value?: string;
  url?: string;
  children?: Node[];
  data?: { hName?: string; hProperties?: Record<string, unknown> };
}

const text = (value: string): Node => ({ type: "text", value });

export default function remarkWiki() {
  return (tree: Node) => {
    const names = lookup();
    visit(tree as never, "text", (node: Node, index: number | undefined, parent: Node | undefined) => {
      if (parent === undefined || index === undefined || parent.type === "link") return;
      const value = node.value ?? "";
      if (!value.includes("[[")) return;
      const out: Node[] = [];
      let last = 0;
      for (const match of value.matchAll(WIKI)) {
        const at = match.index;
        if (at > last) out.push(text(value.slice(last, at)));
        const target = match[1]!.trim();
        const shown = (match[2] ?? match[1]!).trim();
        const entry = names.get(target.toLowerCase());
        out.push(entry === undefined ? text(shown) : {
          type: "link",
          url: `/atlas/${entry.slug}`,
          data: { hProperties: { className: ["wiki"], dataCrate: entry.crate } },
          children: [
            text(shown),
            {
              type: "emphasis",
              data: { hName: "span", hProperties: { className: ["wiki-card"], ariaHidden: "true" } },
              children: [
                { type: "strong", children: [text(entry.term)] },
                { type: "emphasis", data: { hName: "span" }, children: [text(plain(entry.summary))] },
              ],
            },
          ],
        });
        last = at + match[0].length;
      }
      if (last < value.length) out.push(text(value.slice(last)));
      parent.children!.splice(index, 1, ...out);
      return [SKIP, index + out.length];
    });
  };
}
