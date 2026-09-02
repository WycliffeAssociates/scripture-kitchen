import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  FindingsSnapshot,
  FindingsSnapshotError,
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
  badClass[56 + 12] = 11;
  assert.throws(() => FindingsSnapshot.open(badClass).book(0).at(0), FindingsSnapshotError);
  const zeroRun = hexFixture("corpus_v1_hygiene.hex");
  zeroRun[56 + 14] = 0;
  assert.throws(() => FindingsSnapshot.open(zeroRun).book(0).at(0), FindingsSnapshotError);
  const falseSaturation = hexFixture("corpus_v1_hygiene.hex");
  falseSaturation[56 + 11] = 1;
  assert.throws(() => FindingsSnapshot.open(falseSaturation).book(0).at(0), FindingsSnapshotError);
});

test("accepts a view without copying its surrounding bytes", () => {
  const bytes = fixture();
  const padded = new Uint8Array(bytes.length + 8);
  padded.set(bytes, 4);
  assert.equal(FindingsSnapshot.open(padded.subarray(4, 4 + bytes.length)).book("MRK").at(0).to, 0x12);
});

test("supports empty corpus and caller-ordered empty books", () => {
  const empty = new Uint8Array(40);
  const emptyView = new DataView(empty.buffer);
  emptyView.setUint32(0, 0x53554f53, true);
  emptyView.setUint32(4, 1, true);
  emptyView.setUint32(16, 16, true);
  assert.equal(FindingsSnapshot.open(empty).length, 0);

  const books = new Uint8Array(40 + 2 * 16);
  const view = new DataView(books.buffer);
  view.setUint32(0, 0x53554f53, true);
  view.setUint32(4, 1, true);
  view.setUint32(12, 2, true);
  view.setUint32(16, 16, true);
  books.set([71, 69, 78], 40);
  books.set([77, 82, 75], 56);
  view.setUint32(40 + 4, 0, true);
  view.setUint32(40 + 8, 72, true);
  view.setUint32(56 + 4, 0, true);
  view.setUint32(56 + 8, 72, true);
  assert.equal(FindingsSnapshot.open(books).book("MRK").index, 1);
  assert.equal(FindingsSnapshot.open(books).book(0).key, "GEN");
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
  badKey[40] = 0xff;
  expectOpenFailure(badKey);

  const badCode = bytes.slice();
  badCode[56 + 10] = 2;
  assert.throws(() => FindingsSnapshot.open(badCode).book(0).at(0), FindingsSnapshotError);

  const badFlagsRow = bytes.slice();
  badFlagsRow[56 + 11] = 0x80;
  assert.throws(() => FindingsSnapshot.open(badFlagsRow).book(0).at(0), FindingsSnapshotError);

  const badBookIndex = bytes.slice();
  badBookIndex[56 + 8] = 1;
  assert.throws(() => FindingsSnapshot.open(badBookIndex).book(0).at(0), FindingsSnapshotError);

  const badSpan = bytes.slice();
  badSpan[56] = 0x20;
  assert.throws(() => FindingsSnapshot.open(badSpan).book(0).at(0), FindingsSnapshotError);
});
