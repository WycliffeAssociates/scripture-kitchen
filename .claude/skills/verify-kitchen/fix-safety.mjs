#!/usr/bin/env node
// node .claude/skills/verify-kitchen/fix-safety.mjs [--write <dir>] <file.usfm | dir>...
// Publishes, groups the hygiene rows, applies every group's fix, republishes,
// and fails unless each fixed row is gone, no group gains a row, and every
// deleted scalar is of its row's class. A row with no fix may still go when
// a neighbour's fix restores its context (a ZWNJ behind a removed DEL).
// Prints rows -> groups and the counts.
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
const args = process.argv.slice(2);
const writeTo = args[0] === "--write" ? args[1] : undefined;
const inputs = writeTo === undefined ? args : args.slice(2);
if (inputs.length === 0) {
  console.error("usage: fix-safety.mjs [--write <dir>] <file.usfm | dir>...");
  process.exit(2);
}

const { Galley } = (await import(join(root, "galley/pkg-node/usfm_galley.js"))).default;
const { FindingsSnapshot, hygieneFix } = await import(join(root, "galley/sous-reader.ts"));

const files = inputs.flatMap((input) =>
  statSync(input).isDirectory()
    ? readdirSync(input).filter((name) => /\.u?sfm$/i.test(name)).sort().map((name) => join(input, name))
    : [input],
);
const original = new Map(files.map((file) => [basename(file), readFileSync(file, "utf8").replace(/\r\n?/g, "\n")]));

const MEMBER = {
  C0Control: (c) => c < 0x20 && c !== 0x09 && c !== 0x0a,
  Delete: (c) => c === 0x7f,
  NoBreakSpace: (c) => c === 0xa0,
  MisplacedFormat: (c) => /\p{Cf}/u.test(String.fromCodePoint(c)),
  Noncharacter: (c) => (c >= 0xfdd0 && c <= 0xfdef) || (c & 0xfffe) === 0xfffe,
};

function publish(texts) {
  const galley = new Galley();
  for (const [id, text] of texts) galley.update(id, text);
  return FindingsSnapshot.open(galley.publish());
}

const keyOf = (snapshot, group) =>
  `${snapshot.book(group.key.book).id} ${group.key.class} ${group.key.placement}`;

function tally(snapshot) {
  return new Map(snapshot.hygieneGroups({ scope: "book" }).map((group) => [keyOf(snapshot, group), group.count]));
}

const before = publish(original);
const groups = before.hygieneGroups({ scope: "book" });
const project = before.hygieneGroups({ scope: "project" });
const rows = groups.reduce((sum, group) => sum + group.count, 0);
const fixedRows = new Map();
const deletions = new Map();
for (const group of groups) {
  for (let index = 0; index < group.count; index++) {
    const row = group.at(index);
    const deletion = hygieneFix(row, before.coordinateSpace);
    if (deletion === null) continue;
    const key = keyOf(before, group);
    fixedRows.set(key, (fixedRows.get(key) ?? 0) + 1);
    const id = before.book(deletion.bookIdx).id;
    deletions.set(id, [...(deletions.get(id) ?? []), { ...deletion, hygieneClass: row.hygiene.class }]);
  }
}

const deleted = new Map();
const fixed = new Map(original);
for (const [id, list] of deletions) {
  let text = original.get(id);
  let floor = Infinity;
  for (const { from, to, hygieneClass } of list.sort((a, b) => b.from - a.from)) {
    if (to > floor) throw new Error(`${id}: deletions overlap at ${from}..${to}`);
    floor = from;
    for (const ch of text.slice(from, to)) {
      const c = ch.codePointAt(0);
      if (!MEMBER[hygieneClass](c)) {
        throw new Error(`${id} ${from}..${to}: deletes U+${c.toString(16)}, which is no ${hygieneClass}`);
      }
      const code = `U+${c.toString(16).toUpperCase().padStart(4, "0")}`;
      deleted.set(code, (deleted.get(code) ?? 0) + 1);
    }
    text = text.slice(0, from) + text.slice(to);
  }
  fixed.set(id, text);
}

const after = publish(fixed);
const was = tally(before);
const now = tally(after);
let cleared = 0;
for (const [key, count] of was) {
  const most = count - (fixedRows.get(key) ?? 0);
  const left = now.get(key) ?? 0;
  if (left > most) throw new Error(`${key}: ${left} rows after, at most ${most}`);
  cleared += most - left;
}
for (const key of now.keys()) {
  if (!was.has(key)) throw new Error(`${key} appears only after the fix`);
}
let lettersLost = 0;
for (const [id, text] of original) {
  const kept = fixed.get(id);
  const letters = (s) => (s.match(/[\p{L}\p{M}\p{N}]/gu) ?? []).length;
  lettersLost += letters(text) - letters(kept);
}

if (writeTo !== undefined) {
  mkdirSync(writeTo, { recursive: true });
  for (const [id, text] of fixed) writeFileSync(join(writeTo, id), text);
}

const top = (scope) =>
  scope
    .map((group) => ({ group, count: group.count }))
    .sort((a, b) => b.count - a.count)
    .slice(0, 6)
    .map(({ group }) => `${group.key.class}/${group.key.placement} x${group.count} fix ${group.fix()?.length ?? "none"}`);
console.log(
  JSON.stringify(
    {
      books: files.length,
      hygieneRows: rows,
      groupsByBook: groups.length,
      groupsByProject: project.length,
      projectGroups: top(project),
      fixedRows: [...fixedRows.values()].reduce((a, b) => a + b, 0),
      deletedScalars: Object.fromEntries(deleted),
      lettersLost,
      rowsAfter: [...now.values()].reduce((a, b) => a + b, 0),
      unfixedRowsANeighbourCleared: cleared,
      groupsAfter: now.size,
    },
    null,
    1,
  ),
);
