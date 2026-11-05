// The head ids `bttwriter/tests/replay.rs` pins: Sefer's own walk (isomorphic-git) and
// replayWriterHistories, at Sefer 326f02c8, over the five fixture bundles.
//
// To regenerate, from a Sefer checkout of 326f02c8 with its node_modules:
//   cp <kitchen>/bttwriter/fixtures/sefer-heads.test.ts src/platform/web/writerGolden.test.ts
//   KITCHEN=<kitchen> GOLDEN_OUT=/tmp npx vitest run --project core src/platform/web/writerGolden.test.ts
// and copy the heads from /tmp/heads.json into tests/replay.rs.
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { NodeFileSystem } from "@effect/platform-node";
import { Effect } from "effect";
import { it } from "vitest";

import { Git } from "#core/git/git";
import { readWriterHistory, replayWriterHistories } from "#core/migrate/writerHistory";
import { WebGitLive } from "./git";

const FIX = `${process.env.KITCHEN ?? "."}/bttwriter/fixtures/repos`;
const REPOS: [string, string, string][] = [
  ["kpass_tmf_3jn_text_reg", "3JN", "3 John"],
  ["bahasatech.indotimur_kps-x-kaigar_3jn_text_reg", "3JN", "3 John"],
  ["chunga_major_bem-x-ngumbo_2ch_text_reg", "2CH", "2 Chronicles"],
  ["andriatiana_wa_msh_job_text_udb", "JOB", "Job"],
  ["bahasatech.indotimur_saw_isa_text_reg", "ISA", "Isaiah"],
];
const ownerOf = (repo: string): string =>
  repo.startsWith("chunga_major_") ? "chunga_major" :
  repo.startsWith("andriatiana_wa_") ? "andriatiana_wa" :
  repo.slice(0, repo.indexOf("_"));

it("prints heads", { timeout: 600_000 }, async () => {
  const work = mkdtempSync(join(tmpdir(), "sefer-golden-"));
  const histories = [];
  const out: Record<string, unknown> = {};
  for (const [repo, book, title] of REPOS) {
    const dir = join(work, repo);
    execFileSync("git", ["clone", "--quiet", join(FIX, `${repo}.bundle`), dir]);
    const owner = ownerOf(repo);
    const source = `${owner}/${repo.slice(owner.length + 1)}`;
    const history = await Effect.runPromise(
      Effect.gen(function* () {
        yield* Git;
        return yield* readWriterHistory({ root: dir }, book, title, source);
      }).pipe(Effect.provide(WebGitLive), Effect.provide(NodeFileSystem.layer)),
    );
    histories.push(history);
    const one = await replayWriterHistories([history]);
    out[repo] = { head: one?.head, commits: one?.commits, unchanged: one?.unchanged, saves: history.saves.length };
  }
  // One history per book: the second 3 John is left out (Sefer would hold both in one file).
  const all = await replayWriterHistories(histories.filter((_, i) => i !== 1));
  out["all"] = { head: all?.head, commits: all?.commits, unchanged: all?.unchanged };
  writeFileSync(join(process.env.GOLDEN_OUT ?? work, "heads.json"), JSON.stringify(out, null, 2));
  console.log(JSON.stringify(out, null, 2));
});
