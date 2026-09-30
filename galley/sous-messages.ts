/**
 * Why a finding fired, as data: one message id and its parameters, for the
 * consumer to render through its own catalog (`sous-messages.en.json` is the
 * English reference) and its own `Intl`.
 *
 * ```text
 * describe(finding, snapshot.pattern(finding.convention.pattern), {
 *   siteText: "Moses, Moses", bookCount: 66,
 * })
 *   → { id: "convention.doubled.separated",
 *       params: { word: "Moses", text: "Moses, Moses", count: 1, total: 895,
 *                 books: 1, bookTotal: 66 } }
 * en: “Moses” is written twice in a row here (“Moses, Moses”). The project does
 *     this nowhere else; “Moses” appears 895 times.
 * ```
 *
 * A finding names its headline pattern only, so one squiggle gets one message.
 * Word channels read the word from `siteText`; without it their word params
 * are empty strings. Guide: `sous-messages.md`.
 */

// Types only, so this module loads beside the reader without importing it.
import type {
  CasingForm,
  Cluster,
  Finding,
  HygieneClass,
  OuterClass,
  Pattern,
  Pool,
  PresenceKind,
} from "./sous-reader.ts";

/** `PATTERN_DIGIT_GLYPH`, `LETTER_RUN_MAX` and `RUN_BUCKETS` in the reader. */
const DIGIT_GLYPH = 0xffffffff;
const LETTER_RUN_MAX = 8;
const RUN_BUCKETS = 6;

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

type Spread = { count: number; total: number; books: number; bookTotal: number };

/** A mark, a cluster or a pair of marks. The catalog shows every one inside a
 * `<g>` tag and never in quotation marks, since the mark may be one. */
export type Glyph = string;

/** Every parameter each id carries, one id per line; a catalog string may use
 * only these (`tests/sous_messages.rs` reads this block). */
export interface ParamsById {
  "hygiene": { class: HygieneName; run: number; atLeast: boolean };
  "presence.missing": { keys: number; atLeast: boolean };
  "presence.extra": { keys: number; atLeast: boolean };
  "presence.empty": { keys: number; atLeast: boolean };
  "sourceCopy": { run: number; eligible: number };
  "length.long": { deviation: number; inBook: boolean };
  "length.short": { deviation: number; inBook: boolean };
  "convention.exactNeighbor": { glyph: Glyph; neighbor: Glyph; pair: Glyph; reversedPair: Glyph; usual: Glyph; usualCount: number; reversed: number } & Spread;
  "convention.exactNeighbor.swapped": { glyph: Glyph; neighbor: Glyph; pair: Glyph; reversedPair: Glyph; usual: Glyph; usualCount: number; reversed: number } & Spread;
  "convention.pooledNeighbor": { glyph: Glyph; pool: PoolName } & Spread;
  "convention.runShape": { glyph: Glyph; cluster: Glyph; size: number; atLeast: boolean; sameMark: boolean; clusterCount: number; hasUsualCluster: boolean; usualCluster: Glyph; usualClusterCount: number; usualSize: number; usualAtLeast: boolean; usualSameMark: boolean; usualShapeCount: number; usually: boolean } & Spread;
  "convention.placement.follows": { glyph: Glyph; digit: boolean; neighbor: TouchClass; usual: TouchClass; usualCount: number } & Spread;
  "convention.placement.precedes": { glyph: Glyph; digit: boolean; neighbor: TouchClass; usual: TouchClass; usualCount: number } & Spread;
  "convention.rarity": { glyph: Glyph; count: number; books: number; bookTotal: number; hasUsual: boolean; usual: Glyph; usualCount: number };
  "convention.casing": { word: string; form: FormName; usualForm: FormName; usualWord: string; usualCount: number } & Spread;
  "convention.wordLength": { word: string; count: number };
  "convention.doubled.bare": { word: string; text: string } & Spread;
  "convention.doubled.separated": { word: string; text: string } & Spread;
  "convention.letterRun": { letter: Glyph; length: number; atLeast: boolean; run: Glyph; word: string; count: number; total: number };
  "convention.sentenceStart": { glyph: Glyph; word: string; upper: number } & Spread;
  "convention.bookRate.follows": { glyph: Glyph; digit: boolean; neighbor: TouchClass; count: number; total: number; rate: number; book: string; baseline: number; otherBooks: number };
  "convention.bookRate.precedes": { glyph: Glyph; digit: boolean; neighbor: TouchClass; count: number; total: number; rate: number; book: string; baseline: number; otherBooks: number };
}

// `MessageId` and `ParamsById` name the same ids.
const sameIds: [MessageId] extends [keyof ParamsById] ? ([keyof ParamsById] extends [MessageId] ? true : never) : never = true;
void sameIds;

/** One id with exactly its parameters; every one is a plain
 * `{ id: MessageId; params: MessageParams }`. */
export type Message = { [K in MessageId]: { readonly id: K; readonly params: ParamsById[K] } }[MessageId];

export interface MessageContext {
  /** The finding's own text, sliced by the consumer from its span. */
  readonly siteText?: string;
  /** Books in the publication (`FindingsSnapshot.length`). */
  readonly bookCount: number;
  /** A book's display name by its position in the publication. */
  readonly bookName?: (index: number) => string;
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

export type PoolName = "quote" | "bracket" | "dash" | "terminal" | "separator" | "digit" | "symbol" | "other";

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
  Terminal: "terminal",
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
 * group. A directionless quote makes no order a swap, so a reordering of one
 * is never picked. */
function usualCluster(clusters: readonly Cluster[] | undefined, site: string): Cluster | undefined {
  const recurring = clusters?.filter((cluster) => cluster.recurring && cluster.text !== site) ?? [];
  const marks = (text: string) => [...text].sort().join("");
  const reordered = (cluster: Cluster) => marks(cluster.text) === marks(site);
  return (
    recurring.find((cluster) => reordered(cluster) && !cluster.directionless) ??
    recurring.find((cluster) => !reordered(cluster))
  );
}

export function describe(finding: Finding, pattern: Pattern | undefined, context: MessageContext): Message {
  const site = context.siteText ?? "";
  switch (finding.kind) {
    case "Hygiene":
      return {
        id: "hygiene",
        params: { class: HYGIENE[finding.hygiene.class], run: finding.hygiene.run, atLeast: finding.hygiene.saturated },
      };
    case "Presence":
      return {
        id: PRESENCE[finding.presence.kind],
        params: { keys: finding.presence.keys, atLeast: finding.presence.saturated },
      };
    case "SourceCopy":
      return {
        id: "sourceCopy",
        params: { run: finding.sourceCopy.run, eligible: finding.sourceCopy.eligible },
      };
    case "LengthProportionality": {
      // The scope that stands out further is the one the row fired on.
      const { bookScope: book, projectScope: project } = finding.digest;
      const inBook = book !== null && (project === null || Math.abs(book) > Math.abs(project));
      const deviation = (inBook ? book : project) ?? 0;
      return {
        id: deviation < 0 ? "length.short" : "length.long",
        params: { deviation: Math.abs(deviation), inBook },
      };
    }
    case "Convention":
      if (pattern === undefined) {
        throw new Error("a Convention finding needs its pattern to be described");
      }
      return convention(pattern, site, context);
  }
}

function convention(pattern: Pattern, site: string, context: MessageContext): Message {
  // The pooled digit lane is every digit at once; its site is one of them.
  const digit = pattern.glyph === DIGIT_GLYPH;
  const glyph = digit && site !== "" ? site : glyphText(pattern.glyph);
  const spread = { count: pattern.numerator, total: pattern.denominator, books: pattern.books, bookTotal: context.bookCount };
  const { key, usual } = pattern;
  switch (key.kind) {
    case "Placement":
      return {
        id: key.side === "prev" ? "convention.placement.follows" : "convention.placement.precedes",
        params: {
          glyph,
          digit,
          neighbor: touch(key.class),
          ...spread,
          usual: usual.kind === "Placement" ? touch(usual.class) : touch(key.class),
          usualCount: usual.kind === "Placement" ? usual.count : 0,
        },
      };
    case "BookRate": {
      const baselineBp = usual.kind === "BookRate" ? usual.baselineBp : 0;
      return {
        id: key.side === "prev" ? "convention.bookRate.follows" : "convention.bookRate.precedes",
        params: {
          glyph,
          digit,
          neighbor: touch(key.class),
          count: pattern.numerator,
          total: pattern.denominator,
          rate: pattern.shareBp / 10000,
          book: context.bookName?.(key.book) ?? String(key.book + 1),
          baseline: baselineBp / 10000,
          otherBooks: usual.kind === "BookRate" ? usual.otherBooks : 0,
        },
      };
    }
    case "ExactNeighbor": {
      const neighbor = String.fromCodePoint(key.neighbor);
      const reversed = usual.kind === "ExactNeighbor" ? usual.reversed : 0;
      // `"...` opening a quotation, reversed, is `."` closing one: a
      // directionless quote cannot be swapped.
      const swapped =
        !key.directionless && key.neighbor !== pattern.glyph && reversed >= pattern.numerator && reversed >= 5;
      return {
        id: swapped ? "convention.exactNeighbor.swapped" : "convention.exactNeighbor",
        params: {
          glyph,
          neighbor,
          pair: glyph + neighbor,
          reversedPair: neighbor + glyph,
          ...spread,
          usual: usual.kind === "ExactNeighbor" ? String.fromCodePoint(usual.neighbor) : "",
          usualCount: usual.kind === "ExactNeighbor" ? usual.count : 0,
          reversed,
        },
      };
    }
    case "PooledNeighbor":
      return { id: "convention.pooledNeighbor", params: { glyph, pool: POOL[key.pool], ...spread } };
    case "RunShape": {
      const exact = pattern.clusters?.find((cluster) => cluster.text === site);
      const common = usualCluster(pattern.clusters, site);
      const shape = usual.kind === "RunShape" ? usual : undefined;
      const usualShapeCount = shape?.count ?? 0;
      return {
        id: "convention.runShape",
        params: {
          glyph,
          cluster: site,
          // The last length bucket holds every longer run.
          size: site === "" ? key.bucket : [...site].length,
          atLeast: site === "" && key.bucket === RUN_BUCKETS,
          sameMark: key.pure,
          clusterCount: exact?.count ?? 0,
          ...spread,
          hasUsualCluster: common !== undefined,
          usualCluster: common?.text ?? "",
          usualClusterCount: common?.count ?? 0,
          usualSize: shape?.bucket ?? 1,
          usualAtLeast: shape?.bucket === RUN_BUCKETS,
          usualSameMark: shape?.pure ?? true,
          usualShapeCount,
          usually: usualShapeCount * 3 >= pattern.denominator * 2,
        },
      };
    }
    case "Rarity": {
      const other = usual.kind === "Rarity" ? usual.glyph : null;
      return {
        id: "convention.rarity",
        params: {
          glyph,
          count: pattern.numerator,
          books: pattern.books,
          bookTotal: context.bookCount,
          hasUsual: other !== null,
          usual: other === null ? "" : glyphText(other),
          usualCount: usual.kind === "Rarity" ? usual.count : 0,
        },
      };
    }
    case "Casing": {
      const usualForm = usual.kind === "Casing" ? usual.form : "Lower";
      return {
        id: "convention.casing",
        params: {
          word: site,
          form: form(key.form),
          usualForm: form(usualForm),
          usualWord: site === "" ? "" : spelled(site, usualForm),
          usualCount: usual.kind === "Casing" ? usual.count : 0,
          ...spread,
        },
      };
    }
    case "WordLength":
      return { id: "convention.wordLength", params: { word: site, count: pattern.numerator } };
    case "Doubled":
      return {
        id: key.separated ? "convention.doubled.separated" : "convention.doubled.bare",
        params: { word: firstWord(site), text: site, ...spread },
      };
    case "LetterRun": {
      const letter = glyphText(pattern.glyph);
      return {
        id: "convention.letterRun",
        params: {
          letter,
          length: key.length,
          atLeast: key.length === LETTER_RUN_MAX,
          run: letter.repeat(key.length),
          word: site,
          count: pattern.numerator,
          total: pattern.denominator,
        },
      };
    }
    case "SentenceStart":
      return {
        id: "convention.sentenceStart",
        params: {
          glyph,
          word: site,
          count: pattern.numerator,
          upper: pattern.denominator - pattern.numerator,
          total: pattern.denominator,
          books: pattern.books,
          bookTotal: context.bookCount,
        },
      };
  }
}
