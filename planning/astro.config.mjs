import mdx from "@astrojs/mdx";
import { unified } from "@astrojs/markdown-remark";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import rehypeBase from "./src/lib/rehype-base.ts";
import rehypeLoc from "./src/lib/rehype-loc.ts";
import remarkFenceProps from "./src/lib/remark-fence-props.ts";
import remarkMermaid from "./src/lib/remark-mermaid.ts";
import remarkWiki from "./src/lib/remark-wiki.ts";
import { usfmLang } from "./src/lib/usfm-lang.ts";

// The GitHub Pages build sets SITE_BASE=/scripture-kitchen (.github/workflows/pages.yml).
// Unset, the site lives at / and keeps the annotator: `pnpm dev` and a plain `pnpm build`.
const base = (process.env.SITE_BASE ?? "").replace(/^\/*/u, "/").replace(/\/+$/u, "");
const pages = base !== "";

// The inbox is a working view for the repo's own planning, not part of the published docs.
const devOnly = {
  name: "dev-only-routes",
  hooks: {
    "astro:config:setup": ({ command, injectRoute }) => {
      if (command === "dev") injectRoute({ pattern: "/inbox", entrypoint: "./src/dev/inbox.astro" });
    },
  },
};

export default defineConfig({
  ...(pages ? { site: process.env.SITE_ORIGIN ?? "https://wycliffeassociates.github.io", base } : {}),
  devToolbar: { enabled: false },
  // The atlas is the front door.
  redirects: { "/": `${base}/atlas` },
  integrations: [mdx(), devOnly],
  markdown: {
    // The remark/rehype processor, so these plugins run; MDX inherits it.
    processor: unified({ remarkPlugins: [remarkFenceProps, remarkMermaid, remarkWiki], rehypePlugins: [rehypeLoc, [rehypeBase, { base }]] }),
    shikiConfig: { themes: { light: "github-light", dark: "github-dark" }, langs: [usfmLang] },
  },
  vite: {
    plugins: [tailwindcss()],
    define: { __ANNOTATE__: JSON.stringify(!pages) },
  },
});
