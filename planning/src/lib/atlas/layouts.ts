/**
 * The wire layouts, read and decoded at build time.
 *
 *   docs/layouts/dish.json + samples/dish.bin
 *     → { layout, sample: { usfm, bytes, spans, owner, regions } }
 *
 * Every byte of a sample is owned by at most one span (a field of one row of
 * one region), decoded with the format's GENERATED reader when it imports:
 * `TokenRow` from `onion-wasm/reader.ts` reads a Token row here exactly as it
 * does for a consumer. The page ships the spans as JSON and only hovers.
 */
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { layoutsDir } from "./paths.ts";
import { layoutSchema, type Layout, type LayoutField } from "./schema.ts";

type Module = Record<string, unknown>;
const readers = import.meta.glob<Module>(["../../../../onion-wasm/reader.ts", "../../../../galley/*-reader.ts"]);
const readerFor = async (path: string | null | undefined): Promise<Module | null> => {
  const load = path ? readers[`../../../../${path}`] : undefined;
  if (!load) return null;
  try {
    return await load();
  } catch {
    return null;
  }
};

export interface Span {
  start: number;
  end: number;
  /** The envelope region the bytes sit in. */
  region: string;
  /** The record type, for a region of rows. */
  record?: string;
  row: number;
  /** `directory[3]` is `diagnostics`: the row's name, when the reader names it. */
  rowName?: string;
  field: string;
  value: string;
  space: string;
  /** 1..8 → `--crate-N`; 0 neutral. */
  hue: number;
  /** The USFM byte range an offset field points into. */
  src?: [number, number];
  text?: string;
}

/** One row (or the one header) at a known place, with the fields it holds. */
export interface Entry {
  at: number;
  stride: number;
  record?: string;
  fields: LayoutField[];
  row: number;
  rowName?: string;
}

export interface Located {
  /** `chapters`, or `chapters[1]` for a region repeated per book. */
  name: string;
  kind: "fixed" | "table" | "rows" | "words" | "blob";
  at: number;
  end: number;
  entries: Entry[];
  record?: string;
  /** A blob that is another format's whole buffer. */
  holds?: string;
  doc?: string;
}

export interface Sample {
  usfm: string;
  bytes: number[];
  spans: Span[];
  /** Per byte: the index of the span that owns it, or -1. */
  owner: number[];
  regions: Located[];
  /** The USFM cut at every offset any span points to: `[fromByte, toByte, text]`. */
  segments: [number, number, string][];
  /** How each record was decoded. */
  decodedBy: Record<string, "reader" | "layout">;
  /** Set when only the fixed header could be placed. */
  partial: boolean;
  utf16: boolean;
}

export interface Wire {
  file: string;
  layout: Layout;
  crate: string;
  sample: Sample | null;
  readerLoaded: boolean;
  error?: string;
}

/** `onion/src/wire/schema.rs` → onion; `sous-chef/core/…` → sous-core. */
export const crateOf = (path: string | undefined): string => {
  const parts = (path ?? "").split("/").filter(Boolean);
  if (parts[0] !== "sous-chef") return parts[0] ?? "";
  const sub = parts[1] ?? "";
  return sub.startsWith("sous-") ? sub : `sous-${sub}`;
};

const camel = (name: string): string => name.replace(/_([a-z0-9])/gu, (_, c: string) => c.toUpperCase());
const hueOf = (index: number): number => (index % 8) + 1;
const NONE32 = 0xffffffff;

const readUint = (view: DataView, at: number, width: number): number | bigint => {
  if (at + width > view.byteLength) return 0;
  switch (width) {
    case 1: return view.getUint8(at);
    case 2: return view.getUint16(at, true);
    case 4: return view.getUint32(at, true);
    case 8: return view.getBigUint64(at, true);
    default: {
      let v = 0;
      for (let i = width - 1; i >= 0; i--) v = v * 256 + view.getUint8(at + i);
      return v;
    }
  }
};

const show = (field: LayoutField, v: number | bigint): string => {
  if (typeof v === "bigint") return `0x${v.toString(16).padStart(16, "0")}`;
  if (field.width === 4 && v === NONE32) return "NONE";
  if (field.name === "magic" && field.width === 4) {
    const s = String.fromCharCode(v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >>> 24) & 0xff);
    return /^[\x20-\x7e]{4}$/u.test(s) ? `"${s}"` : String(v);
  }
  return String(v);
};

// --- locating regions ------------------------------------------------------

type Env = Map<string, Record<string, number>[]>;

/**
 * `"32"`, `"header.sectionCount"`, `"directory[0].length / 12"`,
 * `"after the last hit"`, `"sum of directory[book].idLen"`,
 * `"sum of idByteLen, then sum of previewByteLen"`. Prose it cannot read is
 * undefined, and the region stays unmapped.
 */
const evaluate = (expr: number | string | undefined | null, env: Env, cursor: number, book: number): number | undefined => {
  if (expr === undefined || expr === null) return undefined;
  if (typeof expr === "number") return expr;
  const text = expr.trim();
  if (/^after\b/u.test(text)) return cursor;
  if (/^sum of /u.test(text)) {
    let total = 0;
    for (const part of text.split(/,?\s*then\s+/u)) {
      const target = /^sum of (.+)$/u.exec(part.trim())?.[1]?.trim();
      if (!target) return undefined;
      const each = /^(\w+)\[book\]\.(\w+)$/u.exec(target);
      if (each) {
        for (const row of env.get(each[1] ?? "") ?? []) total += row[each[2] ?? ""] ?? 0;
      } else if (/^\w+$/u.test(target)) {
        let seen = false;
        for (const rows of env.values()) for (const row of rows) {
          const v = row[target];
          if (v !== undefined) { total += v; seen = true; }
        }
        if (!seen) return undefined;
      } else return undefined;
    }
    return total;
  }
  const tokens = text.replace(/\[book\]/gu, `[${book}]`).match(/\d+|[A-Za-z_]\w*(?:\[\d+\])?(?:\.\w+)?|[-+*/]|\S/gu) ?? [];
  const term = (t: string | undefined): number | undefined => {
    if (t === undefined) return undefined;
    if (/^\d+$/u.test(t)) return Number(t);
    const m = /^(\w+)(?:\[(\d+)\])?\.(\w+)$/u.exec(t);
    if (!m) return undefined;
    return env.get(m[1] ?? "")?.[Number(m[2] ?? 0)]?.[m[3] ?? ""];
  };
  let acc = term(tokens[0]);
  if (tokens.length % 2 === 0) return undefined;
  for (let i = 1; i + 1 < tokens.length && acc !== undefined; i += 2) {
    const rhs = term(tokens[i + 1]);
    if (rhs === undefined) return undefined;
    const op = tokens[i];
    if (op === "+") acc += rhs;
    else if (op === "-") acc -= rhs;
    else if (op === "*") acc *= rhs;
    else if (op === "/") acc = rhs === 0 ? 0 : Math.floor(acc / rhs);
    else return undefined;
  }
  return acc;
};

const decodeFields = (view: DataView, at: number, fields: LayoutField[]): Record<string, number> =>
  Object.fromEntries(fields.map((f) => {
    const v = readUint(view, at + f.offset, f.width);
    return [f.name, typeof v === "bigint" ? Number(v) : v];
  }));

type Record_ = Layout["records"][number];
const tailOf = (record: Record_): { count: string; of: string } | null => {
  const t = record.tail as { count?: unknown; of?: unknown } | null | undefined;
  return t && typeof t.count === "string" && typeof t.of === "string" ? { count: t.count, of: t.of } : null;
};

/** `count` rows of `record` from `at`, walking any tail; returns the entries and the end. */
const walkRows = (layout: Layout, view: DataView, record: Record_, at: number, count: number): { entries: Entry[]; end: number } => {
  const entries: Entry[] = [];
  const tail = tailOf(record);
  const child = tail ? layout.records.find((r) => r.name === tail.of) : undefined;
  let p = at;
  for (let row = 0; row < count && p + record.stride <= view.byteLength; row++) {
    entries.push({ at: p, stride: record.stride, record: record.name, fields: record.fields, row });
    p += record.stride;
    if (tail && child) {
      const n = decodeFields(view, p - record.stride, record.fields)[tail.count] ?? 0;
      for (let k = 0; k < n && p + child.stride <= view.byteLength; k++) {
        entries.push({ at: p, stride: child.stride, record: child.name, fields: child.fields, row: k, rowName: `${record.name} ${row} · ${child.name} ${k}` });
        p += child.stride;
      }
    }
  }
  return { entries, end: p };
};

/** Generic: the layout's own `at` / `kind` / `count` expressions. */
const locateFromLayout = (layout: Layout, view: DataView): Located[] => {
  const env: Env = new Map();
  const out: Located[] = [];
  let cursor = 0;
  for (const region of layout.envelope) {
    const exprs = [region.at, region.count, region.bytes].filter((e): e is string => typeof e === "string");
    const per = exprs.map((e) => /(\w+)\[book\]/u.exec(e)?.[1]).find((t) => t !== undefined && !/^sum of/u.test(exprs.join(" ")));
    const books = per ? (env.get(per)?.length ?? 0) : 1;
    for (let book = 0; book < books; book++) {
      const name = per && books > 1 ? `${region.region}[${book}]` : region.region;
      const at = evaluate(region.at, env, cursor, book) ?? (out.length === 0 ? 0 : undefined);
      const kind = region.kind ?? (out.length === 0 && region.fields.length > 0 ? "fixed" : undefined);
      if (at === undefined || kind === undefined || at > view.byteLength) continue;
      let located: Located | null = null;
      if (kind === "fixed") {
        const bytes = evaluate(region.bytes, env, cursor, book) ?? Math.max(0, ...region.fields.map((f) => f.offset + f.width));
        env.set(region.region, [decodeFields(view, at, region.fields)]);
        located = { name, kind, at, end: at + bytes, entries: [{ at, stride: bytes, fields: region.fields, row: 0 }], doc: region.doc };
      } else if (kind === "table") {
        const stride = region.entry_bytes ?? (typeof region.bytes === "number" ? region.bytes : 0);
        const count = evaluate(region.count, env, cursor, book) ?? 0;
        const entries: Entry[] = Array.from({ length: count }, (_, row) => ({ at: at + row * stride, stride, fields: region.fields, row }));
        env.set(name, entries.map((e) => decodeFields(view, e.at, region.fields)));
        located = { name, kind, at, end: at + count * stride, entries, doc: region.doc };
      } else if (kind === "rows") {
        const record = layout.records.find((r) => r.name === region.of);
        const count = evaluate(region.count, env, cursor, book);
        if (!record || count === undefined) continue;
        const { entries, end } = walkRows(layout, view, record, at, count);
        env.set(name, entries.filter((e) => e.record === record.name).map((e) => decodeFields(view, e.at, record.fields)));
        located = { name, kind, at, end, entries, record: record.name, doc: region.doc };
      } else if (kind === "words") {
        const width = region.width ?? 4;
        const count = evaluate(region.count, env, cursor, book) ?? 0;
        const fields = [{ name: "word", offset: 0, width, space: "plain", doc: region.doc ?? "" }];
        located = { name, kind, at, end: at + count * width, entries: Array.from({ length: count }, (_, row) => ({ at: at + row * width, stride: width, fields, row })), doc: region.doc };
      } else if (kind === "blob") {
        const bytes = evaluate(region.bytes, env, cursor, book);
        if (bytes === undefined) continue;
        located = { name, kind, at, end: at + bytes, entries: [], doc: region.doc };
        const holds = (region as { holds?: unknown }).holds;
        if (typeof holds === "string") located.holds = holds;
      }
      if (!located) continue;
      located.end = Math.min(located.end, view.byteLength);
      out.push(located);
      cursor = located.end;
    }
  }
  return out;
};

/**
 * Per-format locators, for a layout whose JSON does not say where its
 * sections sit (the older fixture). Each reads the generated reader's constants.
 */
const DISH_RECORDS: Record<string, string> = {
  tokens: "Token", nodes: "Node", diagnostics: "Diagnostic", fixes: "Fix", edits: "Edit",
  chapters: "Chapter", verses: "Verse", members: "Member",
};
const LOCATORS: Record<string, (layout: Layout, view: DataView, reader: Module) => Located[] | null> = {
  dish: (layout, view, reader) => {
    const headerBytes = reader["HEADER_BYTES"];
    const names = reader["SECTION_NAMES"];
    const header = layout.envelope[0];
    const directory = layout.envelope[1];
    if (typeof headerBytes !== "number" || !Array.isArray(names) || !header || !directory) return null;
    const out: Located[] = [
      { name: header.region, kind: "fixed", at: 0, end: headerBytes, entries: [{ at: 0, stride: headerBytes, fields: header.fields, row: 0 }] },
      { name: directory.region, kind: "table", at: headerBytes, end: headerBytes + names.length * 8, entries: names.map((_: unknown, row: number) => ({ at: headerBytes + row * 8, stride: 8, fields: directory.fields, row })) },
    ];
    names.forEach((name: string, i: number) => {
      const at = view.getUint32(headerBytes + i * 8, true);
      const len = view.getUint32(headerBytes + i * 8 + 4, true);
      const record = layout.records.find((r) => r.name === DISH_RECORDS[name]);
      if (record) out.push({ name, kind: "rows", at, end: at + len, record: record.name, entries: walkRows(layout, view, record, at, Math.floor(len / record.stride)).entries });
      else if (name === "fixText") out.push({ name, kind: "blob", at, end: at + len, entries: [] });
      else {
        const fields = [{ name: "word", offset: 0, width: 4, space: "plain", doc: "" }];
        out.push({ name, kind: "words", at, end: at + len, entries: Array.from({ length: Math.floor(len / 4) }, (_, row) => ({ at: at + row * 4, stride: 4, fields, row })) });
      }
    });
    return out;
  },
};

// --- decoding ----------------------------------------------------------------

/** start↔end, from↔to, labelStart↔labelEnd, second_from↔second_to. */
const partnerOf = (name: string): string | undefined => {
  const pairs: [string, string][] = [["Start", "End"], ["From", "To"], ["_start", "_end"], ["_from", "_to"]];
  if (name === "start") return "end";
  if (name === "end") return "start";
  if (name === "from") return "to";
  if (name === "to") return "from";
  for (const [a, b] of pairs) {
    if (name.endsWith(a)) return name.slice(0, -a.length) + b;
    if (name.endsWith(b)) return name.slice(0, -b.length) + a;
  }
  return undefined;
};

/** `projectedFrom` is a position in a projection, not in the source text. */
const pointsIntoSource = (field: LayoutField): boolean => field.space === "offset" && !/^projected/u.test(field.name);

/** Byte offset of each UTF-16 unit, plus the end. */
const utf16ToBytes = (text: string): number[] => {
  const map: number[] = [];
  const encoder = new TextEncoder();
  let b = 0;
  for (const ch of text) {
    map.push(b);
    if (ch.length === 2) map.push(b);
    b += encoder.encode(ch).length;
  }
  map.push(b);
  return map;
};

const buildSample = async (layout: Layout, usfm: string, raw: Uint8Array, reader: Module | null): Promise<Sample> => {
  const bytes = new Uint8Array(raw);
  const view = new DataView(bytes.buffer);
  const source = new TextEncoder().encode(usfm);
  const decoder = new TextDecoder();

  const hasExpressions = layout.envelope.some((r) => r.kind !== undefined);
  let regions = hasExpressions ? locateFromLayout(layout, view) : null;
  if (!regions && reader) regions = LOCATORS[layout.format]?.(layout, view, reader) ?? null;
  const partial = !regions;
  regions ??= locateFromLayout({ ...layout, envelope: layout.envelope.slice(0, 1) }, view);

  const header = regions.find((r) => r.kind === "fixed");
  const flagsField = header?.entries[0]?.fields.find((f) => f.name === "flags");
  const flagUtf16 = typeof reader?.["FLAG_UTF16"] === "number" ? (reader["FLAG_UTF16"] as number) : 1;
  // Which unit the offsets are in: the sample's options, else the header's
  // flags, else a format that says so in its own docs (find is always UTF-16).
  const asked = (layout.sample as { options?: { utf16?: unknown } } | null | undefined)?.options?.utf16;
  const utf16 = typeof asked === "boolean" ? asked
    : header && flagsField ? (Number(readUint(view, header.at + flagsField.offset, flagsField.width)) & flagUtf16) !== 0
    : layout.records.some((r) => /UTF-16/u.test(r.doc)) || /UTF-16/u.test(layout.doc ?? "");
  const units = utf16 ? utf16ToBytes(usfm) : null;
  const toByte = (u: number): number => (units ? (units[Math.min(u, units.length - 1)] ?? u) : u);

  const sectionNames = Array.isArray(reader?.["SECTION_NAMES"]) ? (reader["SECTION_NAMES"] as string[]) : [];
  const nodeBit = typeof reader?.["NODE_ID_BIT"] === "number" ? (reader["NODE_ID_BIT"] as number) : undefined;

  const spans: Span[] = [];
  const decodedBy: Record<string, "reader" | "layout"> = {};
  const excerpt = (from: number, to: number): string => {
    const s = decoder.decode(source.subarray(from, Math.min(to, from + 120)));
    return to - from > 120 ? `${s}…` : s;
  };

  for (const region of regions) {
    if (region.kind === "blob") {
      if (region.holds) {
        if (region.end > region.at) spans.push({ start: region.at, end: region.end, region: region.name, row: 0, field: region.holds, value: `a whole ${region.holds} buffer, ${region.end - region.at} bytes`, space: "plain", hue: 8 });
        continue;
      }
      let i = region.at;
      while (i < region.end) {
        const lead = bytes[i] ?? 0;
        const len = Math.min(lead >= 0xf0 ? 4 : lead >= 0xe0 ? 3 : lead >= 0xc0 ? 2 : 1, region.end - i);
        spans.push({ start: i, end: i + len, region: region.name, row: i - region.at, field: "byte", value: JSON.stringify(decoder.decode(bytes.subarray(i, i + len))), space: "text", hue: 8 });
        i += len;
      }
      continue;
    }
    const nameRows = region.kind === "table" && sectionNames.length === region.entries.length;
    for (const entry of region.entries) {
      if (entry.at + entry.stride > bytes.byteLength) continue;
      const Row = entry.record && reader ? reader[`${entry.record}Row`] : undefined;
      let row: Record<string, unknown> | null = null;
      if (typeof Row === "function") {
        try {
          const cursor = new (Row as new (v: DataView) => Record<string, unknown>)(new DataView(bytes.buffer, entry.at, entry.stride));
          row = (cursor["seek"] as (n: number) => Record<string, unknown>).call(cursor, 0);
        } catch {
          row = null;
        }
      }
      if (entry.record) decodedBy[entry.record] ??= row ? "reader" : "layout";
      const values = new Map<string, number | bigint>();
      for (const field of entry.fields) {
        let got: unknown;
        try {
          got = row?.[camel(field.name)];
        } catch {
          got = undefined;
        }
        values.set(field.name, typeof got === "number" || typeof got === "bigint" ? got : readUint(view, entry.at + field.offset, field.width));
      }
      entry.fields.forEach((field, index) => {
        const v = values.get(field.name) ?? 0;
        let value = show(field, v);
        if (region.kind === "words" && nodeBit !== undefined && typeof v === "number") {
          value = (v & nodeBit) !== 0 ? `node ${v & ~nodeBit & 0x7fffffff}` : `token ${v}`;
        }
        const span: Span = {
          start: entry.at + field.offset, end: entry.at + field.offset + field.width, region: region.name,
          row: entry.row, field: field.name, value, space: field.space, hue: region.kind === "words" ? 8 : hueOf(index),
        };
        if (entry.record) span.record = entry.record;
        if (entry.rowName) span.rowName = entry.rowName;
        else if (nameRows && sectionNames[entry.row]) span.rowName = sectionNames[entry.row];
        if (pointsIntoSource(field) && typeof v === "number" && v !== NONE32) {
          const partner = entry.fields.find((f) => f.name === partnerOf(field.name) && pointsIntoSource(f));
          const pv = partner ? values.get(partner.name) : undefined;
          const a = toByte(v);
          const range: [number, number] = typeof pv === "number" && pv !== NONE32
            ? [Math.min(a, toByte(pv)), Math.max(a, toByte(pv))]
            : [a, a + 1];
          range[0] = Math.min(range[0], source.length);
          range[1] = Math.min(Math.max(range[1], range[0]), source.length);
          span.src = range;
          span.text = excerpt(range[0], range[1]);
        }
        spans.push(span);
      });
    }
  }

  spans.sort((a, b) => a.start - b.start);
  const owner = new Array<number>(bytes.length).fill(-1);
  spans.forEach((s, i) => {
    for (let b = s.start; b < s.end && b < owner.length; b++) if (owner[b] === -1) owner[b] = i;
  });

  // Cut the USFM at every boundary a span points to, on character starts.
  const cuts = new Set<number>([0, source.length]);
  const charStart = (b: number): number => {
    while (b < source.length && ((source[b] ?? 0) & 0xc0) === 0x80) b++;
    return b;
  };
  for (const s of spans) if (s.src) for (const b of s.src) cuts.add(charStart(b));
  const sorted = [...cuts].sort((a, b) => a - b);
  const segments: [number, number, string][] = [];
  for (let i = 0; i + 1 < sorted.length; i++) {
    const from = sorted[i] ?? 0;
    const to = sorted[i + 1] ?? 0;
    if (to > from) segments.push([from, to, decoder.decode(source.subarray(from, to))]);
  }

  return { usfm, bytes: [...bytes], spans, owner, regions, segments, decodedBy, partial, utf16 };
};

const order = (a: Wire, b: Wire): number => a.layout.format.localeCompare(b.layout.format);

let cached: Promise<Wire[]> | null = null;
export const loadWires = (): Promise<Wire[]> => (cached ??= (async () => {
  if (!existsSync(layoutsDir)) return [];
  const files = readdirSync(layoutsDir).filter((f) => f.endsWith(".json"));
  const wires = await Promise.all(files.map(async (file): Promise<Wire | null> => {
    const parsed = layoutSchema.safeParse(JSON.parse(readFileSync(join(layoutsDir, file), "utf8")));
    if (!parsed.success) {
      console.warn(`[atlas] ${file}: ${parsed.error.message}`);
      return null;
    }
    const layout = parsed.data;
    const reader = await readerFor(layout.reader);
    let sample: Sample | null = null;
    let error: string | undefined;
    if (layout.sample) {
      try {
        const usfm = readFileSync(join(layoutsDir, layout.sample.usfm), "utf8");
        const bin = readFileSync(join(layoutsDir, layout.sample.bin));
        sample = await buildSample(layout, usfm, new Uint8Array(bin), reader);
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
    }
    return { file, layout, crate: crateOf(layout.declared_in), sample, readerLoaded: reader !== null, ...(error ? { error } : {}) };
  }));
  return wires.filter((w): w is Wire => w !== null).sort(order);
})());

/** Field blocks plus the padding between them, for a strip of `bytes` bytes. */
export interface StripPart { name: string; offset: number; width: number; space: string; doc: string; hue: number; gap: boolean }
export const stripParts = (fields: LayoutField[], bytes: number): StripPart[] => {
  const parts: StripPart[] = [];
  const sorted = fields.map((f, i) => ({ f, hue: hueOf(i) })).sort((a, b) => a.f.offset - b.f.offset);
  let at = 0;
  for (const { f, hue } of sorted) {
    if (f.offset > at) parts.push({ name: "pad", offset: at, width: f.offset - at, space: "plain", doc: "Padding: no field covers these bytes.", hue: 0, gap: true });
    parts.push({ name: f.name, offset: f.offset, width: f.width, space: f.space, doc: f.doc, hue, gap: false });
    at = Math.max(at, f.offset + f.width);
  }
  if (bytes > at) parts.push({ name: "pad", offset: at, width: bytes - at, space: "plain", doc: "Padding: no field covers these bytes.", hue: 0, gap: true });
  return parts;
};

/** Bytes one entry of an envelope region takes, when it is fixed. */
export const regionBytes = (region: Layout["envelope"][number]): number | null => {
  if (region.entry_bytes !== undefined) return region.entry_bytes;
  if (typeof region.bytes === "number") return region.bytes;
  if (region.fields.length > 0) return Math.max(...region.fields.map((f) => f.offset + f.width));
  return null;
};
