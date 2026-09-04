import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  DIRECTORY_ENTRY_BYTES,
  FindingsSnapshot,
  FindingsSnapshotError,
  HEADER_BOOK_COUNT_OFFSET,
  HEADER_BYTES,
  HEADER_MAGIC_OFFSET,
  HEADER_PATTERN_OFFSET_OFFSET,
  HEADER_RECORD_LEN_OFFSET,
  HEADER_VERSION_OFFSET,
  MAGIC,
  FORMAT_VERSION,
  PATTERN_ROW_LEN,
  RECORD_LEN,
} from "./reader.ts";

function hexFixture(name) {
  return Uint8Array.from(
    readFileSync(new URL(`./testdata/${name}`, import.meta.url), "utf8")
      .trim()
      .split(/\s+/)
      .map((value) => Number.parseInt(value, 16)),
  );
}

function fixture() {
  return hexFixture("corpus_v1.hex");
}

// One book under "books/mrk.usfm": the header, one directory row, a 16-byte
// id table, then the pattern table, then the records.
const FIRST_RECORD = HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16;
const FIRST_MIXED_RECORD = FIRST_RECORD + 4 * PATTERN_ROW_LEN;

function expectOpenFailure(bytes) {
  assert.throws(() => FindingsSnapshot.open(bytes), FindingsSnapshotError);
}

test("opens the shared golden buffer and lazily decodes a typed row", () => {
  const bytes = fixture();
  const snapshot = FindingsSnapshot.open(bytes);
  assert.deepEqual([...snapshot.snapshotId], [...Array(16).keys()]);
  assert.equal(snapshot.coordinateSpace, "utf8");
  assert.equal(snapshot.length, 1);

  const mark = snapshot.book("MRK");
  assert.equal(mark.index, 0);
  assert.equal(mark.length, 1);
  assert.equal(mark.publishedLength, 0x200);
  assert.equal(mark.count, 1);
  assert.deepEqual(mark.at(0), {
    kind: "LengthProportionality",
    from: 0x10,
    to: 0x12,
    bookIdx: 0,
    digest: {
      bookScope: 1.5,
      projectScope: -1.5,
      saturated: true,
    },
  });
  assert.equal(snapshot.book(0).key, "MRK");
  assert.equal(snapshot.book("GEN"), undefined);
  assert.equal(mark.id, "books/mrk.usfm");
});

test("seeks by host id through the string table", () => {
  const snapshot = FindingsSnapshot.open(hexFixture("corpus_v1_utf16.hex"));
  assert.deepEqual(
    [snapshot.book(0).id, snapshot.book(1).id],
    ["books/mrk.usfm", "books/gen.usfm"],
  );
  assert.equal(snapshot.bookById("books/gen.usfm").index, 1);
  assert.equal(snapshot.bookById("books/nowhere.usfm"), undefined);

  const genesis = snapshot.findingsFor("books/gen.usfm");
  assert.equal(genesis.length, 1);
  assert.deepEqual(genesis[0], {
    kind: "LengthProportionality",
    from: 28,
    to: 37,
    bookIdx: 1,
    digest: { bookScope: -0.25, projectScope: null, saturated: false },
  });
  assert.equal(snapshot.findingsFor("books/mrk.usfm").length, 2);
  assert.deepEqual(snapshot.findingsFor("books/nowhere.usfm"), []);
});

test("decodes the galley-published UTF-16 golden with rebased spans", () => {
  // Produced by galley::sous::publish_onion_findings: caller order MRK before
  // GEN, a split-mask bounding span, an astral span, and a plain ASCII span.
  const snapshot = FindingsSnapshot.open(hexFixture("corpus_v1_utf16.hex"));
  assert.equal(snapshot.coordinateSpace, "utf16");
  assert.deepEqual([...snapshot.snapshotId], [...Array(16).keys()]);
  assert.equal(snapshot.length, 2);

  const mark = snapshot.book("MRK");
  assert.equal(mark.index, 0, "caller order wins over canonical order");
  assert.equal(mark.publishedLength, 92);
  assert.equal(mark.count, 2);
  assert.deepEqual(mark.at(0), {
    kind: "LengthProportionality",
    from: 21,
    to: 50,
    bookIdx: 0,
    digest: { bookScope: 1.5, projectScope: -1.5, saturated: true },
  });
  assert.deepEqual(mark.at(1), {
    kind: "LengthProportionality",
    from: 88,
    to: 91,
    bookIdx: 0,
    digest: { bookScope: null, projectScope: 0.5, saturated: false },
  });

  const genesis = snapshot.book("GEN");
  assert.equal(genesis.index, 1);
  assert.equal(genesis.publishedLength, 39);
  assert.deepEqual(genesis.at(0), {
    kind: "LengthProportionality",
    from: 28,
    to: 37,
    bookIdx: 1,
    digest: { bookScope: -0.25, projectScope: null, saturated: false },
  });
});

test("decodes mixed proportionality and hygiene rows, saturation included", () => {
  const snapshot = FindingsSnapshot.open(hexFixture("corpus_v1_hygiene.hex"));
  const mark = snapshot.book("MRK");
  assert.equal(mark.count, 3);
  assert.deepEqual(mark.at(0), {
    kind: "Hygiene",
    from: 3,
    to: 6,
    bookIdx: 0,
    hygiene: { class: "C0Control", run: 3, saturated: false },
  });
  assert.deepEqual(mark.at(1), {
    kind: "LengthProportionality",
    from: 0x10,
    to: 0x12,
    bookIdx: 0,
    digest: { bookScope: 1.5, projectScope: null, saturated: false },
  });
  assert.deepEqual(mark.at(2), {
    kind: "Hygiene",
    from: 0x40,
    to: 0xa0,
    bookIdx: 0,
    hygiene: { class: "Delete", run: 0x7fff, saturated: true },
  });

  const badClass = hexFixture("corpus_v1_hygiene.hex");
  badClass[FIRST_MIXED_RECORD + 12] = 11;
  assert.throws(() => FindingsSnapshot.open(badClass).book(0).at(0), FindingsSnapshotError);
  const zeroRun = hexFixture("corpus_v1_hygiene.hex");
  zeroRun[FIRST_MIXED_RECORD + 14] = 0;
  assert.throws(() => FindingsSnapshot.open(zeroRun).book(0).at(0), FindingsSnapshotError);
  const falseSaturation = hexFixture("corpus_v1_hygiene.hex");
  falseSaturation[FIRST_MIXED_RECORD + 11] = 1;
  assert.throws(() => FindingsSnapshot.open(falseSaturation).book(0).at(0), FindingsSnapshotError);
});

test("decodes the pattern table the judge published", () => {
  const snapshot = FindingsSnapshot.open(hexFixture("corpus_v1_hygiene.hex"));
  assert.equal(snapshot.patternCount, 4);
  assert.deepEqual(snapshot.patterns(), [
    {
      glyph: 0x60,
      channel: "Rarity",
      key: { kind: "Rarity" },
      band: null,
      numerator: 1,
      denominator: 48213,
      shareBp: 0,
    },
    {
      glyph: 0x3f,
      channel: "ExactNeighbor",
      key: { kind: "ExactNeighbor", neighbor: 0x2e },
      band: 2,
      numerator: 3,
      denominator: 403,
      shareBp: 74,
    },
    {
      glyph: 0x2c,
      channel: "RunShape",
      key: { kind: "RunShape", pure: false, bucket: 4 },
      band: 2,
      numerator: 1,
      denominator: 601,
      shareBp: 16,
    },
    {
      glyph: 0xffffffff,
      channel: "Placement",
      key: { kind: "Placement", side: "next", class: "Letter" },
      band: 3,
      numerator: 12,
      denominator: 9812,
      shareBp: 12,
    },
  ]);
  assert.throws(() => snapshot.pattern(4), FindingsSnapshotError);

  // Every field the reader refuses, one at a time.
  const start = FIRST_RECORD;
  for (const [offset, byte] of [[11, 1], [22, 1], [8, 1], [8, 5], [10, 0], [9, 1]]) {
    const torn = hexFixture("corpus_v1_hygiene.hex");
    torn[start + offset] = byte;
    expectOpenFailure(torn);
  }
  const moved = hexFixture("corpus_v1_hygiene.hex");
  moved[HEADER_PATTERN_OFFSET_OFFSET] = 0xff;
  expectOpenFailure(moved);
});

test("accepts a view without copying its surrounding bytes", () => {
  const bytes = fixture();
  const padded = new Uint8Array(bytes.length + 8);
  padded.set(bytes, 4);
  assert.equal(FindingsSnapshot.open(padded.subarray(4, 4 + bytes.length)).book("MRK").at(0).to, 0x12);
});

test("supports empty corpus and caller-ordered empty books", () => {
  const empty = new Uint8Array(HEADER_BYTES);
  const emptyView = new DataView(empty.buffer);
  emptyView.setUint32(HEADER_MAGIC_OFFSET, MAGIC, true);
  emptyView.setUint32(HEADER_VERSION_OFFSET, FORMAT_VERSION, true);
  emptyView.setUint32(HEADER_RECORD_LEN_OFFSET, RECORD_LEN, true);
  emptyView.setUint32(HEADER_PATTERN_OFFSET_OFFSET, HEADER_BYTES, true);
  assert.equal(FindingsSnapshot.open(empty).length, 0);

  // Two empty books: header, two directory rows, then "g" and "m" as their
  // ids (3 bytes each, padded to 8), no patterns and no records at all.
  const first = HEADER_BYTES;
  const second = HEADER_BYTES + DIRECTORY_ENTRY_BYTES;
  const idStart = HEADER_BYTES + 2 * DIRECTORY_ENTRY_BYTES;
  const books = new Uint8Array(idStart + 8);
  const view = new DataView(books.buffer);
  view.setUint32(HEADER_MAGIC_OFFSET, MAGIC, true);
  view.setUint32(HEADER_VERSION_OFFSET, FORMAT_VERSION, true);
  view.setUint32(HEADER_BOOK_COUNT_OFFSET, 2, true);
  view.setUint32(HEADER_RECORD_LEN_OFFSET, RECORD_LEN, true);
  view.setUint32(HEADER_PATTERN_OFFSET_OFFSET, books.length, true);
  books.set([71, 69, 78], first);
  books.set([77, 82, 75], second);
  view.setUint32(first + 8, books.length, true);
  view.setUint32(first + 16, idStart, true);
  view.setUint32(second + 8, books.length, true);
  view.setUint32(second + 16, idStart + 3, true);
  view.setUint16(idStart, 1, true);
  books.set([103], idStart + 2);
  view.setUint16(idStart + 3, 1, true);
  books.set([109], idStart + 5);

  assert.equal(FindingsSnapshot.open(books).book("MRK").index, 1);
  assert.equal(FindingsSnapshot.open(books).book(0).key, "GEN");
  assert.equal(FindingsSnapshot.open(books).bookById("m").key, "MRK");

  const repeatedId = books.slice();
  repeatedId[idStart + 5] = 103;
  expectOpenFailure(repeatedId);
});

test("fails closed on malformed envelope and lazily malformed rows", () => {
  const bytes = fixture();
  const badMagic = bytes.slice();
  badMagic[0] = 0;
  expectOpenFailure(badMagic);

  const badFlags = bytes.slice();
  badFlags[8] = 2;
  expectOpenFailure(badFlags);

  const truncated = bytes.slice(0, -1);
  expectOpenFailure(truncated);

  const badKey = bytes.slice();
  badKey[HEADER_BYTES] = 0xff;
  expectOpenFailure(badKey);

  const badCode = bytes.slice();
  badCode[FIRST_RECORD + 10] = 3;
  assert.throws(() => FindingsSnapshot.open(badCode).book(0).at(0), FindingsSnapshotError);

  const badFlagsRow = bytes.slice();
  badFlagsRow[FIRST_RECORD + 11] = 0x80;
  assert.throws(() => FindingsSnapshot.open(badFlagsRow).book(0).at(0), FindingsSnapshotError);

  const badBookIndex = bytes.slice();
  badBookIndex[FIRST_RECORD + 8] = 1;
  assert.throws(() => FindingsSnapshot.open(badBookIndex).book(0).at(0), FindingsSnapshotError);

  const badSpan = bytes.slice();
  badSpan[FIRST_RECORD] = 0x20;
  assert.throws(() => FindingsSnapshot.open(badSpan).book(0).at(0), FindingsSnapshotError);
});
