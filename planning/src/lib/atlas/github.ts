/**
 * Links into the repo at the commit the site was built from, so a cited file
 * opens as it was when the page was written about it.
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { parseCite, resolveCite } from "../code-cite.ts";
import { repoRoot } from "../root.ts";

const REPO = "https://github.com/WycliffeAssociates/scripture-kitchen";

const git = (...args: string[]): string =>
  execFileSync("git", ["-C", repoRoot, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();

/** CI builds a pushed commit; a local build of an unpushed one links master, since GitHub has no page for it. */
export const sha = ((): string => {
  if (process.env["GITHUB_SHA"]) return process.env["GITHUB_SHA"];
  try {
    const head = git("rev-parse", "HEAD");
    return git("branch", "-r", "--contains", head) === "" ? "master" : head;
  } catch {
    return "master";
  }
})();

/** `onion/src/wire.rs` (optionally `#L12`) at the build's sha. */
export const githubBlob = (path: string, line?: number): string =>
  `${REPO}/blob/${sha}/${path.replace(/^\/+/u, "")}${line === undefined ? "" : `#L${line}`}`;

/** Whether a cited path exists in this checkout; a missing one is shown as such. */
export const inRepo = (path: string): boolean => existsSync(join(repoRoot, path));

export type CiteLink =
  | { readonly ok: true; readonly shown: string; readonly href: string; readonly title: string }
  | { readonly ok: false; readonly shown: string; readonly error: string };

/** A `[[code:…]]` target as its GitHub link: `onion/src/cst.rs#CloseReason` → `…/cst.rs#L21-L32`, titled `onion/src/cst.rs:L21–L32`. */
export const citeLink = (target: string, shown?: string): CiteLink => {
  const cite = parseCite(target, shown);
  const found = resolveCite(repoRoot, cite.path, cite.symbol);
  if (!found.ok) return { ok: false, shown: cite.shown, error: found.error };
  const range = found.lines;
  const anchor = range === null ? "" : range.start === range.end ? `#L${range.start}` : `#L${range.start}-L${range.end}`;
  const plainMd = range !== null && /\.mdx?$/u.test(cite.path) ? "?plain=1" : "";
  const title = range === null ? cite.path : range.start === range.end ? `${cite.path}:L${range.start}` : `${cite.path}:L${range.start}–L${range.end}`;
  return { ok: true, shown: cite.shown, href: `${githubBlob(cite.path)}${plainMd}${anchor}`, title };
};
