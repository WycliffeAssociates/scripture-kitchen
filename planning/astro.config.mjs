import mdx from "@astrojs/mdx";
import { unified } from "@astrojs/markdown-remark";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import rehypeLoc from "./src/lib/rehype-loc.ts";
import remarkFenceProps from "./src/lib/remark-fence-props.ts";

export default defineConfig({
  devToolbar: { enabled: false },
  integrations: [mdx()],
  markdown: {
    // The remark/rehype processor, so these plugins run; MDX inherits it.
    processor: unified({ remarkPlugins: [remarkFenceProps], rehypePlugins: [rehypeLoc] }),
    shikiConfig: { themes: { light: "github-light", dark: "github-dark" } },
  },
  vite: {
    plugins: [tailwindcss()],
    // Mermaid is imported lazily, so Vite would find it late and re-bundle mid-page.
    optimizeDeps: { include: ["mermaid"] },
  },
});
