import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkFeatures, lintCategories, sousFamilies } from "./features.ts";
import { repoRoot as root } from "./root.ts";

test("the lint rows are every catalog code, once", () => {
  const { codes } = JSON.parse(readFileSync(join(root, "onion-wasm/diagnostics.json"), "utf8")) as { codes: { name: string }[] };
  assert.deepEqual(lintCategories().flatMap((c) => c.codes.map((x) => x.name)).sort(), codes.map((c) => c.name).sort());
});

test("the sous rows are every message id, once", () => {
  const ids = Object.keys(JSON.parse(readFileSync(join(root, "galley/sous-messages.en.json"), "utf8")) as object);
  assert.deepEqual(sousFamilies().flatMap((f) => f.messages).sort(), [...ids].sort());
});

test("a concept or plan the rows name must exist", () => {
  const problems = checkFeatures(new Set(), new Set());
  assert.ok(problems.some((p) => /Import › USFM: concept "cst" is not a concept page/u.test(p)));
  assert.ok(problems.some((p) => /USJ → USFM: item ".+" is not a plan/u.test(p)));
});
