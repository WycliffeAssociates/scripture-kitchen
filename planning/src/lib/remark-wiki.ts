/**
 * `[[slug]]` and `[[slug|shown text]]` in any MDX file become a link to the
 * concept page, with a hover card (term + summary):
 *
 *   [[cst]] reads [[token|tokens]]
 *     → <a class="wiki" href="/atlas/cst">cst<span class="wiki-card">…</span></a> reads …
 *
 * The target matches a slug, a term or an alias, ignoring case. One that
 * matches nothing renders as its shown text, and `pnpm plans check` lists it.
 *
 * `[[code:path#symbol]]` cites code (src/lib/code-cite.ts): a code-styled link
 * to the lines on GitHub at the build's commit, in a new tab. One that does not
 * resolve renders as plain code.
 *
 *   [[code:onion/src/cst.rs#CloseReason]]
 *     → <a class="cite" href="https://github.com/…/blob/<sha>/onion/src/cst.rs#L21-L32"
 *          target="_blank" rel="noopener" title="onion/src/cst.rs:L21–L32"><code>CloseReason</code></a>
 */
import { visit, SKIP } from "unist-util-visit";
import { lookup, plain, WIKI } from "./atlas/glossary.ts";
import { githubBlob } from "./atlas/github.ts";
import { parseCite, resolveCite } from "./code-cite.ts";
import { repoRoot } from "./root.ts";

interface Node {
  type: string;
  value?: string;
  url?: string;
  children?: Node[];
  data?: { hName?: string; hProperties?: Record<string, unknown> };
}

const text = (value: string): Node => ({ type: "text", value });

const code = (target: string, shown: string | undefined): Node => {
  const cite = parseCite(target, shown);
  const found = resolveCite(repoRoot, cite.path, cite.symbol);
  if (!found.ok) return { type: "inlineCode", value: cite.shown, data: { hProperties: { className: ["cite-broken"], title: found.error } } };
  const range = found.lines;
  const anchor = range === null ? "" : range.start === range.end ? `#L${range.start}` : `#L${range.start}-L${range.end}`;
  const plainMd = range !== null && /\.mdx?$/u.test(cite.path) ? "?plain=1" : "";
  const where = range === null ? cite.path : range.start === range.end ? `${cite.path}:L${range.start}` : `${cite.path}:L${range.start}–L${range.end}`;
  return {
    type: "link",
    url: `${githubBlob(cite.path)}${plainMd}${anchor}`,
    data: { hProperties: { className: ["cite"], target: "_blank", rel: "noopener", title: where } },
    children: [{ type: "inlineCode", value: cite.shown }],
  };
};

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
        if (target.startsWith("code:")) {
          out.push(code(target.slice(5), match[2]));
          last = at + match[0].length;
          continue;
        }
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
