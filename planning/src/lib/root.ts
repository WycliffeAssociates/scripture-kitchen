/**
 * The repo the plans sit in. Every path a plan shows or a comment carries is
 * relative to it, so an agent working in that repo can open it as written.
 *
 * PLANS_REPO_ROOT if set; else the git checkout this folder is in; else this
 * folder (a standalone copy of the boilerplate).
 */
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const find = (): string => {
  const set = process.env["PLANS_REPO_ROOT"];
  if (set !== undefined && set !== "") return resolve(set);
  try {
    return execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return process.cwd();
  }
};

export const repoRoot = find();
