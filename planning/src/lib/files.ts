/** A row in a <Files> tree. */
export type Tone = "new" | "changed" | "deleted" | "moved" | "same";
export interface Row {
  name: string;
  path: string;
  tone: Tone;
  note?: string | undefined;
  line?: string | undefined;
  children: Row[];
  folder: boolean;
}
