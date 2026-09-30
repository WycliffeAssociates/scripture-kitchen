// galley's message modules over the committed golden publications: the
// persisted identities, the kinds and names tables, and every query.
//
//   UPDATE_IDENTITIES=1 npm test   rewrites the identity fixture; a change to
//                                  it is a change to a persisted contract and
//                                  needs a migration (galley/sous-messages.md).
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";

import { FindingsSnapshot } from "../galley/sous-reader.ts";
import { describe } from "../galley/sous-messages.ts";
import { identityOf, ordinalOf, patternIdentity, siteIdentityOf } from "../galley/sous-identity.ts";
import { kindOf } from "../galley/sous-unicode.ts";
import { codePointOf, nameOf } from "../galley/sous-unicode-names.ts";

const galley = new URL("../galley/tests/", import.meta.url);
const fixture = (name) => readFileSync(new URL(`fixtures/sous/${name}`, galley), "utf8");
const golden = (name) => new Uint8Array(readFileSync(new URL(`goldens/sous/${name}`, galley)));
const IDENTITIES = new URL("fixtures/sous/identities.txt", galley);

/** The three goldens and the text each book was published from. */
function publications() {
  const text = (edited) => (id) => fixture(edited && id === "books/GEN.usfm" ? "GEN-edited.usfm" : id.replace("books/", ""));
  return [
    ["cold.bin", FindingsSnapshot.open(golden("cold.bin")), text(false)],
    ["edit.bin", FindingsSnapshot.open(golden("edit.bin")), text(true)],
    ["knobs.bin", FindingsSnapshot.open(golden("knobs.bin")), text(false)],
  ];
}

/** The verse around `at` as a consumer might name it: the last `\c` and `\v`
 * before it, and where that verse starts. */
function verseOf(key, text, at) {
  let chapter = 0;
  let verse = 0;
  let start = 0;
  for (const match of text.matchAll(/\\(c|v) (\d+)/g)) {
    if (match.index >= at) break;
    if (match[1] === "c") [chapter, verse] = [Number(match[2]), 0];
    else verse = Number(match[2]);
    start = match.index;
  }
  return { verseRef: `${key} ${chapter}:${verse}`, start };
}

/** Every finding of every golden, with the consumer-held context. */
function* sites() {
  for (const [name, snapshot, textOf] of publications()) {
    for (let index = 0; index < snapshot.length; index++) {
      const book = snapshot.book(index);
      const text = textOf(book.id);
      for (let row = 0; row < book.count; row++) {
        const finding = book.at(row);
        const pattern = finding.kind === "Convention" ? snapshot.pattern(finding.convention.pattern) : undefined;
        const siteText = text.slice(finding.from, finding.to);
        const { verseRef, start } = verseOf(book.key, text, finding.from);
        const ordinal = ordinalOf(text.slice(start, finding.to), siteText, finding.from - start);
        yield { name, snapshot, book, finding, pattern, siteText, verseRef, ordinal };
      }
    }
  }
}

test("every golden finding's identities match the committed fixture", () => {
  const lines = [];
  for (const { name, snapshot, finding, pattern, siteText, verseRef, ordinal } of sites()) {
    const bookKey = (index) => snapshot.book(index).key;
    const site = { verseRef, siteText, ordinal };
    lines.push(`${name} ${identityOf(finding, pattern, { ...site, bookKey })}`);
    lines.push(`${name} ${siteIdentityOf(site)}`);
  }
  const fresh = `${lines.join("\n")}\n`;
  if (process.env.UPDATE_IDENTITIES === "1") writeFileSync(IDENTITIES, fresh);
  const held = readFileSync(IDENTITIES, "utf8");
  assert.ok(lines.length > 20, "the goldens hold findings");
  assert.equal(
    fresh,
    held,
    "the identity format changed: identities are saved by consumers, so this needs a migration (galley/sous-messages.md, Stable identity)",
  );
});

test("a pattern identity names the claim and none of its strength", () => {
  for (const { pattern } of sites()) {
    if (pattern === undefined) continue;
    const id = patternIdentity(pattern, { bookKey: () => "BOOK" });
    assert.match(id, /^v1:p:[A-Za-z]+:/);
    for (const moved of [pattern.numerator, pattern.denominator, pattern.shareBp]) {
      assert.ok(!id.split(":").includes(String(moved)) || moved < 10, `${id} holds a count`);
    }
  }
  const pattern = { glyph: 0x2014, key: { kind: "Placement", side: "next", class: "Digit" } };
  assert.equal(patternIdentity(pattern), "v1:p:Placement:2014:next:Digit");
  assert.throws(() => patternIdentity({ glyph: 0x2c, key: { kind: "BookRate", side: "prev", class: "Letter", book: 3 } }));
});

test("fields escape their separator and ordinals count earlier starts", () => {
  const site = { verseRef: "GEN 1:2", siteText: "a:b%\n", ordinal: 1 };
  assert.equal(siteIdentityOf(site), "v1:s:GEN 1%3A2:a%3Ab%25%0A:1");
  assert.equal(ordinalOf("a a, a", "a", 0), 0);
  assert.equal(ordinalOf("a a, a", "a", 5), 2);
  assert.equal(ordinalOf("aaa", "aa", 1), 1);
  assert.throws(() => siteIdentityOf({ ...site, ordinal: -1 }));
});

test("the kinds and names tables answer from UCD 17.0.0", () => {
  const kinds = { "'": "quote", "“": "quote", "(": "bracket", ")": "bracket", "—": "dash", "−": "dash", "?": "sentenceEnd",
    "।": "sentenceEnd", ",": "separator", ";": "separator", "5": "digit", "©": "symbol", "§": "other", a: "letter", " ": "space", "*": "other" };
  for (const [char, kind] of Object.entries(kinds)) assert.equal(kindOf(char.codePointAt(0)), kind, char);
  assert.equal(nameOf(0x2013), "en dash");
  assert.equal(nameOf(0x2d), "hyphen-minus");
  assert.equal(nameOf(0x61), undefined);
  assert.equal(codePointOf(0x2d), "U+002D");
  assert.equal(codePointOf(0x1f600), "U+1F600");
});

test("every golden finding describes with plain params", () => {
  for (const { snapshot, finding, pattern, siteText } of sites()) {
    const { params } = describe(finding, pattern, { siteText, bookCount: snapshot.length });
    for (const value of Object.values(params)) assert.ok(["string", "number", "boolean"].includes(typeof value));
  }
});
