/**
 * Why a finding fired, as data: one message id, its parameters, and literal
 * find queries, for the consumer to render through its own catalog
 * (`sous-messages.en.json` is the English reference, a headline and details
 * per id) and its own `Intl`.
 *
 * ```text
 * describe(finding, snapshot.pattern(finding.convention.pattern), {
 *   siteText: "Moses, Moses", bookCount: 66, bookName,
 * })
 *   → { id: "convention.doubled.separated",
 *       params: { word: "Moses", text: "Moses, Moses", count: 1, total: 895,
 *                 books: 1, bookTotal: 66, namedBooks: 1, book1: "GEN", … },
 *       queries: [{ purpose: "this", needle: "Moses, Moses",
 *                   caseSensitive: false, wholeWord: true }] }
 * en headline  “Moses” is written twice with only punctuation between here (“Moses, Moses”).
 *    details   The project does this nowhere else; “Moses” appears 895 times.
 * ```
 *
 * A finding names its headline pattern only, so one squiggle gets one message.
 * Word channels read the word from `siteText`; without it their word params
 * are empty strings. Guide: `sous-messages.md`.
 */

// The reader's types only, so this module loads beside it without importing it.
import type {
  CasingForm,
  Cluster,
  ConventionFinding,
  Finding,
  FindingsSnapshot,
  HygieneClass,
  OuterClass,
  Pattern,
  Pool,
  PresenceKind,
  TerminalContext,
} from "./sous-reader.ts";
import { kindOf, type MarkKind } from "./sous-unicode.ts";

/** `PATTERN_DIGIT_GLYPH`, `LETTER_RUN_MAX` and `RUN_BUCKETS` in the reader. */
const DIGIT_GLYPH = 0xffffffff;
const LETTER_RUN_MAX = 8;
const RUN_BUCKETS = 6;
/** Books a spread names instead of counting. */
const NAMED_BOOKS = 2;

export type MessageId =
  | "hygiene"
  | "presence.missing"
  | "presence.extra"
  | "presence.empty"
  | "sourceCopy"
  | "length.long"
  | "length.short"
  | "convention.exactNeighbor"
  | "convention.exactNeighbor.swapped"
  | "convention.pooledNeighbor"
  | "convention.runShape"
  | "convention.placement.follows"
  | "convention.placement.precedes"
  | "convention.rarity"
  | "convention.casing"
  | "convention.wordLength"
  | "convention.doubled.bare"
  | "convention.doubled.separated"
  | "convention.letterRun"
  | "convention.sentenceStart"
  | "convention.bookRate.follows"
  | "convention.bookRate.precedes";

export type MessageParams = Record<string, string | number | boolean>;

/** How often, against what, and where: `namedBooks` of the `books` holding
 * the count are named in `book1` and `book2` when the spread is small enough
 * to name, else 0. */
type Spread = { count: number; total: number; books: number; bookTotal: number; namedBooks: number; book1: string; book2: string };

/** One mark, beside a `…Kind` parameter naming what it is. The catalog shows
 * every mark inside a `<g>` tag and never in quotation marks, since the mark
 * may be one. */
export type Glyph = string;

/** A pair or a group of marks, shown inside `<g>` like a `Glyph`; the kinds of
 * its marks are its `Glyph` parameters' kinds. */
export type Glyphs = string;

export type { MarkKind };

/** Every parameter each id carries, one id per line; both tiers of a catalog
 * entry may use only these (`tests/sous_messages.rs` reads this block). */
export interface ParamsById {
  "hygiene": { class: HygieneName; run: number; atLeast: boolean };
  "presence.missing": { keys: number; atLeast: boolean };
  "presence.extra": { keys: number; atLeast: boolean };
  "presence.empty": { keys: number; atLeast: boolean };
  "sourceCopy": { run: number; eligible: number };
  "length.long": { deviation: number; inBook: boolean };
  "length.short": { deviation: number; inBook: boolean };
  "convention.exactNeighbor": { glyph: Glyph; glyphKind: MarkKind; neighbor: Glyph; neighborKind: MarkKind; pair: Glyphs; reversedPair: Glyphs; usual: Glyph; usualKind: MarkKind; usualCount: number; reversed: number } & Spread;
  "convention.exactNeighbor.swapped": { glyph: Glyph; glyphKind: MarkKind; neighbor: Glyph; neighborKind: MarkKind; pair: Glyphs; reversedPair: Glyphs; usual: Glyph; usualKind: MarkKind; usualCount: number; reversed: number } & Spread;
  "convention.pooledNeighbor": { glyph: Glyph; glyphKind: MarkKind; pool: PoolName } & Spread;
  "convention.runShape": { glyph: Glyph; glyphKind: MarkKind; cluster: Glyphs; clusterCount: number; hasUsualCluster: boolean; reordered: boolean; swap: boolean; usualCluster: Glyphs; usualClusterCount: number; usualSize: number; usualAtLeast: boolean; usualSameMark: boolean; usualShapeCount: number } & Spread;
  "convention.placement.follows": { glyph: Glyph; glyphKind: MarkKind; digit: boolean; neighbor: TouchClass; usual: TouchClass; usualCount: number } & Spread;
  "convention.placement.precedes": { glyph: Glyph; glyphKind: MarkKind; digit: boolean; neighbor: TouchClass; usual: TouchClass; usualCount: number } & Spread;
  "convention.rarity": { glyph: Glyph; glyphKind: MarkKind; hasUsual: boolean; usual: Glyph; usualKind: MarkKind; usualCount: number; lookalike: boolean } & Spread;
  "convention.casing": { word: string; form: FormName; usualForm: FormName; usualWord: string; usualCount: number; hasBefore: boolean; before: Glyph; beforeKind: MarkKind; beforeContext: BeforeContext; beforeLower: number; beforeCased: number } & Spread;
  "convention.wordLength": { word: string; count: number };
  "convention.doubled.bare": { word: string; text: string } & Spread;
  "convention.doubled.separated": { word: string; text: string } & Spread;
  "convention.letterRun": { letter: Glyph; letterKind: MarkKind; length: number; atLeast: boolean; run: Glyphs; word: string; count: number; total: number };
  "convention.sentenceStart": { glyph: Glyph; glyphKind: MarkKind; word: string; usualWord: string; upper: number } & Spread;
  "convention.bookRate.follows": { glyph: Glyph; glyphKind: MarkKind; digit: boolean; neighbor: TouchClass; count: number; total: number; rate: number; book: string; baseline: number; otherBooks: number };
  "convention.bookRate.precedes": { glyph: Glyph; glyphKind: MarkKind; digit: boolean; neighbor: TouchClass; count: number; total: number; rate: number; book: string; baseline: number; otherBooks: number };
}

// `MessageId` and `ParamsById` name the same ids.
const sameIds: [MessageId] extends [keyof ParamsById] ? ([keyof ParamsById] extends [MessageId] ? true : never) : never = true;
void sameIds;

/**
 * One literal search a consumer may run on demand, through galley's
 * `findAll(needle, { caseSensitive, wholeWord, scope })`, to show every
 * occurrence behind a message.
 *
 * - `this`: the finding's own form, wherever else the project writes it.
 * - `alternative`: what the reader might write instead: the same marks in
 *   another order, a lookalike, or the same word in another case.
 * - `others`: the usual comparison the details name (the mark that usually
 *   follows, the most common group, the pool's most common mark).
 */
export interface Query {
  readonly purpose: "this" | "alternative" | "others";
  readonly needle: string;
  readonly caseSensitive: boolean;
  readonly wholeWord: boolean;
}

/** One id with exactly its parameters, and its queries; every one is a plain
 * `{ id: MessageId; params: MessageParams; queries: Query[] }`. */
export type Message = {
  [K in MessageId]: { readonly id: K; readonly params: ParamsById[K]; readonly queries: readonly Query[] };
}[MessageId];

/** A catalog entry: the headline says one fact and at most one alternative;
 * the details hold the supporting numbers. */
export interface CatalogEntry {
  readonly headline: string;
  readonly details: string;
}

export interface MessageContext {
  /** The finding's own text, sliced by the consumer from its span. */
  readonly siteText?: string;
  /** Books in the publication (`FindingsSnapshot.length`). */
  readonly bookCount: number;
  /** A book's display name by its position in the publication. */
  readonly bookName?: (index: number) => string;
  /** The books whose findings name a pattern, by pattern index:
   * `booksByPattern(snapshot)`. Without it only a one-book spread is named. */
  readonly patternBooks?: (pattern: number) => readonly number[];
  /** The mark before the site and what stands between them, from the
   * consumer's own verse text: `markBefore(text, finding.from)`. */
  readonly before?: MarkBefore;
  /** Where `before` is looked up: the snapshot the finding came from. */
  readonly snapshot?: Pick<FindingsSnapshot, "terminal">;
}

/** The mark a word is handed off from, as the engine reads it. */
export interface MarkBefore extends TerminalContext {
  readonly glyph: number;
}

/** What stands between the mark and the word, as a `select` key. */
export type BeforeContext = "bare" | "quoted" | "bracketed" | "both";

/**
 * The mark before `at` in verse text, by the engine's ride rule
 * (`substrate::ride_of`): walking back, white space is skipped, a quotation
 * mark rides and sets `quoted`, a closing bracket rides and sets `bracketed`,
 * an opening bracket rides and marks nothing, and the first other mark is the
 * glyph. A letter, a digit or the start of the text first means no mark.
 *
 * ```text
 * markBefore('them; He', 6)        → { glyph: ';', quoted: false, bracketed: false }
 * markBefore('said, "Name', 7)     → { glyph: ',', quoted: true,  bracketed: false }
 * markBefore('forever.) to', 10)   → { glyph: '.', quoted: false, bracketed: true }
 * markBefore('said "Go', 6)        → undefined
 * ```
 *
 * `text` is the verse text the engine read, with markers removed; `at` is a
 * UTF-16 offset into it. Quotation marks are Unicode `Quotation_Mark`, and a
 * bracket is general category `Ps`, `Pe`, `Pi` or `Pf` outside them.
 */
export function markBefore(text: string, at: number): MarkBefore | undefined {
  let quoted = false;
  let bracketed = false;
  let index = at;
  while (index > 0) {
    const low = text.charCodeAt(index - 1);
    const width = low >= 0xdc00 && low <= 0xdfff && index > 1 ? 2 : 1;
    const glyph = text.codePointAt(index - width) ?? low;
    const char = String.fromCodePoint(glyph);
    index -= width;
    if (/\s/u.test(char)) continue;
    if (/\p{Quotation_Mark}/u.test(char)) {
      quoted = true;
      continue;
    }
    if (/\p{Pe}/u.test(char)) {
      bracketed = true;
      continue;
    }
    if (/[\p{Ps}\p{Pi}\p{Pf}]/u.test(char)) continue;
    if (/[\p{Alphabetic}\p{M}\p{Nd}]/u.test(char)) return undefined;
    return { glyph, quoted, bracketed };
  }
  return undefined;
}

/**
 * The books whose findings name each pattern, from one pass over the
 * snapshot, for `MessageContext.patternBooks`.
 *
 * ```text
 * booksByPattern(snapshot)(7)   → [22, 23]     // ISA and JER hold its sites
 * ```
 */
export function booksByPattern(snapshot: Pick<FindingsSnapshot, "length" | "book">): (pattern: number) => readonly number[] {
  const books = new Map<number, number[]>();
  for (let index = 0; index < snapshot.length; index += 1) {
    const book = snapshot.book(index);
    if (book === undefined) continue;
    for (let row = 0; row < book.count; row += 1) {
      const finding = book.at(row);
      if (finding.kind !== "Convention") continue;
      const held = books.get(finding.convention.pattern) ?? [];
      if (held.at(-1) !== index) held.push(index);
      books.set(finding.convention.pattern, held);
    }
  }
  return (pattern) => books.get(pattern) ?? [];
}

function beforeContext(before: TerminalContext): BeforeContext {
  if (before.quoted) return before.bracketed ? "both" : "quoted";
  return before.bracketed ? "bracketed" : "bare";
}

/** What touches a glyph, as a `select` key. `Edge` never fires. */
export type TouchClass = "letter" | "space" | "digit" | "punctuation";

/** A case form, as a `select` key. `Uncased` is never on the wire. */
export type FormName = "lowercase" | "capitalized" | "allCaps" | "mixed";

export type HygieneName =
  | "control"
  | "delete"
  | "replacement"
  | "carriageReturn"
  | "backslash"
  | "conflict"
  | "combiningMark"
  | "format"
  | "noBreakSpace"
  | "noncharacter";

/** A pool, named as a `MarkKind`: a run neighbour is never a letter or a space. */
export type PoolName = Exclude<MarkKind, "letter" | "space">;

const TOUCH: Record<Exclude<OuterClass, "Edge">, TouchClass> = {
  Letter: "letter",
  Space: "space",
  Digit: "digit",
  Nonletter: "punctuation",
};

const FORM: Record<Exclude<CasingForm, "Uncased">, FormName> = {
  Lower: "lowercase",
  Title: "capitalized",
  Upper: "allCaps",
  Mixed: "mixed",
};

const HYGIENE: Record<HygieneClass, HygieneName> = {
  C0Control: "control",
  Delete: "delete",
  C1Control: "control",
  ReplacementChar: "replacement",
  StrayCarriageReturn: "carriageReturn",
  StrandedBackslash: "backslash",
  ConflictMarker: "conflict",
  FreeCombiningMark: "combiningMark",
  MisplacedFormat: "format",
  NoBreakSpace: "noBreakSpace",
  Noncharacter: "noncharacter",
};

const PRESENCE: Record<PresenceKind, "presence.missing" | "presence.extra" | "presence.empty"> = {
  Missing: "presence.missing",
  Extra: "presence.extra",
  Empty: "presence.empty",
};

const POOL: Record<Pool, PoolName> = {
  Quote: "quote",
  Bracket: "bracket",
  Dash: "dash",
  Terminal: "sentenceEnd",
  Separator: "separator",
  Digit: "digit",
  Symbol: "symbol",
  Other: "other",
};

function touch(outer: OuterClass): TouchClass {
  // The reader refuses `Edge` on every row that names a class it touches.
  return outer === "Edge" ? "punctuation" : TOUCH[outer];
}

function form(casing: CasingForm): FormName {
  return casing === "Uncased" ? "mixed" : FORM[casing];
}

function glyphText(glyph: number): string {
  return glyph === DIGIT_GLYPH ? "0–9" : String.fromCodePoint(glyph);
}

/** What a mark is: its first scalar's kind, `other` for none. */
function kind(text: string): MarkKind {
  const scalar = text.codePointAt(0);
  return scalar === undefined ? "other" : kindOf(scalar);
}

/** The first word of a site: letters and marks, through inner apostrophes. */
function firstWord(text: string): string {
  return /^[\p{L}\p{M}\p{N}]+(?:['’][\p{L}\p{M}\p{N}]+)*/u.exec(text)?.[0] ?? "";
}

function spelled(word: string, casing: CasingForm): string {
  const lower = word.toLocaleLowerCase();
  if (casing === "Lower") return lower;
  if (casing === "Upper") return word.toLocaleUpperCase();
  if (casing === "Title") {
    const [first = "", ...rest] = lower;
    return first.toLocaleUpperCase() + rest.join("");
  }
  return "";
}

/** The recurring cluster to compare a site with: the same marks in another
 * order when the row lists one (`';` beside `;'`), else the most common other
 * group, and whether it is a reordering. */
function usualCluster(clusters: readonly Cluster[] | undefined, site: string): [Cluster | undefined, boolean] {
  const recurring = clusters?.filter((cluster) => cluster.recurring && cluster.text !== site) ?? [];
  const marks = (text: string) => [...text].sort().join("");
  const reordered = recurring.find((cluster) => marks(cluster.text) === marks(site));
  return reordered === undefined ? [recurring[0], false] : [reordered, true];
}

function query(purpose: Query["purpose"], needle: string, caseSensitive = true, wholeWord = false): Query[] {
  return needle === "" ? [] : [{ purpose, needle, caseSensitive, wholeWord }];
}

/** A class is no literal, so a placement row searches only a site that holds
 * its neighbour too (`),`); one that holds the glyph alone finds nothing
 * narrower than the glyph. */
function placed(site: string): Query[] {
  return [...site].length > 1 ? query("this", site) : [];
}

export function describe(finding: Finding, pattern: Pattern | undefined, context: MessageContext): Message {
  const site = context.siteText ?? "";
  switch (finding.kind) {
    case "Hygiene":
      return {
        id: "hygiene",
        params: { class: HYGIENE[finding.hygiene.class], run: finding.hygiene.run, atLeast: finding.hygiene.saturated },
        queries: [],
      };
    case "Presence":
      return {
        id: PRESENCE[finding.presence.kind],
        params: { keys: finding.presence.keys, atLeast: finding.presence.saturated },
        queries: [],
      };
    case "SourceCopy":
      return {
        id: "sourceCopy",
        params: { run: finding.sourceCopy.run, eligible: finding.sourceCopy.eligible },
        queries: [],
      };
    case "LengthProportionality": {
      // The scope that stands out further is the one the row fired on.
      const { bookScope: book, projectScope: project } = finding.digest;
      const inBook = book !== null && (project === null || Math.abs(book) > Math.abs(project));
      const deviation = (inBook ? book : project) ?? 0;
      return {
        id: deviation < 0 ? "length.short" : "length.long",
        params: { deviation: Math.abs(deviation), inBook },
        queries: [],
      };
    }
    case "Convention":
      if (pattern === undefined) {
        throw new Error("a Convention finding needs its pattern to be described");
      }
      return convention(finding, pattern, site, context);
  }
}

/** The spread of a pattern's count, naming its books when there are few:
 * one book is the finding's own, two come from `patternBooks`. */
function spreadOf(finding: ConventionFinding, pattern: Pattern, context: MessageContext): Spread {
  const name = context.bookName;
  let named: readonly number[] = [];
  if (name !== undefined && pattern.books <= NAMED_BOOKS) {
    const listed = context.patternBooks?.(finding.convention.pattern) ?? [];
    if (pattern.books === 1) named = [finding.bookIdx];
    else if (listed.length === pattern.books) named = listed;
  }
  return {
    count: pattern.numerator,
    total: pattern.denominator,
    books: pattern.books,
    bookTotal: context.bookCount,
    namedBooks: named.length,
    book1: named[0] === undefined || name === undefined ? "" : name(named[0]),
    book2: named[1] === undefined || name === undefined ? "" : name(named[1]),
  };
}

function convention(finding: ConventionFinding, pattern: Pattern, site: string, context: MessageContext): Message {
  // The pooled digit lane is every digit at once; its site is one of them.
  const digit = pattern.glyph === DIGIT_GLYPH;
  const glyph = digit && site !== "" ? site : glyphText(pattern.glyph);
  const spread = spreadOf(finding, pattern, context);
  const { key, usual } = pattern;
  switch (key.kind) {
    case "Placement":
      return {
        id: key.side === "prev" ? "convention.placement.follows" : "convention.placement.precedes",
        params: {
          glyph,
          glyphKind: digit ? "digit" : kind(glyph),
          digit,
          neighbor: touch(key.class),
          ...spread,
          usual: usual.kind === "Placement" ? touch(usual.class) : touch(key.class),
          usualCount: usual.kind === "Placement" ? usual.count : 0,
        },
        queries: placed(site),
      };
    case "BookRate": {
      const baselineBp = usual.kind === "BookRate" ? usual.baselineBp : 0;
      return {
        id: key.side === "prev" ? "convention.bookRate.follows" : "convention.bookRate.precedes",
        params: {
          glyph,
          glyphKind: digit ? "digit" : kind(glyph),
          digit,
          neighbor: touch(key.class),
          count: pattern.numerator,
          total: pattern.denominator,
          rate: pattern.shareBp / 10000,
          book: context.bookName?.(key.book) ?? String(key.book + 1),
          baseline: baselineBp / 10000,
          otherBooks: usual.kind === "BookRate" ? usual.otherBooks : 0,
        },
        queries: placed(site),
      };
    }
    case "ExactNeighbor": {
      const neighbor = String.fromCodePoint(key.neighbor);
      // A pair of one mark twice (`..`) is its own reversal.
      const reversed = usual.kind === "ExactNeighbor" && key.neighbor !== pattern.glyph ? usual.reversed : 0;
      const follower = usual.kind === "ExactNeighbor" ? String.fromCodePoint(usual.neighbor) : "";
      // `"...` opening a quotation, reversed, is `."` closing one: a
      // directionless quote cannot be swapped.
      const swapped =
        !key.directionless && reversed >= pattern.numerator && reversed >= 5;
      const pair = glyph + neighbor;
      const reversedPair = neighbor + glyph;
      return {
        id: swapped ? "convention.exactNeighbor.swapped" : "convention.exactNeighbor",
        params: {
          glyph,
          glyphKind: kind(glyph),
          neighbor,
          neighborKind: kind(neighbor),
          pair,
          reversedPair,
          ...spread,
          usual: follower,
          usualKind: kind(follower),
          usualCount: usual.kind === "ExactNeighbor" ? usual.count : 0,
          reversed,
        },
        queries: [
          ...query("this", pair),
          ...(reversed > 0 ? query("alternative", reversedPair) : []),
          ...query("others", follower === "" ? "" : glyph + follower),
        ],
      };
    }
    case "PooledNeighbor":
      return { id: "convention.pooledNeighbor", params: { glyph, glyphKind: kind(glyph), pool: POOL[key.pool], ...spread }, queries: query("this", site) };
    case "RunShape": {
      const exact = pattern.clusters?.find((cluster) => cluster.text === site);
      const [common, reordered] = usualCluster(pattern.clusters, site);
      const shape = usual.kind === "RunShape" ? usual : undefined;
      // A reordering is a headline alternative unless a directionless quote
      // makes it no swap; then it is a plain fact in the details.
      const swap = reordered && common !== undefined && !common.directionless;
      return {
        id: "convention.runShape",
        params: {
          glyph,
          glyphKind: kind(glyph),
          cluster: site,
          clusterCount: exact?.count ?? 0,
          ...spread,
          hasUsualCluster: common !== undefined,
          reordered,
          swap,
          usualCluster: common?.text ?? "",
          usualClusterCount: common?.count ?? 0,
          usualSize: shape?.bucket ?? 1,
          usualAtLeast: shape?.bucket === RUN_BUCKETS,
          usualSameMark: shape?.pure ?? true,
          usualShapeCount: shape?.count ?? 0,
        },
        queries: [
          ...query("this", site),
          ...query(reordered ? "alternative" : "others", common?.text ?? ""),
        ],
      };
    }
    case "Rarity": {
      const other = usual.kind === "Rarity" ? usual.glyph : null;
      const lookalike = usual.kind === "Rarity" && usual.lookalike;
      const usualText = other === null ? "" : glyphText(other);
      return {
        id: "convention.rarity",
        params: {
          glyph,
          glyphKind: kind(glyph),
          ...spread,
          hasUsual: other !== null,
          usual: usualText,
          usualKind: kind(usualText),
          usualCount: usual.kind === "Rarity" ? usual.count : 0,
          lookalike,
        },
        queries: [...query("this", glyph), ...query(lookalike ? "alternative" : "others", usualText)],
      };
    }
    case "Casing": {
      const usualForm = usual.kind === "Casing" ? usual.form : "Lower";
      const usualWord = site === "" ? "" : spelled(site, usualForm);
      const { before } = context;
      const after = before === undefined ? undefined : context.snapshot?.terminal(before.glyph, before);
      return {
        id: "convention.casing",
        params: {
          word: site,
          form: form(key.form),
          usualForm: form(usualForm),
          usualWord,
          usualCount: usual.kind === "Casing" ? usual.count : 0,
          hasBefore: after !== undefined,
          before: before === undefined ? "" : String.fromCodePoint(before.glyph),
          beforeKind: before === undefined ? "other" : kindOf(before.glyph),
          beforeContext: before === undefined ? "bare" : beforeContext(before),
          beforeLower: after === undefined ? 0 : after.cased - after.upper,
          beforeCased: after?.cased ?? 0,
          ...spread,
        },
        queries: [...query("this", site, true, true), ...query("alternative", usualWord, true, true)],
      };
    }
    case "WordLength":
      return {
        id: "convention.wordLength",
        params: { word: site, count: pattern.numerator },
        queries: query("this", site, false, true),
      };
    case "Doubled":
      return {
        id: key.separated ? "convention.doubled.separated" : "convention.doubled.bare",
        params: { word: firstWord(site), text: site, ...spread },
        queries: query("this", site, false, true),
      };
    case "LetterRun": {
      const letter = glyphText(pattern.glyph);
      const run = letter.repeat(key.length);
      return {
        id: "convention.letterRun",
        params: {
          letter,
          letterKind: kind(letter),
          length: key.length,
          atLeast: key.length === LETTER_RUN_MAX,
          run,
          word: site,
          count: pattern.numerator,
          total: pattern.denominator,
        },
        queries: query("this", run, false),
      };
    }
    case "SentenceStart": {
      const usualWord = site === "" ? "" : spelled(site, "Title");
      return {
        id: "convention.sentenceStart",
        params: {
          glyph,
          glyphKind: kind(glyph),
          word: site,
          usualWord,
          ...spread,
          upper: pattern.denominator - pattern.numerator,
        },
        queries: [...query("this", site, true, true), ...query("alternative", usualWord, true, true)],
      };
    }
  }
}
