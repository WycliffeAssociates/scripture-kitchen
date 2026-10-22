//! `onion-wasm` — the JS doorway over the USFM engine (`onion`).
//!
//! This is **piece 2 of the five-crate layout** (onion, ONION-WASM, sous,
//! sous-wasm, galley): bindgen,
//! the `.d.ts`, and the JS-environment utilities (UTF-16 walls, the decoder
//! wrapper) for THIS engine and nothing else.
//!
//! `galley` is the RESERVED name for the future WORKFLOWS crate over
//! onion + sous — dirty-marking, find, ingest recipes, onion↔sous
//! coordination. This crate will be one of galley's dependencies. It was
//! briefly named `galley` itself; that was a naming error, corrected here.
//!
//! **Nothing is implemented here.** Every export is a tag over a library
//! function plus, where the wall demands it, an offset conversion or a wire
//! struct. If an export starts to have logic, the logic belongs in a library.
//!
//! # The contract
//!
//! - **Stateless.** Text in, bytes out, nothing retained between calls. The
//!   caller pairs each result with the document version it sent; stale =
//!   discard. The document crosses at ~80us for a typical book, which is why
//!   ONE call carries several products rather than several calls carrying one.
//! - **`parse` returns a BUFFER.** One plated dish — tokens, the tree, and
//!   whatever the options asked for — read by `reader.ts`, which is generated
//!   from the same schema as the writer. The `Uint8Array` is JS's and the
//!   collector reclaims it; there is nothing to free. The write path still
//!   hands out handles (`FormatOpts`, `Edits`, `Splices`), and wasm-bindgen
//!   registers a `FinalizationRegistry` for those by default.
//! - **Nothing rich crosses, and nothing is decoded by hand.** JS never holds a
//!   token; it reads one out of the buffer through a generated accessor. The
//!   one structured export is the diff skeleton, a cold modal-open path.
//! - **Offsets are UTF-8 bytes unless asked otherwise.** `parse`'s `utf16` flag
//!   converts every offset in the dish to a CodeMirror code-unit offset. It is
//!   OPT-IN because bytes are what the engine holds, what a native consumer
//!   wants, and what sous reads — only a JS editor counts in UTF-16.
//! - **LF-canonical input.** CodeMirror counts a line break as ONE position
//!   while a literal `\r\n` is TWO UTF-16 code units, so CRLF input yields
//!   offsets one ahead of the editor's from the first line onward. The vision
//!   canonicalizes at ingress (§6.3), so LF-in is the contract; `parse`
//!   debug-asserts it and never repairs it.
//! - **The marker table DOES cross — generated, not mirrored.** A token carries
//!   its row index and `reader.ts` ships the 153 rows, so a consumer reads a
//!   marker's kind and category without slicing the document and without
//!   restating a bit layout. Nothing is coarsened on the way: every kind and
//!   every category arrives, because a consumer that wants its own vocabulary
//!   must be able to switch exhaustively into it.
//!
//! # What is deliberately absent
//!
//! A **checksum** export. Vision §13.4 puts the canonical-source checksum on
//! this facade; Will deferred it (2026-08-24) as a higher-level concern — it
//! lands in `galley`, which is where save/dirty machinery will live. A dish's
//! header carries an xxh3-64 of the source it was plated from, which answers
//! "is this the parse of that text" and nothing wider.

use std::sync::Arc;

use js_sys::{Object, Reflect, Uint32Array};
use serde::Serialize;
use usfm_onion::attributes::{self, AttrEvent};
use usfm_onion::diff::{
    self, Addr, CoveredSide, Decisions, DiffSkeleton, MergeSide, RunKind, RunWhat, SlotRole,
    Status, TextDiffMode, TextDiffRun, UnitKind, UnitTextDiff,
};
use usfm_onion::extensions::{CustomMarker, ExtensionCategory, ExtensionOptions, Extensions};
use usfm_onion::format::{CharBreaks, FormatOptions, Newline, VerseBreaks};
use usfm_onion::lint::Code;
use usfm_onion::mask as mask_recipe;
use usfm_onion::tables::generated::MarkerIdx;
use usfm_onion::utf16::Utf16Index;
use usfm_onion::wire;
use usfm_onion::{Token, TokenKind};
use wasm_bindgen::prelude::*;

pub mod options;
pub mod resolution;

// ---------------------------------------------------------------------------
// parse
// ---------------------------------------------------------------------------

/// THE read call. One document in, one buffer out.
///
/// ```ts
/// interface ParseTextOptions {
///   diagnostics?: boolean;          // run the lint walk; default false
///   toc?: boolean;                  // build the chapter and verse index; default false
///   utf16?: boolean;                // every offset as a UTF-16 code unit; default bytes
///   customMarkers?: CustomMarkers;  // the `\z` list to lex by; default the installed one
/// }
/// parse(text: string, opts?: ParseTextOptions): Uint8Array;
/// ```
///
/// The buffer is a plated parse — tokens, the tree, and whatever `opts` asked
/// for besides — read by `reader.ts`, which is generated from the same schema
/// as the writer. Nothing is retained wasm-side: the `Uint8Array` is JS's, the
/// collector reclaims it, and there is no `free`.
///
/// A misspelled key is a compile error through the declared `ParseOptions`,
/// and a THROW at the wall for a caller the compiler never saw: an unknown key
/// is refused by name rather than read as `false` ([`options`]).
///
/// `text` must be LF-normalized (see the module doc); a debug build asserts it.
#[wasm_bindgen]
pub fn parse(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "ParseTextOptions")] opts: Option<JsValue>,
) -> Result<Vec<u8>, JsError> {
    let (opts, markers) =
        options::parse_text_options(opts.as_ref(), "parse").map_err(|e| JsError::new(&e))?;
    Ok(plated(text, opts, &options::or_installed(markers)))
}

/// [`parse`]'s body over engine options, for a caller on this side of the
/// wall — `galley`, and the native tests, where no `JsValue` exists.
pub fn plated(text: &str, opts: wire::ParseOptions, markers: &Extensions) -> Vec<u8> {
    debug_assert!(
        !text.as_bytes().contains(&b'\r'),
        "onion_wasm::parse: the text must be LF-normalized — a CR makes every \
         emitted offset disagree with CodeMirror's"
    );
    wire::plate(&wire::parse_with(text, opts, markers))
}

#[wasm_bindgen(typescript_custom_section)]
const OPTIONS_TS: &str = r#"
/** `parse`'s options, and `Galley.parse`/`parseText`'s. Every key defaults to false. */
export interface ParseOptions {
  /** Run the lint walk. The one expensive optional. */
  diagnostics?: boolean;
  /** Build the chapter and verse index. Cheap, and needs no tree. */
  toc?: boolean;
  /** Emit every offset as a UTF-16 code unit instead of a byte. */
  utf16?: boolean;
}

/** The list a door that takes text lexes `\z` markers by. */
export interface LexOptions {
  /** Default: the list `setCustomMarkers` installed. */
  customMarkers?: CustomMarkers;
}

/** `parse(text, opts)`, and galley's `parseText`. */
export interface ParseTextOptions extends ParseOptions, LexOptions {}

/** `diff`'s options. */
export interface DiffOptions extends LexOptions {
  /** The runs a changed unit is highlighted by: UAX-29 words, graphemes, or none. */
  text: "none" | "words" | "chars";
  /** Send unchanged units too, in their places; default false. */
  unchanged?: boolean;
}

/** `attrs`'s options. */
export interface AttrsOptions {
  /** `from`/`to` and every word returned are UTF-16 code units; default bytes. */
  utf16?: boolean;
}

/** galley's `resolve(id, address, opts)`. */
export interface ResolveOptions {
  /** Offsets as UTF-16 code units; default bytes. */
  utf16?: boolean;
}

/** `resolve(text, address, opts)`, and galley's `resolveOf`. */
export interface ResolveTextOptions extends ResolveOptions, LexOptions {}

/** A place in scripture, Sefer's `Address`. A `book` key is read past: the book is the caller's to match. */
export type Address =
  | { kind: "book" }
  | { kind: "intro" }
  | { kind: "chapters"; from: number; to: number }
  | { kind: "verses"; from: AddressPoint; to: AddressPoint };

export interface AddressPoint {
  chapter: number;
  verse: number;
  /** `a` in `3a`; absent for the whole verse. */
  segment?: string;
}

/** What `resolve` answers, JSON-encoded. Offsets are half-open. */
export type Resolution =
  | { found: { from: number; to: number; opening: number; covers: Address; coarser: boolean } }
  | { missing: "intro" | "chapter" | "verse"; within?: { from: number; to: number } }
  | { ambiguous: { from: number; to: number; opening: number }[] };

/** `customMarkers`'s options. */
export interface CustomMarkerOptions {
  /** Admit a legacy name without the `z` that the spec does not define. */
  relaxZPrefix?: boolean;
}
"#;

/// One mask recipe's text, and the map back to the source it was cut from.
///
/// Not a section of a dish: a mask is a different question with its own
/// parameter, and most callers never want one. Not hot and not large either,
/// so it takes the string like any other call rather than the buffer.
///
/// `ranges` are the kept SOURCE spans — sorted, disjoint, maximal — and
/// `starts[i]` is the prefix sum, so `ranges[i]`'s bytes sit at `starts[i]..`
/// in `text`. That pair is the map: a finding at a masked offset maps back by
/// a binary search on `starts`.
///
/// Offsets stay in UTF-8 bytes. The mask is onion-to-sous and never reaches an
/// editor, which is the only consumer that counts in UTF-16.
#[wasm_bindgen(js_name = mask)]
pub fn mask(
    text: &str,
    recipe: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "LexOptions")] opts: Option<JsValue>,
) -> Result<Object, JsError> {
    let markers = options::lex_options(opts.as_ref(), "mask").map_err(|e| JsError::new(&e))?;
    let filter = match recipe {
        "verseText" => mask_recipe::Filter::verse_text(),
        "structure" => mask_recipe::Filter::structure(),
        "text" => mask_recipe::Filter::text(),
        other => wasm_bindgen::throw_str(&format!(
            "onion_wasm::mask: no such recipe {other:?} — \
             expected \"verseText\", \"structure\" or \"text\""
        )),
    };
    let tokens = usfm_onion::lex_with(text, &options::or_installed(markers));
    let cst = usfm_onion::cst::build(&tokens);
    let masked = usfm_onion::mask(text.as_bytes(), &tokens, &cst, &filter);

    let out = Object::new();
    let set = |key: &str, value: JsValue| {
        Reflect::set(&out, &JsValue::from_str(key), &value)
            .expect("a fresh Object always accepts a property");
    };
    let mut ranges = Vec::with_capacity(masked.ranges.len() * 2);
    for span in &masked.ranges {
        ranges.push(span.start);
        ranges.push(span.end);
    }
    set("text", JsValue::from_str(&masked.text(text.as_bytes())));
    set("ranges", JsValue::from(Uint32Array::from(&ranges[..])));
    set(
        "starts",
        JsValue::from(Uint32Array::from(&masked.starts[..])),
    );
    Ok(out)
}

// ---------------------------------------------------------------------------
// format
// ---------------------------------------------------------------------------

/// The formatter's switches, defaulted to [`FormatOptions::default`].
///
/// A tagged struct rather than a dozen positional booleans: the `.d.ts` names
/// each switch, and adding one later does not renumber a call site.
#[wasm_bindgen]
pub struct FormatOpts {
    /// 0 = keep the line break in front of a `\v`, 1 = fold it into a space.
    pub verse_breaks: u32,
    /// 0 = keep a break on a character-marker boundary, 1 = join it into a
    /// space (the aligned-corpus shape).
    pub char_marker_breaks: u32,
    /// 0 = LF, 1 = CRLF.
    pub newline: u32,
    pub block_marker_own_line: bool,
    pub collapse_blank_lines: bool,
    pub normalize_newlines: bool,
    pub trim_text_edges: bool,
    pub delimiter_single: bool,
    pub designator_ws_single: bool,
    pub marker_ws_at_line_start: bool,
    pub dedupe_verse_number: bool,
    pub bridge_empty_verses: bool,
    remove_markers: Vec<String>,
    repairs: Vec<u32>,
    custom_markers: Option<Arc<Extensions>>,
}

impl Default for FormatOpts {
    fn default() -> Self {
        let d = FormatOptions::default();
        Self {
            verse_breaks: u32::from(d.verse_breaks == VerseBreaks::Remove),
            char_marker_breaks: u32::from(d.char_marker_breaks == CharBreaks::Join),
            newline: 0,
            block_marker_own_line: d.block_marker_own_line,
            collapse_blank_lines: d.collapse_blank_lines,
            normalize_newlines: d.normalize_newlines,
            trim_text_edges: d.trim_text_edges,
            delimiter_single: d.delimiter_single,
            designator_ws_single: d.designator_ws_single,
            marker_ws_at_line_start: d.marker_ws_at_line_start,
            dedupe_verse_number: d.dedupe_verse_number,
            bridge_empty_verses: d.bridge_empty_verses,
            remove_markers: Vec::new(),
            repairs: Vec::new(),
            custom_markers: None,
        }
    }
}

#[wasm_bindgen]
impl FormatOpts {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marker names, no backslash, comma-separated — `"s5"` for the
    /// unfoldingWord chunk marker. Every occurrence is deleted outright.
    #[wasm_bindgen(js_name = setRemoveMarkers)]
    pub fn set_remove_markers(&mut self, names: &str) {
        self.remove_markers = names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();
    }

    /// Lint codes (the `diagnostics.json` indices) whose existing fixes join
    /// the format transaction. An index naming no code is ignored.
    #[wasm_bindgen(js_name = setRepairs)]
    pub fn set_repairs(&mut self, codes: &[u32]) {
        self.repairs = codes.to_vec();
    }

    /// The list the text lexes `\z` markers by, instead of the one
    /// `setCustomMarkers` installed. Throws where a door refuses its
    /// `customMarkers` option.
    #[wasm_bindgen(js_name = setCustomMarkers)]
    pub fn set_custom_markers(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "CustomMarkers")] markers: JsValue,
    ) -> Result<(), JsError> {
        let door = "FormatOpts.setCustomMarkers";
        let (list, opts) =
            options::read_custom_markers(&markers, door).map_err(|e| JsError::new(&e))?;
        self.custom_markers =
            Some(options::interned(door, list, &opts).map_err(|e| JsError::new(&e))?);
        Ok(())
    }
}

impl FormatOpts {
    /// The library options this stands for. Borrows the two owned lists, which
    /// is why the caller holds `names`/`codes` for the call's duration.
    fn to_options<'a>(&'a self, names: &'a [&'a str], codes: &'a [Code]) -> FormatOptions<'a> {
        FormatOptions {
            verse_breaks: if self.verse_breaks == 0 {
                VerseBreaks::Keep
            } else {
                VerseBreaks::Remove
            },
            char_marker_breaks: if self.char_marker_breaks == 0 {
                CharBreaks::Keep
            } else {
                CharBreaks::Join
            },
            newline: if self.newline == 0 {
                Newline::Lf
            } else {
                Newline::CrLf
            },
            remove_markers: names,
            repairs: codes,
            block_marker_own_line: self.block_marker_own_line,
            collapse_blank_lines: self.collapse_blank_lines,
            normalize_newlines: self.normalize_newlines,
            trim_text_edges: self.trim_text_edges,
            delimiter_single: self.delimiter_single,
            designator_ws_single: self.designator_ws_single,
            marker_ws_at_line_start: self.marker_ws_at_line_start,
            dedupe_verse_number: self.dedupe_verse_number,
            bridge_empty_verses: self.bridge_empty_verses,
            custom_markers: self.custom_markers.as_deref(),
        }
    }

    fn names(&self) -> Vec<&str> {
        self.remove_markers.iter().map(String::as_str).collect()
    }

    fn codes(&self) -> Vec<Code> {
        self.repairs
            .iter()
            .filter_map(|code| {
                usfm_onion::lint::LINT_ROWS
                    .get(*code as usize)
                    .map(|row| row.code)
            })
            .collect()
    }
}

/// One transaction of proposed splices: `[from, to]` pairs, one concatenated
/// ASCII insert blob, one length per edit.
///
/// The same shape a fix crosses in — an editor session applies both the same
/// way, and `lens[i] == 0` is a pure deletion. `spans` and `lens` are always
/// in the SAME unit, because `lens` slices `text` at offsets `spans` place:
/// the formatter's doors here answer in UTF-16 throughout, and a producer in
/// another crate names its own unit (`galley`'s overlay answers bytes unless
/// asked for UTF-16).
///
/// **At most one edit per position.** A pure insert landing where the edit
/// before it ends is folded into that edit, so an applier's order among
/// same-position edits can never matter:
///
/// ```text
/// engine    [66,66) "\n\q2"  [66,66) "\n\q3"  [66,66) "\n\q4"
/// crosses   [66,66) "\n\q2\n\q3\n\q4"
/// ```
///
/// Applied either way the text is the same; only the entry count differs.
#[wasm_bindgen]
pub struct Edits {
    spans: Vec<u32>,
    lens: Vec<u32>,
    text: String,
}

impl Edits {
    /// The three parallel arrays, from a producer in another crate. Not a
    /// `wasm_bindgen` constructor: the class stays read-only from JS, and the
    /// caller owns the unit its spans are in.
    pub fn from_parts(spans: Vec<u32>, lens: Vec<u32>, text: String) -> Edits {
        debug_assert_eq!(spans.len(), lens.len() * 2, "one from/to pair per edit");
        let mut out = Edits {
            spans: Vec::with_capacity(spans.len()),
            lens: Vec::with_capacity(lens.len()),
            text,
        };
        for (span, len) in spans.chunks_exact(2).zip(lens) {
            out.push(span[0], span[1], len);
        }
        out
    }

    /// Appends one edit whose insert is already on `text`, folding a pure
    /// insert at the previous edit's end into it.
    fn push(&mut self, from: u32, to: u32, len: u32) {
        if from == to
            && let (Some(&end), Some(last)) = (self.spans.last(), self.lens.last_mut())
            && end == from
        {
            *last += len;
            return;
        }
        self.spans.extend_from_slice(&[from, to]);
        self.lens.push(len);
    }
}

#[wasm_bindgen]
impl Edits {
    #[wasm_bindgen(getter)]
    pub fn spans(&self) -> Vec<u32> {
        self.spans.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn lens(&self) -> Vec<u32> {
        self.lens.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }
}

/// The formatter's edit list, offsets converted to UTF-16 here.
///
/// The index is built per call: an edit list is small and the conversion is
/// random-access (edits are sorted, but a stride index is ~0.1ms and this is a
/// user-triggered path, not a keystroke one).
#[wasm_bindgen(js_name = formatEdits)]
pub fn format_edits(text: &str, opts: &FormatOpts) -> Edits {
    let names = opts.names();
    let codes = opts.codes();
    let edits = usfm_onion::format_edits(text.as_bytes(), &opts.to_options(&names, &codes));
    wire_edits(&Utf16Index::new(text.as_bytes()), &edits)
}

/// The same transaction bounded to `from..to` (UTF-16, the offsets the editor
/// already holds — a chapter's span out of the `chapters` read).
///
/// The range crosses the wall in UTF-16 and is translated here, on the same
/// index the edits go out through. The policy is the library's: an edit is kept
/// only if its whole span is inside, a multi-edit claim only if all of it is,
/// and a pure insertion sitting ON either edge is inside.
#[wasm_bindgen(js_name = formatEditsIn)]
pub fn format_edits_in(text: &str, from: u32, to: u32, opts: &FormatOpts) -> Edits {
    let names = opts.names();
    let codes = opts.codes();
    let index = Utf16Index::new(text.as_bytes());
    let range = index.to_byte(from)..index.to_byte(to);
    let edits =
        usfm_onion::format_edits_in(text.as_bytes(), range, &opts.to_options(&names, &codes));
    wire_edits(&index, &edits)
}

/// The edit wire: spans converted to UTF-16, inserts concatenated.
///
/// The index is built per call: an edit list is small and the conversion is
/// random-access (edits are sorted, but a stride index is ~0.1ms and this is a
/// user-triggered path, not a keystroke one).
fn wire_edits(index: &Utf16Index, edits: &[usfm_onion::Edit]) -> Edits {
    let mut out = Edits {
        spans: Vec::with_capacity(edits.len() * 2),
        lens: Vec::with_capacity(edits.len()),
        text: String::new(),
    };
    for edit in edits {
        out.text.push_str(edit.insert.as_str());
        out.push(
            index.to_utf16(edit.from),
            index.to_utf16(edit.to),
            edit.insert.as_bytes().len() as u32,
        );
    }
    out
}

/// The formatted document. `format_edits` applied, in one call.
#[wasm_bindgen]
pub fn format(text: &str, opts: &FormatOpts) -> String {
    let names = opts.names();
    let codes = opts.codes();
    let bytes = usfm_onion::format::format(text.as_bytes(), &opts.to_options(&names, &codes));
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// diff — the one rich structure
// ---------------------------------------------------------------------------

/// The skeleton crosses as serde JSON. Cold path (a review opens), zero drift,
/// camelCase. The unit ids are rendered in RUST — a JS-side id renderer is the
/// one place identity could drift, and it is rejected.
///
/// ```text
/// baseline  \id GEN ␊ \c 1 ␊ \v 1 one ␊ \v 2 two ␊ \v 3 three ␊
/// current   \id GEN ␊ \c 1 ␊ \v 1 one ␊ \v 3 three ␊
///
/// units           [ { unitId: "GEN 1:2", status: "deleted",
///                     baseline: [22, 31],   ← its bytes
///                     current:  [22, 22] }] ← where it goes back
/// unchangedCount  4   (GEN 0:0, 1:0, 1:1 and 1:3, not sent)
/// ```
///
/// Units come in the CURRENT document's order: every `current` span, points
/// included, starts at or after the one before. The baseline spans are not
/// always in order (a rebridged pair can sit on either side of a verse added
/// beside it), so a baseline view sorts by `baseline[0]`. Nothing refers to a
/// unit by index, so a dropped unchanged unit leaves nothing dangling.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSkeleton {
    baseline_len: u32,
    current_len: u32,
    /// Every unchanged unit, sent or not.
    unchanged_count: u32,
    units: Vec<WireUnit>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireUnit {
    /// Opaque. Decisions travel as `{unitId: side}` and nothing may parse it.
    unit_id: String,
    kind: &'static str,
    status: &'static str,
    /// The rendered address of each side, `null` when the unit is one-sided.
    baseline_sid: Option<String>,
    current_sid: Option<String>,
    /// UTF-16 spans of each side's own document. The absent side of a
    /// one-sided unit is EMPTY (`from == to`) at the point where the unit
    /// would go in that document; its sid is `null`.
    baseline: [u32; 2],
    current: [u32; 2],
    relabeled: bool,
    baseline_count: u32,
    current_count: u32,
    is_dup: bool,
    covered_by: Option<WireCoveredBy>,
    is_whitespace_change: bool,
    is_usfm_structure_change: bool,
    /// Present only when `text` asked for runs AND this unit has any — an
    /// `unchanged` unit carries none, so the field is ABSENT rather than null.
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<WireUnitText>,
}

/// One unit's intra-verse runs. Presentation only: the units, the slots and
/// every merge are byte-identical whether or not this was asked for.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireUnitText {
    /// Kinds: `unchanged` | `removed`. Empty when the unit is one-sided.
    baseline: Vec<WireTextRun>,
    /// Kinds: `unchanged` | `added`.
    current: Vec<WireTextRun>,
}

/// One run, spanned in its own side's document. `from`/`to` are UTF-16 offsets
/// like every other span here; `what` says which of the three the bytes are, so
/// a renderer hiding markup drops `"markup"` and concatenates the rest — with
/// `note: true` runs kept apart, because a footnote is its own reading. Its
/// bytes are `source.slice(from, to)` on the same side — the run carries no
/// text of its own.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireTextRun {
    from: u32,
    to: u32,
    kind: &'static str,
    what: &'static str,
    note: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireCoveredBy {
    sid: String,
    side: &'static str,
}

fn runs(runs: &[TextDiffRun], index: &Utf16Index<'_>) -> Vec<WireTextRun> {
    runs.iter()
        .map(|run| WireTextRun {
            from: index.to_utf16(run.from),
            to: index.to_utf16(run.to),
            kind: match run.kind {
                RunKind::Unchanged => "unchanged",
                RunKind::Added => "added",
                RunKind::Removed => "removed",
            },
            what: match run.what {
                RunWhat::Markup => "markup",
                RunWhat::Text => "text",
                RunWhat::Whitespace => "whitespace",
            },
            note: run.note,
        })
        .collect()
}

fn wire(
    skeleton: &DiffSkeleton,
    texts: &[Option<UnitTextDiff>],
    baseline: &Utf16Index<'_>,
    current: &Utf16Index<'_>,
    unchanged: bool,
) -> WireSkeleton {
    let sid = |addr: &Option<Addr>| addr.map(|addr| addr.to_string());
    // The current document's order: each unit at the slot that places it in
    // the current, which is every slot but a pair's baseline half. A deleted
    // unit sits at its own slot, where its point is.
    let order = skeleton
        .slots
        .iter()
        .filter(|slot| slot.role != SlotRole::PairBaseline)
        .map(|slot| slot.unit as usize);
    WireSkeleton {
        baseline_len: baseline.len_utf16(),
        current_len: current.len_utf16(),
        unchanged_count: skeleton
            .units
            .iter()
            .filter(|unit| unit.status == Status::Unchanged)
            .count() as u32,
        units: order
            .map(|index| (&skeleton.units[index], &texts[index]))
            .filter(|(unit, _)| unchanged || unit.status != Status::Unchanged)
            .map(|(unit, text)| WireUnit {
                unit_id: unit.id.clone(),
                kind: match unit.kind {
                    UnitKind::Shared => "shared",
                    UnitKind::Added => "added",
                    UnitKind::Deleted => "deleted",
                    UnitKind::Coalesced => "coalesced",
                },
                status: match unit.status {
                    Status::Unchanged => "unchanged",
                    Status::Modified => "modified",
                    Status::Added => "added",
                    Status::Deleted => "deleted",
                },
                baseline_sid: sid(&unit.baseline_addr),
                current_sid: sid(&unit.current_addr),
                baseline: [
                    baseline.to_utf16(unit.baseline.start),
                    baseline.to_utf16(unit.baseline.end),
                ],
                current: [
                    current.to_utf16(unit.current.start),
                    current.to_utf16(unit.current.end),
                ],
                relabeled: unit.relabeled,
                baseline_count: unit.dup_context.baseline_count,
                current_count: unit.dup_context.current_count,
                is_dup: unit.dup_context.is_dup(),
                covered_by: unit.covered_by.map(|covered| WireCoveredBy {
                    sid: covered.addr.to_string(),
                    side: match covered.side {
                        CoveredSide::Baseline => "baseline",
                        CoveredSide::Current => "current",
                    },
                }),
                is_whitespace_change: unit.is_whitespace_change,
                is_usfm_structure_change: unit.is_usfm_structure_change,
                text: text.as_ref().map(|text| WireUnitText {
                    baseline: runs(&text.baseline, baseline),
                    current: runs(&text.current, current),
                }),
            })
            .collect(),
    }
}

/// The diff skeleton as JSON: the changed units of two documents, in the
/// current document's order. Spans are UTF-16 offsets into each side's own document.
///
/// ```ts
/// interface DiffOptions {
///   text: "none" | "words" | "chars";   // required
///   unchanged?: boolean;                // send unchanged units too; default false
/// }
/// diff(baseline: string, current: string, opts: DiffOptions): string;
/// ```
///
/// `text` is the intra-verse runs a `modified`, `added` or `deleted` unit is
/// highlighted by, at UAX-29 word or grapheme grain; `"none"` computes nothing
/// — no CST, no mask. It is required: which of the three a screen draws is the
/// caller's to say. An unknown key, a wrong type or an unknown mode throws by
/// name.
///
/// `unchanged: true` adds every unchanged unit in its place, for a view that
/// draws the whole book from the skeleton. Nothing else changes: the ids, the
/// spans and the runs are the same in both.
#[wasm_bindgen]
pub fn diff(
    baseline: &str,
    current: &str,
    #[wasm_bindgen(unchecked_param_type = "DiffOptions")] opts: JsValue,
) -> Result<String, JsError> {
    let opts = diff_options(Some(&opts)).map_err(|error| JsError::new(&error))?;
    diffed(baseline, current, opts).map_err(|error| JsError::new(&error))
}

/// [`diff`]'s options, read.
struct DiffOptions {
    text: TextDiffMode,
    unchanged: bool,
    markers: Arc<Extensions>,
}

fn diff_options(value: Option<&JsValue>) -> Result<DiffOptions, String> {
    let bag = options::bag(value, "diff", &["text", "unchanged", "customMarkers"])?;
    let text = options::get(bag.as_ref(), "text")
        .ok_or("diff: text is required; expected \"none\", \"words\" or \"chars\"")?
        .as_string()
        .ok_or("diff: text must be a string")?;
    Ok(DiffOptions {
        text: mode(&text)?,
        unchanged: options::flag(bag.as_ref(), "diff", "unchanged")?,
        markers: options::or_installed(options::custom_markers(bag.as_ref(), "diff")?),
    })
}

/// `diff`'s body, split from the door for the same reason [`merged`] is.
fn diffed(baseline: &str, current: &str, opts: DiffOptions) -> Result<String, String> {
    let (skeleton, texts) = diff::diff_with_text_under(baseline, current, opts.text, &opts.markers);
    let wire = wire(
        &skeleton,
        &texts,
        &Utf16Index::new(baseline.as_bytes()),
        &Utf16Index::new(current.as_bytes()),
        opts.unchanged,
    );
    Ok(serde_json::to_string(&wire).expect("the wire structs are always serializable"))
}

/// An unknown mode is an error rather than a default: a typo silently meaning
/// `"none"` would drop an editor's highlighting with nothing to see.
fn mode(name: &str) -> Result<TextDiffMode, String> {
    match name {
        "none" => Ok(TextDiffMode::None),
        "words" => Ok(TextDiffMode::Words),
        "chars" => Ok(TextDiffMode::Chars),
        other => Err(format!(
            "diff: unknown text mode {other:?}; expected \"none\", \"words\" or \"chars\""
        )),
    }
}

/// `{"unitId": "baseline"|"current"}` — the consumer contract, parsed here.
///
/// An unknown side name is an error rather than a default: a typo silently
/// meaning "baseline" would change a document.
fn decisions(json: &str) -> Result<Decisions, String> {
    let map: std::collections::BTreeMap<String, String> =
        serde_json::from_str(json).map_err(|error| format!("decisions: {error}"))?;
    map.into_iter()
        .map(|(id, side)| match side.as_str() {
            "baseline" => Ok((id, MergeSide::Baseline)),
            "current" => Ok((id, MergeSide::Current)),
            other => Err(format!("decisions: {id} names an unknown side {other:?}")),
        })
        .collect()
}

fn default_side(name: &str) -> Result<MergeSide, String> {
    match name {
        "baseline" => Ok(MergeSide::Baseline),
        "current" => Ok(MergeSide::Current),
        other => Err(format!("unknown default side {other:?}")),
    }
}

/// The merged document. An unknown unit id REJECTS loudly — the caller must
/// re-diff, and there is no fuzzy stale-id fallback.
#[wasm_bindgen]
pub fn merge(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "LexOptions")] opts: Option<JsValue>,
) -> Result<String, JsError> {
    let markers = options::lex_options(opts.as_ref(), "merge").map_err(|e| JsError::new(&e))?;
    merged(
        baseline,
        current,
        decisions_json,
        default,
        &options::or_installed(markers),
    )
    .map_err(|error| JsError::new(&error))
}

/// `merge`'s body. Separate because `JsError` cannot be CONSTRUCTED off wasm,
/// so a native test would panic inside wasm-bindgen instead of seeing the
/// rejection it is checking for.
fn merged(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
    markers: &Extensions,
) -> Result<String, String> {
    let skeleton = diff::diff_under(baseline, current, markers);
    let merged = diff::merge(
        &skeleton,
        baseline.as_bytes(),
        current.as_bytes(),
        &decisions(decisions_json)?,
        default_side(default)?,
    )
    .map_err(|error| error.to_string())?;
    Ok(String::from_utf8_lossy(&merged).into_owned())
}

/// The same merge as replay splices over the BASELINE — the hot-path shape, for
/// an editor that would rather apply a transaction than replace a document.
///
/// `spans` is `[from, to]` per splice in BASELINE UTF-16; `inserts` is
/// `[from, to]` per splice in CURRENT UTF-16, the text to put there. An empty
/// insert is a deletion; `from == to` in `spans` is an insertion.
#[wasm_bindgen]
pub struct Splices {
    spans: Vec<u32>,
    inserts: Vec<u32>,
}

#[wasm_bindgen]
impl Splices {
    #[wasm_bindgen(getter)]
    pub fn spans(&self) -> Vec<u32> {
        self.spans.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn inserts(&self) -> Vec<u32> {
        self.inserts.clone()
    }
}

#[wasm_bindgen(js_name = mergeSplices)]
pub fn merge_splices(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "LexOptions")] opts: Option<JsValue>,
) -> Result<Splices, JsError> {
    let markers =
        options::lex_options(opts.as_ref(), "mergeSplices").map_err(|e| JsError::new(&e))?;
    splices(
        baseline,
        current,
        decisions_json,
        default,
        &options::or_installed(markers),
    )
    .map_err(|error| JsError::new(&error))
}

/// `merge_splices`' body — same reason as [`merged`].
fn splices(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
    markers: &Extensions,
) -> Result<Splices, String> {
    let skeleton = diff::diff_under(baseline, current, markers);
    let edits = diff::to_edits(
        &skeleton,
        &decisions(decisions_json)?,
        default_side(default)?,
    )
    .map_err(|error| error.to_string())?;

    let base = Utf16Index::new(baseline.as_bytes());
    let curr = Utf16Index::new(current.as_bytes());
    let mut out = Splices {
        spans: Vec::with_capacity(edits.len() * 2),
        inserts: Vec::with_capacity(edits.len() * 2),
    };
    for edit in &edits {
        out.spans
            .extend_from_slice(&[base.to_utf16(edit.from), base.to_utf16(edit.to)]);
        out.inserts.extend_from_slice(&[
            curr.to_utf16(edit.insert.start),
            curr.to_utf16(edit.insert.end),
        ]);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Offset translation for the stragglers
// ---------------------------------------------------------------------------

/// A CodeMirror offset as a source byte offset. Rebuilds the 1.6% stride index
/// per call (~0.1ms) — honest and stateless at a handful of calls per user
/// interaction, which is what a cursor-to-sid lookup is.
#[wasm_bindgen(js_name = toByte)]
pub fn to_byte(text: &str, utf16: u32) -> u32 {
    Utf16Index::new(text.as_bytes()).to_byte(utf16)
}

/// A source byte offset as a CodeMirror offset. Same deal.
#[wasm_bindgen(js_name = toUtf16)]
pub fn to_utf16(text: &str, byte: u32) -> u32 {
    Utf16Index::new(text.as_bytes()).to_utf16(byte)
}

// ---------------------------------------------------------------------------
// The engine's hash, for a host's own bytes
// ---------------------------------------------------------------------------

/// XXH3-64, seed 0, over the bytes — the hash every dish header stamps as
/// `sourceHash`, for whatever a host wants to key: a file fetched over the
/// network, a chapter slice cut at a TOC row.
///
/// ```js
/// xxh3(bytes)                          // → 0x…n, a bigint (u64)
/// xxh3Text(text) === xxh3(new TextEncoder().encode(text))
/// xxh3Text(text) === parse(text, …).sourceHash
/// ```
#[wasm_bindgen]
pub fn xxh3(bytes: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(bytes)
}

/// [`xxh3`] over a string's UTF-8, without a `TextEncoder` round trip on the
/// JS side.
#[wasm_bindgen(js_name = xxh3Text)]
pub fn xxh3_text(text: &str) -> u64 {
    xxh3(text.as_bytes())
}

// ---------------------------------------------------------------------------
// The two named readouts
// ---------------------------------------------------------------------------

/// The reference at a CodeMirror offset — `"MRK 6:3"`, `"MRK 6:1-3"` for a
/// bridge, `"MRK 6"` ahead of a chapter's first verse, `"MRK"` in front matter,
/// `"###"` when the book declares no `\id`.
///
/// TOTAL: an offset past the end names the last chapter rather than nothing,
/// because every caller of this is labelling a position it already has.
///
/// One of the two exports on the sketch's method list that the reads
/// could not supply: the sid RENDERING is a format with rules (bridges, absent
/// chapters, an unknown book), and a JS re-implementation of it in every
/// consumer is the drift the no-strings-cross rule exists to prevent.
#[wasm_bindgen]
pub fn locate(
    text: &str,
    utf16: u32,
    #[wasm_bindgen(unchecked_optional_param_type = "LexOptions")] opts: Option<JsValue>,
) -> Result<String, JsError> {
    let markers = options::lex_options(opts.as_ref(), "locate").map_err(|e| JsError::new(&e))?;
    Ok(located(text, utf16, &options::or_installed(markers)))
}

/// [`locate`]'s body, for a caller with no `JsValue`.
pub fn located(text: &str, utf16: u32, markers: &Extensions) -> String {
    let source = text.as_bytes();
    let tokens = usfm_onion::lex_with(text, markers);
    let byte = Utf16Index::new(source).to_byte(utf16);
    usfm_onion::toc::toc(source, &tokens)
        .locate(byte)
        .to_string()
}

/// Where an address is in one text, as JSON: found, missing or ambiguous,
/// never a silent nearest. The rules are `onion/src/resolve.md`, the cases
/// `testData/resolve/cases.json`, whose `expect` is this door's answer.
///
/// ```js
/// resolve(text, { kind: "verses", from: { chapter: 1, verse: 2 }, to: { chapter: 1, verse: 2 } })
///   // → '{"found":{"from":8,"to":19,"opening":5,"covers":{…},"coarser":false}}'
/// resolve(text, { kind: "chapters", from: 3, to: 3 }, { utf16: true })
///   // → '{"missing":"chapter"}'
/// ```
///
/// Offsets are bytes unless `utf16` asks otherwise. galley's
/// `Galley.resolveOf` answers the same over text, and `Galley.resolve` over a
/// registered book.
#[wasm_bindgen]
pub fn resolve(
    text: &str,
    #[wasm_bindgen(unchecked_param_type = "Address")] address: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ResolveTextOptions")] opts: Option<JsValue>,
) -> Result<String, JsError> {
    let (utf16, markers) =
        options::resolve_text_options(opts.as_ref(), "resolve").map_err(|e| JsError::new(&e))?;
    let address = resolution::address(&address).map_err(|e| JsError::new(&e))?;
    Ok(resolved(
        text,
        &address,
        utf16,
        &options::or_installed(markers),
    ))
}

/// [`resolve`]'s body over a parsed address, for a caller with no `JsValue`.
pub fn resolved(
    text: &str,
    address: &usfm_onion::resolve::Address,
    utf16: bool,
    markers: &Extensions,
) -> String {
    let tokens = usfm_onion::lex_with(text, markers);
    let answer =
        usfm_onion::toc::toc(text.as_bytes(), &tokens).resolve_with(text, address, markers);
    let table = utf16.then(|| mise::utf16::utf16_table(text.as_bytes()));
    resolution::json(&answer, table.as_ref())
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

/// The k/v view of one `AttrList` token, flat.
///
/// `[from, to)` is the list token's span in the CALLER's space — UTF-16 under
/// `{ utf16: true }`, bytes otherwise — and every word comes back in that same
/// space.
///
/// ```text
/// |lemma="grace" x-y="z"   ->  [nameFrom nameTo valueFrom valueTo] x 2, NONE, NONE
/// |grace                   ->  one quadruple, its name span EMPTY at the value
/// |lemma="grace            ->  no quadruple, then UnterminatedQuote and the quote's offset
/// ```
///
/// Four words per attribute, then two: the `MalformedAttr` code and its
/// offset, both `NONE` when the list parsed clean. A malformed tail is always
/// the last event, so it can only be the last pair of words.
#[wasm_bindgen]
pub fn attrs(
    text: &str,
    from: u32,
    to: u32,
    #[wasm_bindgen(unchecked_optional_param_type = "AttrsOptions")] opts: Option<JsValue>,
) -> Result<Vec<u32>, JsError> {
    let utf16 = options::utf16_only(opts.as_ref(), "attrs").map_err(|e| JsError::new(&e))?;
    Ok(attr_words(text, from, to, utf16))
}

/// [`attrs`]'s body, for a caller with no `JsValue` to hand.
pub fn attr_words(text: &str, from: u32, to: u32, utf16: bool) -> Vec<u32> {
    let source = text.as_bytes();
    // A byte caller pays for no index.
    let index = utf16.then(|| Utf16Index::new(source));
    let (start, end) = match &index {
        Some(index) => (index.to_byte(from), index.to_byte(to)),
        None => (from, to),
    };
    let out_space = |byte: u32| match &index {
        Some(index) => index.to_utf16(byte),
        None => byte,
    };

    // The interpreter reads a SPAN; it never needs the row a scan would have
    // built around it, so the span is handed over as one rather than re-lexed.
    debug_assert!(start <= end && (end - start) <= u32::from(u16::MAX));
    let list = Token {
        start,
        len: (end - start) as u16,
        kind_bits: TokenKind::AttrList.to_bits(),
        marker_idx: 0,
        level: 0,
    };

    let mut out = Vec::new();
    let mut malformed = [wire::NONE, wire::NONE];
    for event in attributes::attrs(source, &list) {
        match event {
            AttrEvent::Attr(attr) => out.extend_from_slice(&[
                out_space(attr.name_span.start),
                out_space(attr.name_span.end),
                out_space(attr.value_span.start),
                out_space(attr.value_span.end),
            ]),
            AttrEvent::Malformed { at, why } => malformed = [why as u32, out_space(at)],
        }
    }
    out.extend_from_slice(&malformed);
    out
}

/// One attribute name against one marker row: the `AttrResolution` code.
///
/// An empty name is the bare default-value form, which resolves through the
/// row's own default — so it is a legitimate argument, not a mistake.
#[wasm_bindgen(js_name = attrResolve)]
pub fn attr_resolve(name: &str, marker: u32) -> u32 {
    attributes::resolve(name.as_bytes(), marker as MarkerIdx).code()
}

/// The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
/// none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
/// where that becomes a finding, not here.
#[wasm_bindgen]
pub fn book(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "LexOptions")] opts: Option<JsValue>,
) -> Result<String, JsError> {
    let markers = options::lex_options(opts.as_ref(), "book").map_err(|e| JsError::new(&e))?;
    Ok(book_of(text, &options::or_installed(markers)))
}

/// [`book`]'s body, for a caller with no `JsValue`.
pub fn book_of(text: &str, markers: &Extensions) -> String {
    let source = text.as_bytes();
    let tokens = usfm_onion::lex_with(text, markers);
    let code = usfm_onion::toc::toc(source, &tokens).book;
    let end = code.iter().position(|b| *b == 0).unwrap_or(code.len());
    String::from_utf8_lossy(&code[..end]).into_owned()
}

// ---------------------------------------------------------------------------
// user `\z` markers
// ---------------------------------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const CUSTOM_MARKERS_SECTION: &str = CUSTOM_MARKERS_TS;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the section's macro hides the read")
)]
const CUSTOM_MARKERS_TS: &str = r#"
/** A `\category` word, as the USFM spec spells it. */
export type CustomMarkerCategory =
  | "header" | "title" | "introduction" | "sectionpara" | "versepara" | "list" | "otherpara"
  | "crossreference" | "footnote"
  | "char" | "introchar" | "listchar" | "footnotechar" | "crossreferencechar"
  | "milestone" | "attribute" | "cell" | "standalone" | "internal";

/** One user `\z` marker. */
export interface CustomMarker {
  name: string;
  category: CustomMarkerCategory;
  /** The `markers.ext` description; shown, never lexed by. */
  description?: string;
  /** The attribute names `markers.ext` lists; shown, never lexed by. */
  attributes?: readonly string[];
}

/** An entry a list could not keep. `line` only where a file gave one. */
export interface MalformedCustomMarker {
  name?: string;
  reason: string;
  line?: number;
}

declare const customMarkersBrand: unique symbol;

/** A judged list, from `customMarkers` or `customMarkersFromMarkersExt`. */
export type CustomMarkers = {
  /** The entries kept; every door lexes by exactly these. */
  readonly markers: readonly CustomMarker[];
  readonly relaxZPrefix: boolean;
  /** The entries refused, and why. */
  readonly malformed: readonly MalformedCustomMarker[];
} & { readonly [customMarkersBrand]: "CustomMarkers" };
"#;

/// One entry a list could not keep. `line` is the file's, and absent for a
/// list a host assembled itself.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub line: Option<u32>,
    pub name: Option<String>,
    pub reason: &'static str,
}

/// A host's list, judged: what the registry keeps, and a report for each
/// entry it refuses, the unknown category words first.
pub fn judged(entries: Vec<options::Entry>, opts: &ExtensionOptions) -> (Extensions, Vec<Report>) {
    let mut reports = Vec::new();
    let mut markers = Vec::new();
    for entry in entries {
        match ExtensionCategory::parse(&entry.category) {
            Some(category) => markers.push(CustomMarker {
                name: entry.name,
                category,
                description: entry.description,
                attributes: entry.attributes,
            }),
            None => reports.push(Report {
                line: None,
                name: Some(entry.name),
                reason: "unknown category word",
            }),
        }
    }
    let (extensions, refused) = Extensions::new_with(&markers, opts);
    reports.extend(refused.into_iter().map(|report| Report {
        line: None,
        name: report.name,
        reason: report.reason,
    }));
    (extensions, reports)
}

/// A list of user markers, judged once and held as plain data.
///
/// ```js
/// const mine = customMarkers([{ name: "zmyf", category: "footnote" }]);
/// mine.malformed;                                            // → []
/// galley.update("books/MRK.usfm", text, { customMarkers: mine });
/// parse(refText, { customMarkers: mine });
///
/// customMarkers([{ name: "s5", category: "standalone" }], { relaxZPrefix: true });
/// ```
///
/// `markers` holds the entries kept and `malformed` the ones refused: no name,
/// a name that is not `z`-initial, an unknown category word, a duplicate. A
/// bad entry costs only itself. Installs nothing.
///
/// Throws when `list` is not an array of `{ name, category }` strings (with
/// an optional string `description` and string-array `attributes`), or
/// `opts` is not `CustomMarkerOptions`.
#[wasm_bindgen(js_name = customMarkers, unchecked_return_type = "CustomMarkers")]
pub fn custom_markers(
    #[wasm_bindgen(unchecked_param_type = "readonly CustomMarker[]")] list: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "CustomMarkerOptions")] opts: Option<JsValue>,
) -> Result<JsValue, JsError> {
    let door = "customMarkers";
    let opts = options::custom_marker_options(opts.as_ref(), door).map_err(|e| JsError::new(&e))?;
    let entries = options::entries(&list, door, false).map_err(|e| JsError::new(&e))?;
    let (extensions, reports) = judged(entries, &opts);
    Ok(custom_markers_object(&extensions, &opts, &reports))
}

/// A `markers.ext` file, read into the same object [`custom_markers`] makes,
/// with `relaxZPrefix` false.
///
/// ```js
/// const mine = customMarkersFromMarkersExt(text);
/// for (const { name, reason, line } of mine.malformed) show(line, name, reason);
/// galley.update("books/MRK.usfm", text, { customMarkers: mine });
/// ```
///
/// A bad entry costs only itself and lands in `malformed` with its line; the
/// file never fails as a whole. Installs nothing.
#[wasm_bindgen(js_name = customMarkersFromMarkersExt, unchecked_return_type = "CustomMarkers")]
pub fn custom_markers_from_markers_ext(text: &str) -> JsValue {
    let (extensions, reports) = markers_ext(text);
    custom_markers_object(&extensions, &ExtensionOptions::default(), &reports)
}

/// [`custom_markers_from_markers_ext`] for a caller with no `JsValue`.
pub fn markers_ext(text: &str) -> (Extensions, Vec<Report>) {
    let read = mise::extensions::parse_markers_ext(text);
    let mut reports: Vec<Report> = read
        .malformed
        .into_iter()
        .map(|report| Report {
            line: Some(report.line),
            name: report.name,
            reason: report.reason,
        })
        .collect();
    let (extensions, refused) = Extensions::new(&read.markers);
    reports.extend(refused.into_iter().map(|report| Report {
        line: None,
        name: report.name,
        reason: report.reason,
    }));
    (extensions, reports)
}

fn custom_markers_object(
    extensions: &Extensions,
    opts: &ExtensionOptions,
    reports: &[Report],
) -> JsValue {
    let markers: js_sys::Array = extensions
        .declared()
        .iter()
        .map(|marker| {
            let entry = Object::new();
            put(&entry, "name", &JsValue::from_str(&marker.name));
            put(
                &entry,
                "category",
                &JsValue::from_str(marker.category.as_str()),
            );
            if !marker.description.is_empty() {
                put(
                    &entry,
                    "description",
                    &JsValue::from_str(&marker.description),
                );
            }
            if !marker.attributes.is_empty() {
                let attributes: js_sys::Array = marker
                    .attributes
                    .iter()
                    .map(|name| JsValue::from_str(name))
                    .collect();
                put(&entry, "attributes", &attributes);
            }
            JsValue::from(entry)
        })
        .collect();
    let malformed: js_sys::Array = reports
        .iter()
        .map(|report| {
            let entry = Object::new();
            if let Some(name) = &report.name {
                put(&entry, "name", &JsValue::from_str(name));
            }
            put(&entry, "reason", &JsValue::from_str(report.reason));
            if let Some(line) = report.line {
                put(&entry, "line", &JsValue::from(line));
            }
            JsValue::from(entry)
        })
        .collect();
    let out = Object::new();
    put(&out, "markers", &markers);
    put(
        &out,
        "relaxZPrefix",
        &JsValue::from_bool(opts.relax_z_prefix),
    );
    put(&out, "malformed", &malformed);
    out.into()
}

fn put(object: &Object, key: &str, value: &JsValue) {
    Reflect::set(object, &JsValue::from_str(key), value)
        .expect("a fresh Object always accepts a property");
}

/// Installs a list process-wide: the one every door lexes by when it is given
/// no `customMarkers`.
///
/// ```js
/// setCustomMarkers(customMarkersFromMarkersExt(text));
/// setCustomMarkers(customMarkers([]));                       // clears
/// ```
///
/// Every registered marker then behaves as its `\category` — a `footnote`
/// takes a caller and a note scope, a `milestone` pairs `-s`/`-e` and takes
/// attributes — because the engine resolves it to the spec row that category
/// behaves as. An unregistered `\z` marker stays what it has always been.
///
/// A resident `Galley` re-derives the books registered without a list; a book
/// registered with one keeps its own.
///
/// Throws where a door refuses its `customMarkers` option.
#[wasm_bindgen(js_name = setCustomMarkers)]
pub fn set_custom_markers(
    #[wasm_bindgen(unchecked_param_type = "CustomMarkers")] markers: JsValue,
) -> Result<(), JsError> {
    let (list, opts) =
        options::read_custom_markers(&markers, "setCustomMarkers").map_err(|e| JsError::new(&e))?;
    installed(list, &opts).map_err(|e| JsError::new(&e))
}

/// `set_custom_markers`'s body, split from the door for the same reason
/// [`diffed`] is: `JsError` cannot be constructed off a wasm target, so a
/// native test would have nothing to call.
fn installed(list: Vec<(String, String)>, opts: &ExtensionOptions) -> Result<(), String> {
    let extensions = options::interned("setCustomMarkers", list, opts)?;
    usfm_onion::set_extensions_with(extensions.declared(), opts);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section's byte length, through the exported header constants rather
    /// than a literal — a hardcoded offset here is a test that passes until
    /// the header grows and then asserts about the wrong bytes.
    fn section_index(name: &str) -> usize {
        wire::schema::SECTIONS
            .iter()
            .position(|s| s.name == name)
            .expect("a declared section")
    }

    fn section_len(dish: &[u8], section: usize) -> u32 {
        let at = wire::HEADER_BYTES + section * wire::DIRECTORY_ENTRY_BYTES + 4;
        u32::from_le_bytes(dish[at..at + 4].try_into().expect("four bytes"))
    }

    static NONE: std::sync::LazyLock<Extensions> = std::sync::LazyLock::new(Extensions::default);

    const BOOK: &str = "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f* .\n\\c 2\n\\p \\v 1 λόγος\n";

    fn opts(diagnostics: bool, toc: bool, utf16: bool) -> wire::ParseOptions {
        wire::ParseOptions {
            diagnostics,
            toc,
            utf16,
        }
    }

    /// The wall's own job: a string in, a readable dish out. What the dish
    /// SAYS is `onion`'s test (`tests/wire_roundtrip.rs`); this checks only
    /// that the boundary hands one over intact.
    #[test]
    fn parse_returns_a_readable_dish() {
        let dish = plated(BOOK, opts(true, true, false), &NONE);
        let word = |at: usize| u32::from_le_bytes(dish[at..at + 4].try_into().unwrap());
        assert_eq!(word(0), wire::MAGIC);
        assert_eq!(word(4), wire::FORMAT_VERSION);
        assert_eq!(word(8) as usize, wire::schema::SECTIONS.len());
        assert_eq!(word(12) & wire::FLAG_UTF16, 0, "bytes unless asked");
    }

    /// The one flag that changes every offset in the buffer.
    #[test]
    fn utf16_is_opt_in_at_the_call() {
        let bytes = plated(BOOK, opts(false, false, false), &NONE);
        let units = plated(BOOK, opts(false, false, true), &NONE);
        let flag = |d: &[u8]| u32::from_le_bytes(d[12..16].try_into().unwrap()) & wire::FLAG_UTF16;
        assert_eq!(flag(&bytes), 0);
        assert_ne!(flag(&units), 0);
        // `λόγος` is two bytes per character and one code unit, so the buffers
        // cannot be identical.
        assert_ne!(bytes, units);
    }

    /// Asking for nothing computes nothing: the optional sections are empty,
    /// and the tree is there either way because the tree is the product.
    #[test]
    fn the_optionals_are_off_by_default() {
        let dish = plated(BOOK, opts(false, false, false), &NONE);
        let len = |name: &str| section_len(&dish, section_index(name));
        assert_eq!(len("diagnostics"), 0, "no lint walk was asked for");
        assert_eq!(len("chapters"), 0, "no toc was asked for");
        assert_eq!(len("verses"), 0);
        assert!(len("tokens") > 0, "the tree is the product, always");
        assert!(len("nodes") > 0);
    }

    /// The sous door: text out, and the map that puts a finding back.
    #[test]
    fn mask_recipes_resolve() {
        let tokens = usfm_onion::lex(BOOK);
        let cst = usfm_onion::cst::build(&tokens);
        for recipe in [
            mask_recipe::Filter::verse_text(),
            mask_recipe::Filter::structure(),
        ] {
            let masked = usfm_onion::mask(BOOK.as_bytes(), &tokens, &cst, &recipe);
            assert_eq!(
                masked.starts.len(),
                masked.ranges.len(),
                "every kept range has its prefix sum"
            );
        }
    }

    #[test]
    fn format_edits_are_utf16() {
        let messy = "\\c 1\\p \\v 1 λόγος  ἦν\n";
        let edits = format_edits(messy, &FormatOpts::new());
        let index = Utf16Index::new(messy.as_bytes());
        assert!(!edits.spans().is_empty());
        for span in edits.spans().chunks_exact(2) {
            assert!(span[0] <= span[1] && span[1] <= index.len_utf16());
        }
        assert_eq!(edits.lens().len() * 2, edits.spans().len());
        assert_eq!(
            edits.lens().iter().sum::<u32>() as usize,
            edits.text().len()
        );
    }

    /// The range crosses in UTF-16 and is translated on the SAME index the
    /// spans go out through — multibyte text or not.
    #[test]
    fn format_edits_in_takes_its_range_in_utf16() {
        let messy = "\\c 1\n\\p\n\\v 1 λόγος  ἦν\n\\c 2\n\\p\n\\v 1 a\n\\v 2 b\n";
        let opts = FormatOpts::new();
        let index = Utf16Index::new(messy.as_bytes());
        let whole = format_edits(messy, &opts);
        assert_eq!(
            format_edits_in(messy, 0, index.len_utf16(), &opts).spans(),
            whole.spans(),
            "the full range is the whole book"
        );

        // Chapter 2 opens at the `\c 2` byte; in UTF-16 that is 10 units earlier
        // than the byte offset (`λόγος  ἦν` is 2 bytes per Greek letter).
        let at = messy.find("\\c 2").unwrap() as u32;
        let from = index.to_utf16(at);
        assert!(from < at, "the Greek made the wall matter");
        let ranged = format_edits_in(messy, from, index.len_utf16(), &opts);
        assert!(!ranged.spans().is_empty());
        assert!(ranged.spans().len() < whole.spans().len());
        assert!(ranged.spans().chunks_exact(2).all(|span| span[0] >= from));
    }

    #[test]
    fn format_applies_the_switches() {
        let mut opts = FormatOpts::new();
        opts.set_remove_markers("s5");
        assert!(!format("\\c 1\n\\s5\n\\p \\v 1 a\n", &opts).contains("\\s5"));
    }

    #[test]
    fn the_diff_wire_is_camel_case_json_with_utf16_spans() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 λόγος one\n\\v 2 two\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 λόγος one\n\\v 2 TWO\n";
        let json = diffed(baseline, current, diff_opts(TextDiffMode::None, false)).unwrap();
        assert!(json.contains("\"unitId\""), "{json}");
        assert!(!json.contains("\"text\""), "none computes no runs: {json}");
        assert!(json.contains("\"isWhitespaceChange\""));
        assert!(json.contains("\"baselineSid\""));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let units = parsed["units"].as_array().unwrap();
        let changed = units
            .iter()
            .find(|unit| unit["status"] == "modified")
            .expect("verse 2 changed");
        let from = changed["current"][0].as_u64().unwrap() as u32;
        let index = Utf16Index::new(current.as_bytes());
        // The span is UTF-16: the Greek ahead of it makes it shorter than the
        // byte offset it came from.
        assert_eq!(
            index.to_byte(from) as usize,
            current.rfind("\\v 2").unwrap()
        );
        assert!(from < index.to_byte(from));
    }

    /// The runs ride beside the units, only where a highlight belongs, and an
    /// unchanged unit stays absent so a whole-Bible skeleton does not carry a
    /// null per verse.
    #[test]
    fn text_mode_runs_only_where_a_highlight_belongs() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 the \\w quick\\w* fox\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 the \\w slow\\w* fox\n";
        let parsed: serde_json::Value = serde_json::from_str(
            &diffed(baseline, current, diff_opts(TextDiffMode::Words, true)).unwrap(),
        )
        .unwrap();
        let units = parsed["units"].as_array().unwrap();
        for unit in units.iter().filter(|unit| unit["status"] == "unchanged") {
            assert!(unit.get("text").is_none(), "{unit}");
        }
        let text = &units
            .iter()
            .find(|unit| unit["status"] == "modified")
            .expect("verse 2 changed")["text"];
        // Spans are UTF-16 offsets; these test strings are pure ASCII, so byte
        // slicing the source lines up with them.
        let runs = |side: &str, key: &str, value: &str| {
            let source = if side == "baseline" {
                baseline
            } else {
                current
            };
            text[side]
                .as_array()
                .unwrap()
                .iter()
                .filter(|run| run[key] == value)
                .map(|run| {
                    let (from, to) = (
                        run["from"].as_u64().unwrap() as usize,
                        run["to"].as_u64().unwrap() as usize,
                    );
                    source[from..to].to_string()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(runs("baseline", "kind", "removed"), ["quick"]);
        assert_eq!(runs("current", "kind", "added"), ["slow"]);
        assert!(
            runs("baseline", "kind", "unchanged")
                .concat()
                .contains("the")
        );
        // The `\w` wrapper is not in the reading, but it is not lost either:
        // it is markup, spanned, and unchanged on both sides.
        assert_eq!(
            runs("baseline", "what", "markup"),
            ["\\v 2 ", "\\w ", "\\w*"]
        );
        assert_eq!(
            runs("baseline", "what", "markup"),
            runs("current", "what", "markup")
        );
    }

    /// A typo must not silently mean `"none"` — the editor would lose its
    /// highlighting with nothing to see.
    #[test]
    fn an_unknown_text_mode_rejects() {
        assert!(mode("wrods").unwrap_err().contains("\"wrods\""));
        assert_eq!(mode("chars"), Ok(TextDiffMode::Chars));
    }

    fn diff_opts(text: TextDiffMode, unchanged: bool) -> DiffOptions {
        DiffOptions {
            text,
            unchanged,
            markers: Arc::new(Extensions::default()),
        }
    }

    /// The example on [`WireSkeleton`], held.
    #[test]
    fn a_changed_only_diff_sends_the_changed_units_and_counts_the_rest() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 3 three\n";
        let parsed: serde_json::Value = serde_json::from_str(
            &diffed(baseline, current, diff_opts(TextDiffMode::None, false)).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed["unchangedCount"], 4);
        assert!(parsed.get("slots").is_none());
        let units = parsed["units"].as_array().unwrap();
        assert_eq!(units.len(), 1);
        let deleted = &units[0];
        assert_eq!(deleted["unitId"], "GEN 1:2");
        assert_eq!(deleted["status"], "deleted");
        assert_eq!(deleted["baseline"], serde_json::json!([22, 31]));
        assert_eq!(&baseline[22..31], "\\v 2 two\n");
        assert_eq!(deleted["current"], serde_json::json!([22, 22]));
        assert!(deleted["currentSid"].is_null());
        assert!(current[22..].starts_with("\\v 3"));
    }

    /// A rebridged pair whose current half follows an added verse: the pair's
    /// FIRST slot is its baseline half, ahead of the addition, but in the
    /// current the addition comes first, and so it does on the wire.
    #[test]
    fn units_follow_the_current_document_past_a_rebridged_pair() {
        let baseline = "\\id GEN\n\\c 1\n\\v 2-3 two three\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 two three\n";
        let json = diffed(baseline, current, diff_opts(TextDiffMode::None, false)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let units: Vec<String> = parsed["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| {
                format!(
                    "{} {}",
                    unit["unitId"].as_str().unwrap(),
                    unit["status"].as_str().unwrap()
                )
            })
            .collect();
        assert_eq!(units, ["GEN 1:1 added", "GEN 1:2 modified"]);
    }

    /// `unchanged: true` is the same units, with the unchanged ones in their
    /// places: nothing sent in changed-only differs from its full-mode twin.
    #[test]
    fn the_full_diff_is_the_changed_one_with_the_unchanged_units_in_place() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 one\n\\v 2 two\n\\v 3 three\n\\v 4 four\n";
        let current =
            "\\id GEN\n\\c 1\n\\v 1 ONE\n\\v 3 three\n\\v 2 two\n\\v 4 four\n\\v 5 five\n";
        let units = |unchanged: bool| {
            let json =
                diffed(baseline, current, diff_opts(TextDiffMode::Words, unchanged)).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            parsed["units"].as_array().unwrap().clone()
        };
        let (full, changed) = (units(true), units(false));
        let kept: Vec<_> = full
            .iter()
            .filter(|unit| unit["status"] != "unchanged")
            .cloned()
            .collect();
        assert_eq!(kept, changed);
        // The current document's order: every span, points included, starts
        // at or after the last one.
        let starts: Vec<u64> = full
            .iter()
            .map(|unit| unit["current"][0].as_u64().unwrap())
            .collect();
        assert!(starts.is_sorted(), "{starts:?}");
        // The swap is a deletion and an addition, never a third kind.
        let statuses: Vec<&str> = changed
            .iter()
            .map(|unit| unit["status"].as_str().unwrap())
            .collect();
        assert!(
            statuses
                .iter()
                .all(|s| ["modified", "added", "deleted"].contains(s))
        );
    }

    #[test]
    fn a_merge_decision_is_honoured_and_a_stale_id_rejected() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 one\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 ONE\n";
        assert_eq!(
            merged(baseline, current, "{}", "current", &NONE).unwrap(),
            current
        );
        assert_eq!(
            merged(baseline, current, "{}", "baseline", &NONE).unwrap(),
            baseline
        );
        assert!(
            merged(
                baseline,
                current,
                "{\"GEN 9:9\":\"baseline\"}",
                "current",
                &NONE
            )
            .is_err()
        );
        assert!(merged(baseline, current, "{\"x\":\"sideways\"}", "current", &NONE).is_err());
        assert!(merged(baseline, current, "not json", "current", &NONE).is_err());
        assert!(merged(baseline, current, "{}", "middle", &NONE).is_err());
    }

    #[test]
    fn splices_replay_the_merge() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 λόγος\n\\v 2 two\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 λόγος\n\\v 2 TWO\n";
        let splices = super::splices(baseline, current, "{}", "current", &NONE).unwrap();
        assert_eq!(splices.spans().len(), splices.inserts().len());
        assert!(!splices.spans().is_empty(), "one verse changed");
        // Replayed by hand, in UTF-16 space, exactly as the editor would.
        let base: Vec<u16> = baseline.encode_utf16().collect();
        let curr: Vec<u16> = current.encode_utf16().collect();
        let mut out: Vec<u16> = Vec::new();
        let mut at = 0usize;
        for (span, insert) in splices
            .spans()
            .chunks_exact(2)
            .zip(splices.inserts().chunks_exact(2))
        {
            out.extend_from_slice(&base[at..span[0] as usize]);
            out.extend_from_slice(&curr[insert[0] as usize..insert[1] as usize]);
            at = span[1] as usize;
        }
        out.extend_from_slice(&base[at..]);
        assert_eq!(String::from_utf16(&out).unwrap(), current);
    }

    #[test]
    fn the_offset_stragglers_round_trip() {
        let text = "\\v 1 λόγος ἦν";
        assert_eq!(to_utf16(text, 16), 11);
        assert_eq!(to_byte(text, 11), 16);
    }

    #[test]
    fn locate_and_book_answer_in_editor_offsets() {
        // Greek ahead of the verse, so a byte offset would land elsewhere.
        let text = "\\id MRK\n\\c 6\n\\p \\v 1 λόγος\n\\v 2-4 bridged\n";
        assert_eq!(book_of(text, &NONE), "MRK");
        assert_eq!(located(text, 0, &NONE), "MRK");
        assert_eq!(located(text, 8, &NONE), "MRK 6");
        let inside = text.find("λόγος").unwrap() + "λόγος".len();
        let utf16 = to_utf16(text, inside as u32);
        assert!(utf16 < inside as u32, "the offsets have drifted apart");
        assert_eq!(located(text, utf16, &NONE), "MRK 6:1");
        assert_eq!(
            located(text, to_utf16(text, text.len() as u32 - 2), &NONE),
            "MRK 6:2-4"
        );
        // Total past the end, and honest about a book with no `\id`.
        assert_eq!(located(text, u32::MAX, &NONE), "MRK 6:2-4");
        assert_eq!(book_of("\\c 1\n", &NONE), "");
        assert_eq!(located("\\c 1\n\\v 1 a\n", 8, &NONE), "### 1:1");
    }

    /// The `\usfm` version is a lint fact now, not a header field on a read:
    /// it reaches a consumer through the catalog's severity ladder, which is
    /// where it was always used.
    #[test]
    fn the_tree_is_there_without_asking() {
        let dish = plated(BOOK, opts(false, false, false), &NONE);
        let len = |name: &str| section_len(&dish, section_index(name));
        assert!(len("tokens") > 0);
        assert!(len("nodes") > 0);
        assert!(len("childIds") > 0);
    }
    fn entries(list: &[(&str, &str)]) -> Vec<options::Entry> {
        pairs(list).into_iter().map(options::Entry::from).collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(name, category)| ((*name).to_owned(), (*category).to_owned()))
            .collect()
    }

    /// The `markers.ext` door reads and installs NOTHING, keeps only what the
    /// registry keeps, and its file reports carry the line the file gave them.
    #[test]
    fn markers_ext_reads_without_installing() {
        let (read, reports) = markers_ext(
            "\\marker zmyf\n\\category footnote\n\\marker bad\n\\category char\n\\marker zmyf\n\\category char\n",
        );
        assert_eq!(read.declared().len(), 1);
        assert_eq!(read.declared()[0].name, "zmyf");
        assert_eq!(read.declared()[0].category, ExtensionCategory::Footnote);
        assert_eq!(reports[0].line, Some(3));
        assert_eq!(reports[0].reason, "name does not start with z");
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(
            reports[1].reason,
            "duplicate name; the first definition stands"
        );
        assert_eq!(
            usfm_onion::lex("\\zmyf x")[0].marker_idx,
            0,
            "nothing installed"
        );
    }

    /// The `.d.ts` category union is the spec's word list, whole.
    #[test]
    fn the_category_union_is_every_word() {
        let union = CUSTOM_MARKERS_TS
            .split("export type CustomMarkerCategory =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap();
        let words: Vec<&str> = union.split('"').skip(1).step_by(2).collect();
        let spec: Vec<&str> = ExtensionCategory::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(words, spec);
    }

    /// A bad entry is a report and costs only itself.
    #[test]
    fn a_list_is_judged_entry_by_entry() {
        let (kept, reports) = judged(
            entries(&[("znope", "bogus"), ("nope", "char"), ("zok", "versepara")]),
            &ExtensionOptions::default(),
        );
        assert_eq!(kept.declared().len(), 1);
        assert_eq!(kept.declared()[0].name, "zok");
        assert_eq!(
            reports,
            [
                Report {
                    line: None,
                    name: Some("znope".into()),
                    reason: "unknown category word"
                },
                Report {
                    line: None,
                    name: Some("nope".into()),
                    reason: "name does not start with z"
                },
            ]
        );
        let legacy = entries(&[("s5", "standalone"), ("s1", "standalone")]);
        let (kept, reports) = judged(
            legacy,
            &ExtensionOptions {
                relax_z_prefix: true,
            },
        );
        assert_eq!(kept.declared()[0].name, "s5");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].name.as_deref(), Some("s1"));
    }

    /// A list handed to a door lexes by itself, installs nothing, and is
    /// resolved once however often it arrives.
    #[test]
    fn a_door_list_lexes_by_itself_and_is_interned() {
        let mine = options::interned(
            "test",
            pairs(&[("zmyf", "footnote"), ("zmyc", "char")]),
            &ExtensionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            usfm_onion::lex("\\zmyf x")[0].marker_idx,
            0,
            "nothing installed"
        );
        let reordered = options::interned(
            "test",
            pairs(&[("zmyc", "char"), ("zmyf", "footnote")]),
            &ExtensionOptions::default(),
        )
        .unwrap();
        assert!(
            Arc::ptr_eq(&mine, &reordered),
            "an equal list is the same registry"
        );
        let relaxed = options::interned(
            "test",
            pairs(&[("zmyf", "footnote"), ("zmyc", "char")]),
            &ExtensionOptions {
                relax_z_prefix: true,
            },
        )
        .unwrap();
        assert_eq!(relaxed.fingerprint(), mine.fingerprint());

        let text = "\\id GEN\n\\c 1\n\\p \\v 1 Jesus wept\\zmyf + \\ft why\\zmyf*.\n";
        let verse_text = |markers: &Extensions| {
            let tokens = usfm_onion::lex_with(text, markers);
            let cst = usfm_onion::cst::build(&tokens);
            usfm_onion::mask(
                text.as_bytes(),
                &tokens,
                &cst,
                &mask_recipe::Filter::verse_text(),
            )
            .text(text.as_bytes())
        };
        assert!(verse_text(&NONE).contains("why"));
        assert!(!verse_text(&mine).contains("why"), "the note drops out");
        assert_ne!(
            plated(text, opts(false, false, false), &mine),
            plated(text, opts(false, false, false), &NONE)
        );
        let chapter = usfm_onion::resolve::Address::Chapters { from: 1, to: 1 };
        assert_eq!(
            resolved(text, &chapter, false, &mine),
            resolved(text, &chapter, false, &NONE),
            "a chapter's place does not move"
        );

        let refused = options::interned(
            "mask",
            pairs(&[("zmyf", "footnote"), ("nope", "char")]),
            &ExtensionOptions::default(),
        )
        .unwrap_err();
        assert_eq!(
            refused,
            "mask: customMarkers: \"nope\": name does not start with z"
        );
    }

    /// Same-position edits cross as one, and the text is the same either way.
    #[test]
    fn edits_fold_to_one_per_position() {
        let edits = Edits::from_parts(
            vec![2, 4, 4, 4, 4, 4, 9, 9],
            vec![1, 2, 3, 1],
            "abbcccd".to_owned(),
        );
        assert_eq!(edits.spans, [2, 4, 9, 9]);
        assert_eq!(edits.lens, [6, 1]);
        assert_eq!(edits.text, "abbcccd");

        // A fix longer than one `FixStr` arrives split and crosses whole.
        let long = usfm_onion::Edit {
            from: 0,
            to: 0,
            insert: usfm_onion::edit::FixStr::new(b"0123456789abcde"),
        };
        let tail = usfm_onion::Edit {
            from: 0,
            to: 0,
            insert: usfm_onion::edit::FixStr::new(b"XYZ"),
        };
        let crossed = wire_edits(&Utf16Index::new(b"text"), &[long, tail]);
        assert_eq!(crossed.spans, [0, 0]);
        assert_eq!(crossed.lens, [18]);
    }

    /// The free hash is the header's: same function, same seed, same bytes.
    #[test]
    fn xxh3_is_the_dish_source_hash() {
        let text = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning\n";
        let dish = wire::plate(&wire::parse(text, wire::ParseOptions::default()));
        let stamped = u64::from_le_bytes(dish[24..32].try_into().unwrap());
        assert_eq!(xxh3_text(text), stamped);
        assert_eq!(xxh3(text.as_bytes()), stamped);
        // Pinned, so a dependency bump that changed the function is loud.
        assert_eq!(xxh3(b""), 0x2d06_8005_38d3_94c2);
    }

    /// Installing changes what the same bytes mean; a list the registry would
    /// not keep whole installs nothing.
    #[test]
    fn set_custom_markers_installs_a_whole_list() {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = installed(Vec::new(), &ExtensionOptions::default());
            }
        }
        let _restore = Restore;
        let strict = ExtensionOptions::default();

        installed(pairs(&[("zaln", "milestone")]), &strict).unwrap();
        let idx = usfm_onion::lex("\\zaln-s |x=\"1\"\\*")[0].marker_idx;
        assert!(
            usfm_onion::tables::generated::is_extension(idx),
            "`\\zaln-s` resolves to the milestone template"
        );

        assert!(installed(pairs(&[("zok", "versepara"), ("nope", "char")]), &strict).is_err());
        assert_eq!(
            usfm_onion::lex("\\zok x")[0].marker_idx,
            0,
            "nothing installed"
        );
        assert!(usfm_onion::tables::generated::is_extension(
            usfm_onion::lex("\\zaln-s |x=\"1\"\\*")[0].marker_idx
        ));

        let legacy = pairs(&[("s5", "standalone")]);
        assert!(installed(legacy.clone(), &strict).is_err());
        installed(
            legacy,
            &ExtensionOptions {
                relax_z_prefix: true,
            },
        )
        .unwrap();
        assert!(usfm_onion::tables::generated::is_extension(
            usfm_onion::lex("\\s5\n")[0].marker_idx
        ));

        installed(Vec::new(), &strict).unwrap();
        assert_eq!(usfm_onion::lex("\\s5\n")[0].marker_idx, 0, "cleared");
    }
}
