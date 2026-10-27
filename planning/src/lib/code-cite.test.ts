import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import assert from "node:assert/strict";
import { parseCite, resolveCite } from "./code-cite.ts";
import { repoRoot as root } from "./root.ts";

const lines = (path: string): string[] => readFileSync(join(root, path), "utf8").split("\n");
const lineOf = (path: string, text: string, from = 0): number => {
  const at = lines(path).findIndex((l, i) => i >= from && l.includes(text));
  assert.ok(at >= 0, `${path} has no line with ${text}`);
  return at + 1;
};
const range = (path: string, symbol: string): { start: number; end: number } => {
  const found = resolveCite(root, path, symbol);
  assert.ok(found.ok, found.ok ? "" : found.error);
  assert.ok(found.lines !== null);
  return found.lines;
};
const error = (path: string, symbol: string): string => {
  const found = resolveCite(root, path, symbol);
  assert.ok(!found.ok, `${path}#${symbol} resolved`);
  return found.error;
};

test("a free fn starts at its doc comment and ends at its closing brace", () => {
  const { start, end } = range("onion/src/cst.rs", "build");
  assert.equal(start, lineOf("onion/src/cst.rs", "/// Builds a CST from scanner-stamped tokens."));
  assert.equal(lines("onion/src/cst.rs")[end - 1], "}");
  assert.ok(end > lineOf("onion/src/cst.rs", "pub fn build(tokens"));
});

test("an enum takes its attributes; a variant is its own lines", () => {
  const path = "onion/src/cst.rs";
  const { start, end } = range(path, "CloseReason");
  assert.equal(start, lineOf(path, "#[repr(u8)]"));
  assert.equal(lines(path)[end - 1], "}");
  const variant = range(path, "CloseReason::Implicit");
  assert.equal(variant.start, lineOf(path, "/// The grammar ends this frame"));
  assert.equal(variant.end, lineOf(path, "Implicit = 1,"));
});

test("a method is found in whichever impl of a multi-impl file holds it", () => {
  const path = "onion/src/cst.rs";
  const impls = lines(path).flatMap((l, i) => (l.startsWith("impl CloseReason") ? [i + 1] : []));
  assert.ok(impls.length >= 2);
  const feed = range(path, "Builder::feed");
  assert.ok(feed.start > lineOf(path, "impl Builder {"));
  assert.match(lines(path).slice(feed.start - 1, feed.end).join("\n"), /fn feed\(/u);
  assert.match(error(path, "impl CloseReason"), /ambiguous in onion\/src\/cst\.rs: lines \d+, \d+/u);
  assert.match(error(path, "fmt"), /^no `fmt` in /u);
});

test("braces in a doc comment and in string literals do not move the end", () => {
  const diff = "onion/src/diff.rs";
  const revert = range(diff, "revert");
  assert.equal(revert.start, lineOf(diff, "/// Reverting one unit IS a one-decision merge: `{id: Baseline}`"));
  assert.equal(lines(diff)[revert.end - 1], "}");
  const decisions = range(diff, "Decisions");
  assert.deepEqual(decisions, { start: lineOf(diff, "/// `{unitId: side}` — the consumer contract"), end: lineOf(diff, "pub type Decisions") });

  const usj = "onion/src/usj.rs";
  const run = range(usj, "Export::run");
  assert.equal(run.start, lineOf(usj, "    fn run(&mut self) {"));
  assert.equal(lines(usj)[run.end - 1], "    }");
  assert.ok(run.end < lineOf(usj, "    fn leaf(&mut self"));
  const impl = range(usj, "impl Export");
  assert.equal(impl.start, lineOf(usj, "impl<'a> Export<'a> {"));
  assert.equal(lines(usj)[impl.end - 1], "}");
});

test("trait impls are named `impl Trait for Type`", () => {
  const path = "onion/src/cst.rs";
  const { start } = range(path, "impl Iterator for InOrder");
  assert.equal(start, lineOf(path, "impl Iterator for InOrder<'_> {"));
});

test("typescript: exports, classes and a class's method", () => {
  const path = "galley/toc-reader.ts";
  assert.deepEqual(range(path, "MAGIC").end, lineOf(path, "export const MAGIC"));
  const open = range(path, "Census.open");
  assert.equal(open.start, lineOf(path, "Validate the envelope and take a view", lineOf(path, "export class Census {")) - 1);
  assert.equal(lines(path)[open.end - 1], "  }");
  assert.equal(lines(path)[range(path, "Census").end - 1], "}");
});

test("markdown: a heading's section, up to the next heading at its level", () => {
  const { start, end } = range("GLOSSARY.md", "invariants");
  assert.equal(start, lineOf("GLOSSARY.md", "## Invariants"));
  assert.ok(end >= start);
  assert.match(error("GLOSSARY.md", "no-such-heading"), /^no heading `no-such-heading` in GLOSSARY\.md$/u);
});

test("lines, whole files, and what goes wrong", () => {
  assert.deepEqual(range("onion/src/cst.rs", "L3-L5"), { start: 3, end: 5 });
  assert.deepEqual(resolveCite(root, "onion/src/cst.rs", null), { ok: true, lines: null });
  assert.match(error("onion/src/cst.rs", "L1-L999999"), /outside it$/u);
  assert.equal(error("onion/src/gone.rs", "x"), "onion/src/gone.rs is not a file in the repo");
  assert.match(error("Cargo.toml", "workspace"), /only line ranges/u);
});

test("shown text defaults to the symbol, the line range or the path", () => {
  assert.equal(parseCite("onion/src/cst.rs#Cst::build").shown, "Cst::build");
  assert.equal(parseCite("onion/src/cst.rs#L120-L180").shown, "cst.rs:120-180");
  assert.equal(parseCite("onion/src/cst.rs").shown, "onion/src/cst.rs");
  assert.equal(parseCite("onion/src/cst.rs#build", "the build").shown, "the build");
  assert.equal(parseCite("a.rs#x").key, parseCite("a.rs#x", "other").key);
});

test("the fixture's broken citation fails the atlas check", () => {
  let out = "";
  try {
    execFileSync("node", ["scripts/atlas.ts", "check"], { env: { ...process.env, ATLAS_DIR: "/nonexistent" }, encoding: "utf8" });
  } catch (e) {
    out = (e as { stdout: string }).stdout;
  }
  assert.match(out, /atlas-fixture\/concepts\/cst\.mdx:\d+: \[\[code:onion\/src\/cst\.rs#NoSuchSymbol\]\] no `NoSuchSymbol` in onion\/src\/cst\.rs/u);
});
