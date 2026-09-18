/**
 * The generated reader against the generated writer, across the wall.
 *
 *   wasm-pack build --target nodejs --release --out-dir pkg-node
 *   node onion-wasm/tests/conformance.mjs pkg-node [corpus-dir]
 *
 * Node 24 strips types, so this imports `reader.ts` as shipped — no build step,
 * and no second copy of the reader to fall out of date.
 *
 * The Rust half of this lives in `onion/tests/wire_roundtrip.rs`, which reads
 * every field back against the engine's own values. What is left, and what this
 * asserts, is that the TYPESCRIPT reader agrees: same counts, same spans, a
 * tree that tiles, and markers that resolve to the same rows.
 */
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

const pkg = resolve(process.argv[2] ?? "pkg-node");
const corpus = process.argv[3];
const here = import.meta.dirname;

const {
  parse: rawParse,
  attrs: rawAttrs,
  attrResolve,
  diff: rawDiff,
  mask: rawMask,
} = await import(join(pkg, "onion_wasm.js"));
const {
  reader,
  deserialize,
  deserializeCorpus,
  declaredVersion,
  MARKERS,
  CODES,
  TokenKind,
  StructuralWhitespaceRequirement,
  MalformedAttr,
  AttrResolution,
  attrList,
  NONE,
} = await import(resolve(here, "../reader.ts"));

// Everything below goes through the typed door, which is what a consumer uses;
// `deserialize` is exercised directly once, at the end, to prove the raw shape
// still reads.
const onion = reader(rawParse);
const parse = (text, diagnostics, toc, utf16) => onion.parse(text, { diagnostics, toc, utf16 });

let failures = 0;
const check = (ok, what) => {
  if (!ok) {
    console.error(`  FAIL  ${what}`);
    failures++;
  }
};
const eq = (a, b, what) => check(a === b, `${what}: ${a} !== ${b}`);

const BOOK = `\\id GEN
\\usfm 3.0
\\c 1
\\s The beginning
\\p \\v 1 In the beginning\\f + \\ft a note\\f* God created.
\\q1 \\v 2-3 And the earth was without form.
\\c 2
\\p \\v 1 Thus the heavens were finished.
`;

// --- the tree tiles -------------------------------------------------------
{
  const { tree, tokens } = (parse(BOOK, false, false, false));
  const seen = new Set();
  let nodes = 0;
  for (const item of tree.walk()) {
    if (item.isNode) nodes++;
    else {
      check(!seen.has(item.id), `token ${item.id} visited twice`);
      seen.add(item.id);
    }
  }
  eq(seen.size, tokens.length, "every token is reached exactly once by walk()");
  check(nodes > 0, "the walk found nodes");

  // Every token's span is inside the document and ascends.
  let previous = 0;
  for (const t of tokens) {
    const s = t.span();
    check(s.from >= previous, `token ${t.id} span ascends`);
    check(s.to >= s.from, `token ${t.id} span is not inverted`);
    previous = s.from;
  }
  eq(tokens.at(tokens.length - 1).span().to, BOOK.length, "the last token ends at EOF");
}

// --- nodes contain their children ----------------------------------------
{
  const { tree } = (parse(BOOK, false, false, false));
  const visit = (node) => {
    const outer = node.span();
    for (let i = 0; i < node.childCount(); i++) {
      const child = node.child(i);
      const inner = child.span();
      check(
        inner.from >= outer.from && inner.to <= outer.to,
        `child ${i} of node ${node.id} escapes its parent`,
      );
      if (child.isNode) visit(child);
    }
  };
  visit(tree.root());

  // nodeAt lands inside what it names, and finds the INNERMOST.
  for (const pos of [0, 10, 40, 80, BOOK.length - 2]) {
    const node = tree.nodeAt(pos);
    if (node === null) continue;
    const s = node.span();
    check(s.from <= pos && pos < s.to, `nodeAt(${pos}) does not contain ${pos}`);
  }
}

// --- markers resolve through the table, not the document ------------------
{
  const { tokens } = (parse(BOOK, false, false, false));
  const names = new Set();
  for (const t of tokens) {
    const m = t.marker();
    if (m && !m.isUnknown()) names.add(m.name());
  }
  for (const want of ["id", "usfm", "c", "s", "p", "v", "f", "ft", "q"]) {
    check(
      [...names].some((n) => n === want || n.startsWith(want)),
      `the table resolved \\${want} without slicing the document`,
    );
  }
  eq(MARKERS.length, 153, "the whole marker table shipped");
  check(MARKERS[0].name === "", "row 0 has no name");

  // `\b` is the one marker taking no content on its own line.
  const blankLine = MARKERS.findIndex((row) => row.name === "b");
  eq(
    MARKERS[blankLine].ws,
    StructuralWhitespaceRequirement.SingleNewline,
    "the whitespace rule crossed with the row",
  );
}

/**
 * Reassemble the punctuation `spelling()` drops and the document's own bytes
 * come back. Holds over a real book, where `\+nd`, milestones and unknown `\z`
 * markers actually occur. UTF-16 offsets, because a JS string indexes that way.
 */
const spellsBackToTheDocument = (doc, tokens, where) => {
  for (const t of tokens) {
    const spelled = t.spelling(doc);
    if (t.marker() === null) {
      eq(spelled, "", `${where}: token ${t.id} is no marker, so it spells nothing`);
      continue;
    }
    const plus = t.spelled() && t.kind() !== TokenKind.Milestone ? "+" : "";
    const star = t.kind() === TokenKind.ClosingMarker ? "*" : "";
    eq(
      `\\${plus}${spelled}${star}`,
      doc.slice(t.span().from, t.payloadEnd()),
      `${where}: token ${t.id} does not spell back to its own bytes`,
    );
  }
};

// --- the flags byte: the fold, and the blank run --------------------------
{
  const dish = onion.parse(BOOK, { utf16: true });
  const { tokens } = dish;
  eq(dish.sourceLength, BOOK.length, "the source's own length crossed");
  check(typeof dish.sourceHash === "bigint" && dish.sourceHash !== 0n, "…and its hash");

  let folded = 0;
  for (const t of tokens) {
    const { from, to } = t.span();
    if (t.delimiterFolded()) {
      folded++;
      check(" \t".includes(BOOK[to - 1]), `token ${t.id} claims a fold it does not carry`);
      eq(t.payloadEnd(), to - 1, `token ${t.id} payload stops short of the delimiter`);
    } else {
      eq(t.payloadEnd(), to, `token ${t.id} payload is the whole span`);
    }
    if (t.isBlank()) {
      eq(t.kind(), TokenKind.Text, `token ${t.id} is blank but not Text`);
      eq(BOOK.slice(from, to).trim(), "", `token ${t.id} is not blank after all`);
    }
  }
  check(folded > 0, "`\\c ` and `\\v ` fold their delimiter");
  spellsBackToTheDocument(BOOK, tokens, "BOOK");
}

// --- the LEVEL crosses, so nobody re-parses a marker's digits -------------
//
// `\q1` and `\q2` are the SAME table row: the row carries the cap, the token
// carries the level. Until it did, a consumer re-parsed the marker's spelling
// to tell them apart, and the level drives a CSS class with real layout behind
// it — so this is the last markup fact that made a consumer read a document
// byte. The law is document-against-row: whenever a token reports a level, the
// bytes it spans END with that number, and nothing but a marker reports one.
const levelIsSpelled = (doc, tokens, where) => {
  for (const t of tokens) {
    if (!t.marker()) {
      eq(t.level(), 0, `${where}: token ${t.id} is no marker, so it has no level`);
      continue;
    }
    if (t.level() === 0) continue;
    const { from, to } = t.span();
    const name = doc.slice(from, to).trimEnd().replace(/\*$/, "").replace(/-[se]$/, "");
    check(
      name.endsWith(String(t.level())),
      `${where}: token ${t.id} reports level ${t.level()}, spelled ${JSON.stringify(name)}`,
    );
  }
};

{
  const LEVELS = `\\id GEN
\\mt2 A subtitle
\\c 1
\\q1 \\v 1 first line
\\q2 second line
\\q third line
\\tr \\tc1 first column \\tc2 second
`;
  const { tree, tokens } = parse(LEVELS, false, false, false);
  levelIsSpelled(LEVELS, tokens, "levels");

  const spelled = [];
  for (const t of tokens) {
    const m = t.marker();
    if (m && !m.isUnknown()) spelled.push(`${m.name()}${t.level() || ""}`);
  }
  eq(
    spelled.join(" "),
    "id mt2 c q1 v q2 q tr tc1 tc2",
    "the number rides the row it belongs to, column indices included",
  );

  // A node's level is its opening marker's. Held against `child(0)` because
  // that is the expression a consumer wrote before the field existed.
  for (const node of tree.walkNodes()) {
    const opener = node.childCount() ? node.child(0) : null;
    if (!opener || opener.isNode) continue;
    eq(node.level(), opener.level(), `node ${node.id}: level is its opener's`);
  }
}

// --- every getter reads the WIRE, not the cursor --------------------------
//
// The generated row classes are cursors: they hold a position AND expose a
// getter per wire field. A wire field named like a cursor member would have
// been shadowed, and the getter would have returned the cursor's own state —
// silently, with a plausible-looking small integer. `VerseRow.at` is exactly
// that name. Cross-checking a getter against a value reachable another way is
// the only thing that catches it; a type stripper never typechecks, and a
// row-count assertion passes either way.
{
  const { toc, tokens } = parse(BOOK, false, true, false);
  for (const v of toc.verses()) {
    const marker = tokens.at(v.token).span();
    eq(v.at, marker.from, `verse ${v.first}: .at must be the anchor's offset`);
    check(v.at > 0, `verse ${v.first}: .at is not a row index`);
  }
  const chapters = toc.chapters();
  for (const c of chapters) {
    check(c.to > c.from || c.from === 0, `chapter ${c.number}: extent is not inverted`);
  }
  // Chapters tile: each starts where the last ended.
  for (let i = 1; i < chapters.length; i++) {
    eq(chapters[i].from, chapters[i - 1].to, `chapter ${chapters[i].number} abuts the last`);
  }
}

// --- the designator crosses, so nobody re-walks the token stream ----------
{
  const { toc, tokens } = parse(BOOK, false, true, false);
  for (const c of toc.chapters()) {
    if (c.token === 0xffffffff) {
      eq(c.designator, 0xffffffff, "the front-matter row has no designator");
      continue;
    }
    const d = tokens.at(c.designator);
    eq(d.kind(), TokenKind.Designator, `chapter ${c.number}: designator is a Designator`);
    check(c.designator > c.token, `chapter ${c.number}: it follows its marker`);
  }
  for (const v of toc.verses()) {
    const d = tokens.at(v.designator);
    eq(d.kind(), TokenKind.Designator, `verse ${v.first}: designator is a Designator`);
  }
}

// --- the declared version crosses, because it gates every finding ---------
{
  const declared = parse(BOOK, false, false, false);
  eq(declaredVersion(declared), "3.0", "the \\usfm line was read");
  const silent = parse("\\id GEN\n\\c 1\n\\p \\v 1 a\n", false, false, false);
  eq(declaredVersion(silent), null, "a document declaring none says so");
}

// --- walkNodes is document order, and only nodes --------------------------
{
  const { tree } = parse(BOOK, false, false, false);
  const flat = [...tree.walkNodes()];
  eq(flat.length, tree.nodeCount(), "every node, once");
  let previous = -1;
  for (const node of flat) {
    check(node.isNode, "walkNodes yields only nodes");
    const from = node.span().from;
    check(from >= previous, `node ${node.id}: document order`);
    previous = from;
  }
  // The same nodes the arena walk finds, and no tokens.
  const viaArena = new Set();
  for (const item of tree.walk()) if (item.isNode) viaArena.add(item.id);
  for (const node of flat) if (node.id !== 0) check(viaArena.has(node.id), `node ${node.id} agrees`);
}

// --- the toc forEach twins agree with the allocating readers --------------
{
  const { toc } = parse(BOOK, false, true, false);
  const rows = [];
  toc.forEachChapter((number, from, to) => { rows.push([number, from, to]); });
  eq(
    JSON.stringify(rows),
    JSON.stringify(toc.chapters().map((c) => [c.number, c.from, c.to])),
    "forEachChapter agrees with chapters()",
  );
  const verses = [];
  toc.forEachVerse((chapter, first, last, at) => { verses.push([chapter, first, last, at]); });
  eq(
    JSON.stringify(verses),
    JSON.stringify(toc.verses().map((v) => [v.chapter, v.first, v.last, v.at])),
    "forEachVerse agrees with verses()",
  );
}

// --- the toc counts what a hand count counts ------------------------------
{
  const { toc } = (parse(BOOK, false, true, false));
  const chapters = toc.chapters();
  eq(chapters.length, 3, "front matter plus two chapters");
  eq(chapters[1].number, 1, "chapter 1");
  eq(chapters[2].number, 2, "chapter 2");
  const verses = toc.verses();
  eq(verses.length, 3, "three verse anchors");
  eq(verses[1].first, 2, "the bridge starts at 2");
  eq(verses[1].last, 3, "…and ends at 3");
  const at = toc.at(chapters[2].from + 4);
  eq(at.chapter, 2, "at() lands in chapter 2");
}

// --- diagnostics resolve through the catalog ------------------------------
{
  const bad = "\\id GEN\n\\c 1\n\\p \\v 1 unclosed \\add here\n";
  const { diagnostics } = (parse(bad, true, false, false));
  check(diagnostics.length > 0, "an unclosed \\add reports");
  const d = diagnostics.at(0);
  check(typeof d.code().name === "string", "the finding names a catalog row");
  const slice = (from, to) => bad.slice(from, to);
  check(d.message(slice).length > 0, "the message renders from the document");
  check(d.severity(null) !== undefined, "the severity ladder resolves");
  eq(CODES.length, 59, "the whole catalog shipped");
}

// --- utf16 is opt-in and moves the right things ---------------------------
{
  const hindi = "\\id MAT\n\\c 1\n\\p \\v 1 अब्राहम की सन्तान\n";
  const bytes = (parse(hindi, false, false, false));
  const units = (parse(hindi, false, false, true));
  check(!bytes.utf16 && units.utf16, "the flag reports the space");
  eq(bytes.tokens.length, units.tokens.length, "the same tokens either way");

  let moved = false;
  for (let i = 0; i < bytes.tokens.length; i++) {
    const b = bytes.tokens.at(i).span();
    const u = units.tokens.at(i).span();
    check(u.from <= b.from, `token ${i}: a UTF-16 offset never exceeds a byte one`);
    moved ||= u.from !== b.from;
    eq(bytes.tokens.at(i).kind(), units.tokens.at(i).kind(), `token ${i} kind is not an offset`);
  }
  check(moved, "Devanagari must move some offset");

  // The last token's end is the document length in whichever space.
  eq(bytes.tokens.at(bytes.tokens.length - 1).span().to, Buffer.byteLength(hindi), "byte EOF");
  eq(units.tokens.at(units.tokens.length - 1).span().to, hindi.length, "UTF-16 EOF");
}

// --- the edges and the ownership map agree with the walk ------------------
{
  const { tree, tokens } = (parse(BOOK, false, false, false));
  const owners = tree.owners();
  eq(owners.length, tokens.length, "one owner slot per token");
  check([...owners].every((owner) => owner !== NONE), "every token is owned");
  eq(tree.parent(0), NONE, "the root has no parent");

  for (let node = 0; node < tree.nodeCount(); node++) {
    const span = tree.extent(node);
    const first = tree.firstToken(node);
    const last = tree.lastToken(node);
    eq(tokens.at(first).span().from, span.from, `node ${node} starts at firstToken`);
    eq(tokens.at(last).span().to, span.to, `node ${node} ends at lastToken`);
    if (node === 0) continue;
    // Every parent chain reaches the root, and a parent's extent contains it.
    const parent = tree.parent(node);
    const outer = tree.extent(parent);
    check(
      outer.from <= span.from && span.to <= outer.to,
      `node ${node} escapes its parent ${parent}`,
    );
  }

  // The owner is the INNERMOST node, which is to say the one whose own child
  // list the token is in.
  for (const t of tokens) {
    const owner = owners[t.id];
    const r = tree.nodes.seek(owner);
    const slots = tree.childIds.subarray(r.childFrom, r.childTo);
    eq(
      slots.filter((id) => id === t.id).length,
      1,
      `token ${t.id} is not a direct child of its owner ${owner}`,
    );
  }
}

// --- the attribute view, through its typed wrapper ------------------------
//
// UTF-16 throughout: the span goes in in the caller's space and every word
// comes back in it, so the document's own `slice` reads the answer.
{
  const doc = "\\p \u{1d11e}\n\\w grace|lemma=\"grace\" x-y=\"z\"\\w*\n";
  const { tokens } = onion.parse(doc, { utf16: true });
  const listOf = (text, tks) => {
    for (const t of tks) if (t.kind() === TokenKind.AttrList) return t.span();
    throw new Error("no attribute list");
  };
  const span = listOf(doc, tokens);
  const list = attrList(rawAttrs(doc, span.from, span.to, 1));
  eq(list.attrs.length, 2, "two attributes came back");
  eq(doc.slice(list.attrs[0].name.from, list.attrs[0].name.to), "lemma", "the name span");
  eq(doc.slice(list.attrs[0].value.from, list.attrs[0].value.to), "grace", "the value span");
  eq(doc.slice(list.attrs[1].name.from, list.attrs[1].name.to), "x-y", "the second name");
  check(list.malformed === undefined, "a clean list reports nothing malformed");

  // A bare value: an empty name span at the value's start.
  const bare = "\\w In|in\\w*";
  const bareList = attrList(rawAttrs(bare, listOf(bare, onion.parse(bare, {}).tokens).from, bare.length, 0));
  eq(bareList.attrs.length, 1, "one bare value");
  eq(bareList.attrs[0].name.from, bareList.attrs[0].name.to, "…with an empty name");

  // An unterminated quote ends the walk with one finding.
  const broken = '\\w x|lemma="grace\\w*';
  const brokenSpan = listOf(broken, onion.parse(broken, {}).tokens);
  const brokenList = attrList(rawAttrs(broken, brokenSpan.from, brokenSpan.to, 0));
  eq(brokenList.attrs.length, 0, "nothing parsed before the break");
  eq(brokenList.malformed.code, MalformedAttr.UnterminatedQuote, "the code names the break");
  eq(broken[brokenList.malformed.at], '"', "…at the opening quote");

  const w = MARKERS.findIndex((row) => row.name === "w");
  eq(attrResolve("lemma", w), AttrResolution.Defined, "the table knows `lemma`");
  eq(attrResolve("x-strong", w), AttrResolution.UserNamespace, "…and the user namespace");
  eq(attrResolve("nonesuch", w), AttrResolution.Unknown, "…and what it does not know");
}

// --- forEach agrees with the iterator -------------------------------------
{
  const { tokens } = (parse(BOOK, false, false, false));
  const slow = [...tokens].map((t) => [t.kind(), t.span().from, t.span().to]);
  const fast = [];
  tokens.forEach((kind, _marker, from, to) => {
    fast.push([kind & ~16, from, to]);
  });
  eq(JSON.stringify(slow), JSON.stringify(fast), "forEach and the iterator agree");
}

// --- the whole corpus, if it is here --------------------------------------
if (corpus && existsSync(corpus)) {
  let books = 0;
  for (const name of readdirSync(corpus).sort()) {
    if (!/\.usfm$/i.test(name)) continue;
    const text = readFileSync(join(corpus, name), "utf8").replace(/\r\n/g, "\n");
    const { tree, tokens, toc } = (parse(text, true, true, true));
    const seen = new Set();
    for (const item of tree.walk()) if (!item.isNode) seen.add(item.id);
    check(seen.size === tokens.length, `${name}: the walk reaches every token`);
    check(toc.chapters().length > 0, `${name}: has a chapter table`);
    levelIsSpelled(text, tokens, name);
    spellsBackToTheDocument(text, tokens, name);
    books++;
  }
  console.log(`corpus: ${books} books walked`);
}

// The raw door, once: `deserialize` over bytes the wrapper did not unpack.
{
  const dish = deserialize(rawParse(BOOK, false, true, false));
  eq(dish.toc.chapters().length, 3, "deserialize reads a raw dish too");
  check(!dish.utf16, "and reports its addressing");
}

// --- the corpus envelope frames, it does not re-encode -------------------
//
// Read from a fixture `galley::corpus`'s own tests write, so this holds the
// reader to bytes the RUST writer produced. Building the envelope here in JS
// would only prove the test agrees with itself.
{
  const fixture = resolve(here, "../../target/corpus-fixture.bin");
  if (!existsSync(fixture)) {
    console.log("corpus: fixture absent — run `cargo test -p usfm_galley corpus` first");
  } else {
    const corpus = deserializeCorpus(new Uint8Array(readFileSync(fixture)));
    eq(corpus.length, 2, "two books came back");
    eq(corpus[0].key, "GEN", "the key is carried verbatim");
    eq(corpus[1].key, "books/02 — Exodus.usfm", "…including one that is a path");
    const gen = corpus[0].dish;
    check(gen.toc.chapters().length > 0, "a framed dish reads like any other");
    check(gen.tokens.length > 0, "…with its tokens intact");
    for (const item of gen.tree.walk()) {
      if (item.isNode) continue;
      check(item.span().to >= item.span().from, "…and a walkable tree");
      break;
    }
  }
}

// --- the range questions, against Rust's own answers ----------------------
//
// `onion/tests/dish_queries.rs` computes every entry through `Cst::extent`
// and `Cst::owners` and writes it out. The reader has the same parts and does
// the same walk; if the two ever disagree, one of them is wrong here rather
// than in a consumer.
{
  const dir = resolve(here, "../../testData/goldens/dish-queries");
  if (!existsSync(dir)) {
    throw new Error(`${dir} is absent — run \`cargo test -p usfm_onion --test dish_queries\``);
  }
  let entries = 0;
  for (const name of readdirSync(dir).sort()) {
    if (!name.endsWith(".json")) continue;
    const golden = JSON.parse(readFileSync(join(dir, name), "utf8"));
    const path =
      golden.source === "(inline: SHAPES)"
        ? join(dir, "shapes.usfm")
        : resolve(here, "../..", golden.source);
    const text = readFileSync(path, "utf8").replace(/\r\n/g, "\n");
    // Byte offsets, because that is the space the goldens are in.
    const { tree, tokens, sourceLength } = parse(text, false, false, false);
    for (const want of golden.entries) {
      const got = tree.enclosing(want.from, want.to);
      check(
        got.from === want.enclosing.from &&
          got.to === want.enclosing.to &&
          got.marker === want.enclosing.marker,
        `${name} ${want.from}..${want.to}: enclosing ${got.from}..${got.to}/${got.marker} ` +
          `!== ${want.enclosing.from}..${want.enclosing.to}/${want.enclosing.marker}`,
      );
      eq(tree.inMarkup(want.from, want.to), want.inMarkup, `${name} ${want.from}..${want.to}: inMarkup`);
      entries++;
    }

    // `tokenAt` is total inside the document and refuses outside it, and
    // `spansIn` over a whole token is that token.
    for (let i = 0; i < tokens.length; i += Math.max(1, Math.floor(tokens.length / 200))) {
      const { from, to } = tokens.at(i).span();
      eq(tree.tokenAt(from), i, `${name}: tokenAt(start of ${i})`);
      eq(tree.tokenAt(to - 1), i, `${name}: tokenAt(last byte of ${i})`);
      const spans = tree.spansIn(from, to);
      eq(spans.length, 1, `${name}: spansIn over token ${i} is one span`);
      eq(spans[0].from, from, `${name}: …starting where it does`);
      eq(spans[0].to, to, `${name}: …and ending there`);
      eq(spans[0].kind, tokens.at(i).kind(), `${name}: …with its kind`);
    }
    let threw = false;
    try {
      // `sourceLength`, not `text.length`: the dish is in bytes here and a JS
      // string counts UTF-16, so a non-ASCII book would still be inside.
      tree.tokenAt(sourceLength);
    } catch {
      threw = true;
    }
    check(threw, `${name}: tokenAt past the document throws`);
  }
  console.log(`dish queries: ${entries} golden entries reproduced`);
}

// --- diff runs tile their unit and rebuild the text cut -------------------
//
// L1 and L2 over the JSON, in the UTF-16 offsets the door speaks. A run
// carries no bytes of its own — `source.slice(from, to)` is its text.
{
  const baseline = BOOK;
  const current = BOOK.replace("God created.", "God \\add truly\\add* made.").replace(
    "\\q1 \\v 2-3",
    "\\q2 \\v 2-3",
  );
  const skeleton = JSON.parse(rawDiff(baseline, current, "words"));
  let runs = 0;
  let markupSeen = false;
  for (const unit of skeleton.units) {
    if (!unit.text) continue;
    for (const [side, source] of [
      ["baseline", baseline],
      ["current", current],
    ]) {
      const list = unit.text[side];
      if (list.length === 0) continue;
      const span = unit[side];
      eq(list[0].from, span[0], `${unit.unitId} ${side}: runs open the unit`);
      eq(list[list.length - 1].to, span[1], `${unit.unitId} ${side}: …and close it`);
      let reading = "";
      for (let i = 0; i < list.length; i++) {
        const run = list[i];
        if (i > 0) eq(list[i - 1].to, run.from, `${unit.unitId} ${side}: run ${i} is gapless`);
        if (run.what === "markup") markupSeen = true;
        else reading += source.slice(run.from, run.to);
        runs++;
      }
      // L2: what is not markup IS the `text` cut of the same span.
      eq(
        reading,
        rawMask(source.slice(span[0], span[1]), "text").text,
        `${unit.unitId} ${side}: the non-markup runs rebuild the text cut`,
      );
    }
  }
  check(runs > 0, "the diff produced runs");
  check(markupSeen, "a `\\add` the words moved around shows as a markup run");
}

console.log(failures === 0 ? "conformance: OK" : `conformance: ${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
