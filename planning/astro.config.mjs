import mdx from "@astrojs/mdx";
import { unified } from "@astrojs/markdown-remark";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import rehypeLoc from "./src/lib/rehype-loc.ts";
import remarkFenceProps from "./src/lib/remark-fence-props.ts";
import remarkMermaid from "./src/lib/remark-mermaid.ts";
import remarkWiki from "./src/lib/remark-wiki.ts";

export default defineConfig({
  devToolbar: { enabled: false },
  // The atlas is the front door; the inbox moved to /inbox.
  redirects: { "/": "/atlas" },
  integrations: [mdx()],
  markdown: {
    // The remark/rehype processor, so these plugins run; MDX inherits it.
    processor: unified({ remarkPlugins: [remarkFenceProps, remarkMermaid, remarkWiki], rehypePlugins: [rehypeLoc] }),
    shikiConfig: { themes: { light: "github-light", dark: "github-dark" } },
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
