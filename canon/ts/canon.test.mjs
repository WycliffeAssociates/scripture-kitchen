// The TypeScript half of canon's fixtures: the same cases tests/fixtures.rs
// runs against the Rust. `npm test` in canon/ (Node 24 strips the types).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  SCHEME_NAMES,
  book,
  books,
  chapters,
  excluded,
  fold,
  isSchemeName,
  lastVerse,
  parseBook,
  slug,
  usfmFileName,
} from "./canon.ts";

const load = (name) =>
  JSON.parse(readFileSync(new URL(`../fixtures/${name}`, import.meta.url), "utf8"));
const fixtures = load("books.json");
const schemeFixtures = load("schemes.json");

const CANONS = { protestant66: "protestant66", with_deuterocanon: "withDeuterocanon", all: "all" };

test("parse_book", () => {
  for (const row of fixtures.parse_book) {
    assert.equal(parseBook(row.text), row.book ?? undefined, `parseBook(${JSON.stringify(row.text)})`);
  }
});

test("fold", () => {
  for (const row of fixtures.fold) {
    assert.equal(fold(row.text), row.folded, `fold(${JSON.stringify(row.text)})`);
  }
});

test("book", () => {
  for (const row of fixtures.book) {
    const found = book(row.code);
    if (row.missing) {
      assert.equal(found, undefined, `book(${row.code}) should be missing`);
      continue;
    }
    assert.ok(found, `book(${row.code}) is missing`);
    assert.equal(found.usfmNumber, row.usfm_number, `${row.code} usfmNumber`);
    assert.equal(found.division, row.division, `${row.code} division`);
    assert.equal(found.canonicalIndex, row.canonical_index, `${row.code} canonicalIndex`);
    assert.equal(found.english, row.english, `${row.code} english`);
  }
});

test("usfm_file_name", () => {
  for (const row of fixtures.usfm_file_name) {
    assert.equal(usfmFileName(row.code), row.name ?? undefined, `usfmFileName(${row.code})`);
  }
});

test("books", () => {
  for (const row of fixtures.books) {
    const list = books(CANONS[row.canon]);
    assert.equal(list.length, row.count, `${row.canon} count`);
    assert.equal(list[0]?.code, row.first, `${row.canon} first`);
    assert.equal(list.at(-1)?.code, row.last, `${row.canon} last`);
  }
});

test("slug", () => {
  assert.equal(slug("MAT"), "mat");
  assert.equal(slug("1JN"), "1jn");
});

test("scheme names", () => {
  assert.deepEqual([...SCHEME_NAMES], schemeFixtures.schemes);
  for (const name of schemeFixtures.schemes) assert.ok(isSchemeName(name), name);
  for (const name of schemeFixtures.not_schemes) assert.ok(!isSchemeName(name), `${name} is not a scheme`);
});

test("chapters", () => {
  for (const row of schemeFixtures.chapters) {
    assert.equal(chapters(row.scheme, row.book), row.chapters ?? undefined, `${row.scheme}.chapters(${row.book})`);
  }
});

test("last_verse", () => {
  for (const row of schemeFixtures.last_verse) {
    assert.equal(
      lastVerse(row.scheme, row.book, row.chapter),
      row.last_verse ?? undefined,
      `${row.scheme}.lastVerse(${row.book}, ${row.chapter})`,
    );
  }
});

test("excluded", () => {
  for (const row of schemeFixtures.excluded) {
    assert.equal(
      excluded(row.scheme, row.book, row.chapter, row.verse),
      row.excluded,
      `${row.scheme}.excluded(${row.book} ${row.chapter}:${row.verse})`,
    );
  }
});
