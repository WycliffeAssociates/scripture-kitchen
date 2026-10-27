/**
 * The sample's hex grid as lines, in two layouts.
 *
 *   bytes   header ················ 0000 4f 4e 49 …          16 per line, every
 *           tokens  Token × 41      0070 ▏00 00 00 00 03 …   region on a fresh line,
 *                                                            ▏ where a row starts
 *   rows    tokens  Token × 41
 *                   start       end         kind marker …     one row per line, so a
 *           0070 #0 00 00 00 00 03 00 00 00 01   02   …       field is a column
 *           007c #1 03 00 00 00 05 00 00 00 03   …
 */
import type { Located, Sample } from "./layouts.ts";

export interface FieldLabel { name: string; col: number; width: number; hue: number }

export type HexLine =
  | { kind: "label"; name: string; note: string; at: number }
  /** Regions with no bytes in this sample, where they would have been. */
  | { kind: "empty"; names: string[]; at: number }
  | { kind: "fields"; fields: FieldLabel[] }
  | { kind: "bytes"; off: number; tag: string; cells: (number | null)[] };

export interface HexCell {
  /** `tokens:3`: the record row a byte belongs to, so the whole row can light. */
  row: string;
  /** Set on the first byte of every row after a region's first. */
  rowStart: string | undefined;
}

export interface HexView {
  lines: HexLine[];
  /** Cells per bytes line: 16, or the widest row. */
  width: number;
}

const ROW = 16;

type Region = Omit<Located, "kind"> & { kind: Located["kind"] | "unmapped" };

const noteOf = (r: Region): string => {
  if (r.kind === "fixed") return `${r.end - r.at} B`;
  if (r.kind === "blob") return r.holds ? `a ${r.holds} buffer, ${r.end - r.at} B` : `UTF-8 text, ${r.end - r.at} B`;
  if (r.kind === "unmapped") return `${r.end - r.at} B not placed`;
  const head = r.entries[0];
  const what = r.record ?? head?.record ?? "row";
  return `${what} × ${r.entries.length}, ${head?.stride ?? 0} B each`;
};

/** The sample's regions in order, without overlaps, with unplaced bytes as their own region. */
export const regionsOf = (sample: Sample): { regions: Region[]; empty: Region[] } => {
  const placed = sample.regions.filter((r) => r.end > r.at).toSorted((a, b) => a.at - b.at);
  const empty = sample.regions.filter((r) => r.end <= r.at);
  const regions: Region[] = [];
  let cursor = 0;
  const gap = (to: number): void => {
    if (to > cursor) regions.push({ name: "unmapped", kind: "unmapped", at: cursor, end: to, entries: [] });
  };
  for (const r of placed) {
    if (r.end <= cursor) continue;
    gap(r.at);
    const at = Math.max(r.at, cursor);
    regions.push({ ...r, at, entries: r.entries.filter((e) => e.at >= at) });
    cursor = r.end;
  }
  gap(sample.bytes.length);
  return { regions, empty };
};

/** Which record row each byte sits in, and where rows start. */
export const cellsOf = (sample: Sample): HexCell[] => {
  const cells: HexCell[] = sample.bytes.map(() => ({ row: "", rowStart: undefined }));
  for (const r of regionsOf(sample).regions) {
    r.entries.forEach((e, n) => {
      const key = `${r.name}:${e.row}`;
      const label = e.rowName ?? `${e.record ?? r.name} ${e.row}`;
      for (let b = e.at; b < e.at + e.stride && b < cells.length; b++) {
        const cell = cells[b];
        if (cell) cell.row = key;
      }
      const first = cells[e.at];
      if (first && n > 0) first.rowStart = label;
    });
  }
  return cells;
};

const hueAt = (sample: Sample, i: number): number => sample.spans[sample.owner[i] ?? -1]?.hue ?? 0;

const label = (r: Region): HexLine => ({ kind: "label", name: r.name, note: noteOf(r), at: r.at });

/** Each region's lines after its label, with empty regions noted where they fall. */
const walk = (sample: Sample, body: (r: Region) => HexLine[]): HexLine[] => {
  const { regions, empty } = regionsOf(sample);
  const lines: HexLine[] = [];
  let e = 0;
  const flush = (upTo: number): void => {
    const names: string[] = [];
    let at = 0;
    while (e < empty.length && (empty[e]?.at ?? 0) <= upTo) {
      at = empty[e]?.at ?? 0;
      names.push(empty[e]?.name ?? "");
      e++;
    }
    if (names.length) lines.push({ kind: "empty", names, at });
  };
  for (const r of regions) {
    flush(r.at);
    lines.push(label(r), ...body(r));
  }
  flush(Infinity);
  return lines;
};

/** 16 per line, aligned to absolute offsets, each region from a fresh line. */
const sixteen = (from: number, to: number): HexLine[] => {
  const lines: HexLine[] = [];
  for (let off = Math.floor(from / ROW) * ROW; off < to; off += ROW) {
    const cells = Array.from({ length: ROW }, (_, k) => (off + k >= from && off + k < to ? off + k : null));
    lines.push({ kind: "bytes", off, tag: "", cells });
  }
  return lines;
};

export const byteView = (sample: Sample): HexView => ({
  width: ROW,
  lines: walk(sample, (r) => sixteen(r.at, r.end)),
});

/** One row per line (several when a row is 8 bytes or fewer), under a line naming its fields. */
export const rowView = (sample: Sample): HexView => {
  let width = ROW;
  const lines = walk(sample, (r) => {
    if (r.entries.length === 0) return sixteen(r.at, r.end);
    const lines: HexLine[] = [];
    // Consecutive rows of one shape share a field line.
    const groups: (typeof r.entries)[] = [];
    for (const e of r.entries) {
      const last = groups.at(-1)?.[0];
      const same = last && last.stride === e.stride && last.record === e.record
        && last.fields.map((f) => f.name).join() === e.fields.map((f) => f.name).join();
      if (same) groups.at(-1)?.push(e);
      else groups.push([e]);
    }
    let covered = r.at;
    for (const group of groups) {
      const head = group[0];
      if (!head || head.stride === 0) continue;
      const per = head.stride <= ROW / 2 ? Math.floor(ROW / head.stride) : 1;
      width = Math.max(width, head.stride * per);
      if (head.at > covered) lines.push(...sixteen(covered, head.at));
      const fields: FieldLabel[] = [];
      for (let k = 0; k < Math.min(per, group.length); k++) {
        for (const f of head.fields) {
          fields.push({ name: f.name, col: k * head.stride + f.offset, width: f.width, hue: hueAt(sample, head.at + f.offset) });
        }
      }
      lines.push({ kind: "fields", fields });
      for (let n = 0; n < group.length; n += per) {
        const rows = group.slice(n, n + per);
        const cells = rows.flatMap((e) => Array.from({ length: e.stride }, (_, k) => (e.at + k < sample.bytes.length ? e.at + k : null)));
        const first = rows[0];
        const last = rows.at(-1);
        lines.push({ kind: "bytes", off: first?.at ?? 0, tag: rows.length > 1 ? `#${first?.row}–${last?.row}` : `#${first?.row}`, cells });
        covered = (last?.at ?? 0) + (last?.stride ?? 0);
      }
    }
    if (r.end > covered) lines.push(...sixteen(covered, r.end));
    return lines;
  });
  return { lines, width };
};
