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
import { describe, markBefore } from "../galley/sous-messages.ts";
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

/** Verse text as a consumer reads it, markers and notes removed, with each
 * character's offset in the USFM. */
function verseText(usfm) {
  let text = "";
  const at = [];
  const keep = (from, to) => {
    for (let index = from; index < to; index++) {
      text += usfm[index];
      at.push(index);
    }
  };
  let last = 0;
  for (const match of usfm.matchAll(/\\(?:f|x) .*?\\(?:f|x)\*|\\(?:c|v) \d+ ?|\\\+?[a-z]+\d*\*? ?|\n/gs)) {
    keep(last, match.index);
    if (!/^\\(?:c|v|f|x)/.test(match[0])) {
      text += " ";
      at.push(match.index);
    }
    last = match.index + match[0].length;
  }
  keep(last, usfm.length);
  return { text, at };
}

/** Whether `query` matches somewhere overlapping the finding's own span. */
function matchesSite(query, usfm, finding) {
  const { text, at } = verseText(usfm);
  for (const match of text.matchAll(new RegExp(query.source, `${query.flags}g`))) {
    const from = at[match.index];
    const to = at[match.index + match[0].length - 1] + 1;
    if (from < finding.to && to > finding.from) return true;
  }
  return false;
}

test("every query is well formed, every regex compiles, and each regex finds its own site", () => {
  let regexes = 0;
  const purposes = new Set(["this", "alternative", "others"]);
  for (const { name, snapshot, book, finding, pattern, siteText } of sites()) {
    const usfm = fixture(name === "edit.bin" && book.id === "books/GEN.usfm" ? "GEN-edited.usfm" : book.id.replace("books/", ""));
    const { queries } = describe(finding, pattern, { siteText, bookCount: snapshot.length });
    for (const query of queries) {
      assert.ok(purposes.has(query.purpose));
      if (query.kind === "literal") {
        assert.ok(query.needle !== "" && typeof query.caseSensitive === "boolean" && typeof query.wholeWord === "boolean");
        continue;
      }
      assert.equal(query.kind, "regex");
      assert.equal(query.flags, "u");
      assert.doesNotThrow(() => new RegExp(query.source, query.flags), query.source);
      regexes += 1;
      if (query.purpose === "this") assert.ok(matchesSite(query, usfm, finding), `${pattern.channel} ${query.source}`);
    }
  }
  assert.ok(regexes > 0, "the goldens hold a finding with a regex query");
});

test("a regex after a mark rides quotes and brackets as the engine does", () => {
  const casing = {
    kind: "Convention", from: 0, to: 0, bookIdx: 0, convention: { pattern: 0 },
  };
  const pattern = {
    glyph: 0, channel: "Casing", key: { kind: "Casing", hash: 1n, form: "Title" }, usual: { kind: "Casing", form: "Lower", count: 9 },
    numerator: 1, denominator: 10, books: 1, shareBp: 1000,
  };
  const cases = [
    ['them; He', "bare"],
    ['said, "He', "quoted"],
    ['forever.) He', "bracketed"],
    ['it?") He', "both"],
    ['them; (He', "bare"],
  ];
  for (const [text, context] of cases) {
    const before = markBefore(text, text.length - 2);
    assert.ok(before !== undefined, text);
    const { queries } = describe(casing, pattern, { siteText: "He", bookCount: 1, before });
    const [capital, lower] = queries.filter((query) => query.kind === "regex");
    assert.equal(capital.purpose, "this", context);
    assert.match(text, new RegExp(capital.source, capital.flags), `${context}: ${capital.source}`);
    assert.doesNotMatch(text, new RegExp(lower.source, lower.flags), context);
    assert.doesNotMatch(text.replace("He", "he"), new RegExp(capital.source, capital.flags), context);
  }
  // A quoted handoff is no bare one, so a bare query never reaches across it.
  const bare = describe(casing, pattern, { siteText: "He", bookCount: 1, before: markBefore("them; He", 6) });
  assert.doesNotMatch('them;" He', new RegExp(bare.queries.find((query) => query.kind === "regex").source, "u"));
});
