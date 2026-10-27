/**
 * Site paths under the deploy base. Locally the base is `/`; the Pages build
 * sets SITE_BASE (astro.config.mjs) and every internal link goes through here:
 *
 *   url("/atlas/cst")     → "/scripture-kitchen/atlas/cst"   (Pages)
 *   url("/atlas/cst")     → "/atlas/cst"                     (pnpm dev)
 *   url("https://x.org")  → "https://x.org"                  (not a site path)
 */
export const withBase = (base: string, path: string): string =>
  path.startsWith("/") && !path.startsWith("//") ? `${base.replace(/\/+$/u, "")}${path}` : path;

const BASE: string = (import.meta.env as { BASE_URL?: string } | undefined)?.BASE_URL ?? "/";

export const url = (path: string): string => withBase(BASE, path);

/** `Astro.url.pathname` with the base and any trailing slash taken off: `/scripture-kitchen/items/x/` → `/items/x`. */
export const sitePath = (pathname: string): string => {
  const base = BASE.replace(/\/+$/u, "");
  const rest = base !== "" && pathname.startsWith(base) ? pathname.slice(base.length) : pathname;
  return rest.replace(/\/$/u, "") || "/";
};
