#!/usr/bin/env node
// node .claude/skills/verify-kitchen/findings.mjs <out.jsonl> <file.usfm | dir>...
// Registers every book on one Galley, publishes, and writes one JSON line per
// finding: book id, the message id and params Sefer would render, the site text.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
const [out, ...inputs] = process.argv.slice(2);
if (!out || inputs.length === 0) {
  console.error("usage: findings.mjs <out.jsonl> <file.usfm | dir>...");
  process.exit(2);
}

const { Galley } = (await import(join(root, "galley/pkg-node/usfm_galley.js"))).default;
const { FindingsSnapshot } = await import(join(root, "galley/sous-reader.ts"));
const { describe } = await import(join(root, "galley/sous-messages.ts"));

const files = inputs.flatMap((input) =>
  statSync(input).isDirectory()
    ? readdirSync(input).filter((name) => /\.u?sfm$/i.test(name)).sort().map((name) => join(input, name))
    : [input],
);

const galley = new Galley();
const texts = new Map();
for (const file of files) {
  const text = readFileSync(file, "utf8").replace(/\r\n?/g, "\n");
  const id = basename(file);
  texts.set(id, text);
  galley.update(id, text);
}

const snapshot = FindingsSnapshot.open(galley.publish());
const lines = [];
for (let index = 0; index < snapshot.length; index++) {
  const book = snapshot.book(index);
  const source = texts.get(book.id);
  for (let row = 0; row < book.count; row++) {
    const finding = book.at(row);
    const pattern = finding.kind === "Convention" ? snapshot.pattern(finding.convention.pattern) : undefined;
    const siteText = source.slice(finding.from, finding.to);
    const { id, params } = describe(finding, pattern, { siteText, bookCount: snapshot.length });
    lines.push(JSON.stringify({ book: book.id, from: finding.from, to: finding.to, id, siteText, params }));
  }
}
writeFileSync(out, lines.join("\n") + (lines.length ? "\n" : ""));
console.log(`${files.length} books, ${lines.length} findings -> ${out}`);
