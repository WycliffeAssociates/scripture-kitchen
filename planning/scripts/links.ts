/**
 * `pnpm links check`: every internal href/src in the built site resolves to a
 * file in dist/, and every site path sits under the deploy base.
 *
 *   SITE_BASE=/scripture-kitchen pnpm build && SITE_BASE=/scripture-kitchen pnpm links check
 *   ok: 412 pages, 9_310 internal links
 *
 *   dist/atlas/cst/index.html: /atlas/token (outside the base /scripture-kitchen)
 *   dist/items/x/index.html: /scripture-kitchen/items/gone (no such file)
 *
 * `/x` resolves to `x`, `x/index.html` or `x.html`, as GitHub Pages serves them.
 * Run it with the SITE_BASE the build used; a second argument names another output dir.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const [command, dir] = process.argv.slice(2);
const dist = resolve(dir ?? join(dirname(fileURLToPath(import.meta.url)), "..", "dist"));
const base = (process.env["SITE_BASE"] ?? "").replace(/^\/*/u, "/").replace(/\/+$/u, "");

if (command !== "check") {
  console.error("usage: pnpm links check [dist-dir]   (SITE_BASE as the build had it)");
  process.exit(2);
}
if (!existsSync(dist)) {
  console.error("no dist/: run pnpm build first");
  process.exit(2);
}

const walk = (dir: string): string[] =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });

const files = walk(dist);
const exists = (rel: string): boolean => {
  const path = join(dist, rel);
  const file = (p: string): boolean => existsSync(p) && statSync(p).isFile();
  return file(path) || file(join(path, "index.html")) || file(`${path}.html`);
};

const ATTR = /\s(?:href|src|xlink:href|action|poster)\s*=\s*(?:"([^"]*)"|'([^']*)')/gu;
const SRCSET = /\ssrcset\s*=\s*"([^"]*)"/gu;
const CSS_URL = /url\(\s*["']?([^"')]+)["']?\s*\)/gu;
const EXTERNAL = /^(?:[a-z][a-z0-9+.-]*:|\/\/)/iu;

const problems: string[] = [];
let pages = 0;
let links = 0;

const check = (from: string, raw: string): void => {
  const target = raw.replace(/&amp;/gu, "&").trim();
  if (target === "" || target.startsWith("#") || EXTERNAL.test(target) || target.includes("${")) return;
  const path = decodeURI(target.replace(/[?#].*$/su, ""));
  if (path === "") return;
  links++;
  const where = relative(join(dist, ".."), from);
  // A relative link resolves against the page's own URL.
  const fromUrl = `/${relative(dist, from).replace(/index\.html$/u, "").replace(/\.html$/u, "")}`;
  const absolute = path.startsWith("/") ? path : new URL(path, `https://site${base}${fromUrl}`).pathname;
  if (base !== "" && absolute !== base && !absolute.startsWith(`${base}/`)) {
    problems.push(`${where}: ${target} (outside the base ${base})`);
    return;
  }
  const rel = absolute.slice(base.length).replace(/^\/+/u, "");
  if (!exists(rel)) problems.push(`${where}: ${target} (no such file)`);
};

for (const file of files) {
  if (file.endsWith(".html")) {
    pages++;
    const html = readFileSync(file, "utf8")
      // Scripts and JSON islands carry code and data, not links.
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/giu, "");
    for (const m of html.matchAll(ATTR)) check(file, m[1] ?? m[2] ?? "");
    for (const m of html.matchAll(SRCSET)) for (const part of m[1]!.split(",")) check(file, part.trim().split(/\s+/u)[0] ?? "");
    for (const m of html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/giu)) for (const u of m[1]!.matchAll(CSS_URL)) check(file, u[1]!);
    for (const m of html.matchAll(/<meta\s+http-equiv="refresh"\s+content="[^"]*url=([^"]+)"/giu)) check(file, m[1]!);
  } else if (file.endsWith(".css")) {
    for (const u of readFileSync(file, "utf8").matchAll(CSS_URL)) check(file, u[1]!);
  }
}

if (problems.length > 0) {
  for (const p of problems) console.error(p);
  console.error(`\n${problems.length} broken internal link(s) in ${pages} pages (base ${base || "/"})`);
  process.exit(1);
}
console.log(`ok: ${pages} pages, ${links} internal links (base ${base || "/"})`);
