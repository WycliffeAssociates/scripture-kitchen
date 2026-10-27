/**
 * Links into the repo at the commit the site was built from, so a cited file
 * opens as it was when the page was written about it.
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
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
