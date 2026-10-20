/**
 * The document for a sandboxed UI frame (<Mock>, a <Machine> state's screen):
 * the design tokens as plain custom properties, the parent page's theme, and
 * the given HTML.
 */
import tokens from "../styles/tokens.css?raw";

// Tailwind's directives mean nothing in a frame; @theme's values (fonts,
// radii, text sizes) become plain :root properties.
const vars = tokens
  .replace(/@import[^;]+;|@source[^;]+;|@custom-variant[\s\S]*?\n\}\n/gu, "")
  .replace(/@theme(?: inline)? \{/gu, ":root {");

const theme =
  "<script>try{const t=parent.document.documentElement.dataset.theme;if(t)document.documentElement.dataset.theme=t}catch{}<\/script>";

export const frameDocument = (body: string): string =>
  `<!doctype html><meta charset="utf-8"><style>${vars} body{margin:0;padding:12px;font-family:var(--font-sans);background:var(--surface-primary);color:var(--on-surface-primary)}</style>${theme}${body}`;
