//! `onion-wasm` — the JS doorway over the USFM engine (`onion`).
//!
//! This is **piece 2 of the five-crate layout** (onion, ONION-WASM, sous,
//! sous-wasm, galley — see `planning/ideas/committed/galley.md`): bindgen,
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
//! - **Stateless.** Text in, numbers and strings out, nothing retained between
//!   calls. The caller pairs each result with the document version it sent;
//!   stale = discard.
//! - **`analyze` returns a PLAIN JS OBJECT.** Every read is built eagerly, so
//!   nothing wasm-side outlives the call and there is nothing to free. The
//!   write path still hands out handles (`FormatOpts`, `Edits`, `Splices`);
//!   those are freed explicitly, with a `--weak-refs` build as the backstop.
//! - **Nothing rich crosses.** Flat `Uint32Array`s (copies, never views into
//!   wasm memory) and a few strings. JS never holds a token. The one structured
//!   export is the diff skeleton, which is a cold modal-open path.
//! - **UTF-16 offsets, LF-canonical input.** Every offset out is a CodeMirror
//!   code-unit offset. That conversion assumes the text is LF-normalized —
//!   CodeMirror counts a line break as ONE position, while a literal `\r\n` is
//!   TWO UTF-16 code units, so CRLF input yields offsets one ahead of the
//!   editor's from the first line onward. The vision canonicalizes at ingress
//!   (§6.3), so LF-in is the contract; `analyze` debug-asserts it and never
//!   repairs it.
//! - **The marker registry never crosses.** Marker names are bytes the editor
//!   already has (`doc.sliceString`); the coarse rendering class rides packed
//!   in the spans. Lint codes index `diagnostics.json`, codegen'd from the same
//!   rows and shipped in this package.
//!
//! # What is deliberately absent
//!
//! A **checksum** export. Vision §13.4 puts the canonical-source checksum on
//! this facade; Will deferred it (2026-08-24) as a higher-level concern — it
//! lands in `galley`, which is where save/dirty machinery will live. There is
//! no hashing dependency here, on purpose.

use js_sys::{Object, Reflect, Uint32Array};
use serde::Serialize;
use usfm_onion_2::analyze::{self, wants as want_bits};
use usfm_onion_2::diff::{
    self, Addr, CoveredSide, Decisions, DiffSkeleton, MergeSide, SlotRole, Status, UnitKind,
};
use usfm_onion_2::format::{CharBreaks, FormatOptions, Newline, VerseBreaks};
use usfm_onion_2::lint::Code;
use usfm_onion_2::utf16::Utf16Index;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// analyze
// ---------------------------------------------------------------------------

/// One analysis as a PLAIN JS OBJECT — every read built eagerly, nothing
/// wasm-side left alive, nothing for the caller to free.
///
/// Every key is always present; a read whose `wants` bit is clear is an empty
/// array. The field names and the stride schema live in `onion-wasm.ts`
/// (`RawAnalysis`), which ships in this package and is versioned with this
/// binary. Nothing here is meant to be indexed by hand.
fn object(a: &analyze::Analysis) -> Object {
    let out = Object::new();
    let set = |key: &str, value: JsValue| {
        Reflect::set(&out, &JsValue::from_str(key), &value)
            .expect("a fresh Object always accepts a property");
    };
    let array = |rows: &[u32]| JsValue::from(Uint32Array::from(rows));

    set("lenUtf16", JsValue::from_f64(f64::from(a.len_utf16)));
    // The `\usfm` line's version as a ladder index (0 = 3.0, 1 = 3.2,
    // 2 = 4.0), or `NONE`. The severity ladder in `diagnostics.json` gates
    // several codes on it, so without it a consumer re-reads the header.
    set("usfmVersion", JsValue::from_f64(f64::from(a.usfm_version)));
    set("chapters", array(&a.chapters));
    set("blocks", array(&a.blocks));
    set("lines", array(&a.lines));
    set("noteExtents", array(&a.note_extents));
    set("noteParts", array(&a.note_parts));
    set("tokenSpans", array(&a.token_spans));
    set("textRuns", array(&a.text_runs));
    set("verseAnchors", array(&a.verse_anchors));
    set("diagnostics", array(&a.diagnostics));
    set("fixes", array(&a.fixes));
    set("fixEdits", array(&a.fix_edits));
    set("fixLens", array(&a.fix_lens));
    set("fixText", JsValue::from_str(&a.fix_text));
    out
}

/// The one read call. `wants` is the bitmask in `onion-wasm.ts`; an unset bit
/// computes nothing and returns an empty array.
///
/// `clipFrom`/`clipTo` are UTF-16 offsets and bound ONLY the token-granularity
/// reads (`tokenSpans`, `textRuns`) to a viewport — chapters, blocks and
/// diagnostics stay whole-book, because a finding's evidence is regularly
/// outside the viewport that shows it. Pass `undefined` for both to skip.
///
/// `text` must be LF-normalized (see the module doc); a debug build asserts it.
#[wasm_bindgen]
pub fn analyze(text: &str, wants: u32, clip_from: Option<u32>, clip_to: Option<u32>) -> Object {
    object(&reads(text, wants, clip_from, clip_to))
}

/// `analyze`'s body, still in Rust types. Separate because `js_sys` values
/// cannot be built off wasm, so a native test would panic inside the shim
/// instead of reading the numbers it is checking.
fn reads(
    text: &str,
    wants: u32,
    clip_from: Option<u32>,
    clip_to: Option<u32>,
) -> analyze::Analysis {
    debug_assert!(
        !text.as_bytes().contains(&b'\r'),
        "onion_wasm::analyze: the text must be LF-normalized — a CR makes every \
         emitted offset disagree with CodeMirror's"
    );
    let clip = match (clip_from, clip_to) {
        (Some(from), Some(to)) => Some(from..to),
        _ => None,
    };
    analyze::analyze(text, wants, clip)
}

/// `wants::ALL` — every read. Exported so a caller that wants everything does
/// not restate the bitmask.
#[wasm_bindgen(js_name = wantsAll)]
pub fn wants_all() -> u32 {
    want_bits::ALL
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
        }
    }

    fn names(&self) -> Vec<&str> {
        self.remove_markers.iter().map(String::as_str).collect()
    }

    fn codes(&self) -> Vec<Code> {
        self.repairs
            .iter()
            .filter_map(|code| {
                usfm_onion_2::lint::LINT_ROWS
                    .get(*code as usize)
                    .map(|row| row.code)
            })
            .collect()
    }
}

/// One transaction of proposed splices: `[from, to]` pairs in UTF-16, one
/// concatenated ASCII insert blob, one byte length per edit.
///
/// The same shape a fix crosses in — an editor session applies both the same
/// way, and `lens[i] == 0` is a pure deletion.
#[wasm_bindgen]
pub struct Edits {
    spans: Vec<u32>,
    lens: Vec<u32>,
    text: String,
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
    let edits = usfm_onion_2::format_edits(text.as_bytes(), &opts.to_options(&names, &codes));
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
        usfm_onion_2::format_edits_in(text.as_bytes(), range, &opts.to_options(&names, &codes));
    wire_edits(&index, &edits)
}

/// The edit wire: spans converted to UTF-16, inserts concatenated.
///
/// The index is built per call: an edit list is small and the conversion is
/// random-access (edits are sorted, but a stride index is ~0.1ms and this is a
/// user-triggered path, not a keystroke one).
fn wire_edits(index: &Utf16Index, edits: &[usfm_onion_2::Edit]) -> Edits {
    let mut out = Edits {
        spans: Vec::with_capacity(edits.len() * 2),
        lens: Vec::with_capacity(edits.len()),
        text: String::new(),
    };
    for edit in edits {
        out.spans
            .extend_from_slice(&[index.to_utf16(edit.from), index.to_utf16(edit.to)]);
        out.lens.push(edit.insert.as_bytes().len() as u32);
        out.text.push_str(edit.insert.as_str());
    }
    out
}

/// The formatted document. `format_edits` applied, in one call.
#[wasm_bindgen]
pub fn format(text: &str, opts: &FormatOpts) -> String {
    let names = opts.names();
    let codes = opts.codes();
    let bytes = usfm_onion_2::format::format(text.as_bytes(), &opts.to_options(&names, &codes));
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// diff — the one rich structure
// ---------------------------------------------------------------------------

/// RULED (sketches/wasm-analyze.md, 2026-08-24): the skeleton crosses as serde
/// JSON. Cold path (a modal opens), zero drift, and the old editors' camelCase
/// contract back nearly verbatim. The unit ids are rendered in RUST — a JS-side
/// id renderer is the one place identity could drift, and it is rejected.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSkeleton {
    baseline_len: u32,
    current_len: u32,
    units: Vec<WireUnit>,
    slots: Vec<WireSlot>,
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
    /// UTF-16 spans of each side's own document. Empty (`from == to`) is the
    /// absent side.
    baseline: [u32; 2],
    current: [u32; 2],
    displaced: bool,
    relabeled: bool,
    baseline_count: u32,
    current_count: u32,
    is_dup: bool,
    covered_by: Option<WireCoveredBy>,
    is_whitespace_change: bool,
    is_usfm_structure_change: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireCoveredBy {
    unit: u32,
    sid: String,
    side: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSlot {
    unit: u32,
    role: &'static str,
    after_unit: Option<u32>,
    after_side: Option<&'static str>,
}

fn side_name(side: MergeSide) -> &'static str {
    match side {
        MergeSide::Baseline => "baseline",
        MergeSide::Current => "current",
    }
}

fn wire(
    skeleton: &DiffSkeleton,
    baseline: &Utf16Index<'_>,
    current: &Utf16Index<'_>,
) -> WireSkeleton {
    let sid = |addr: &Option<Addr>| addr.map(|addr| addr.to_string());
    WireSkeleton {
        baseline_len: baseline.len_utf16(),
        current_len: current.len_utf16(),
        units: skeleton
            .units
            .iter()
            .map(|unit| WireUnit {
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
                    Status::Moved => "moved",
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
                displaced: unit.displaced,
                relabeled: unit.relabeled,
                baseline_count: unit.dup_context.baseline_count,
                current_count: unit.dup_context.current_count,
                is_dup: unit.dup_context.is_dup(),
                covered_by: unit.covered_by.map(|covered| WireCoveredBy {
                    unit: covered.unit,
                    sid: covered.addr.to_string(),
                    side: match covered.side {
                        CoveredSide::Baseline => "baseline",
                        CoveredSide::Current => "current",
                    },
                }),
                is_whitespace_change: unit.is_whitespace_change,
                is_usfm_structure_change: unit.is_usfm_structure_change,
            })
            .collect(),
        slots: skeleton
            .slots
            .iter()
            .map(|slot| WireSlot {
                unit: slot.unit,
                role: match slot.role {
                    SlotRole::Shared => "shared",
                    SlotRole::BaselineOnly => "baselineOnly",
                    SlotRole::CurrentOnly => "currentOnly",
                    SlotRole::PairBaseline => "pairBaseline",
                    SlotRole::PairCurrent => "pairCurrent",
                },
                after_unit: slot.after.map(|anchor| anchor.unit),
                after_side: slot.after.map(|anchor| side_name(anchor.side)),
            })
            .collect(),
    }
}

/// The diff skeleton as JSON. Spans are UTF-16 offsets into each side's own
/// document.
#[wasm_bindgen]
pub fn diff(baseline: &str, current: &str) -> String {
    let skeleton = diff::diff(baseline, current);
    let wire = wire(
        &skeleton,
        &Utf16Index::new(baseline.as_bytes()),
        &Utf16Index::new(current.as_bytes()),
    );
    serde_json::to_string(&wire).expect("the wire structs are always serializable")
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
) -> Result<String, JsError> {
    merged(baseline, current, decisions_json, default).map_err(|error| JsError::new(&error))
}

/// `merge`'s body. Separate because `JsError` cannot be CONSTRUCTED off wasm,
/// so a native test would panic inside wasm-bindgen instead of seeing the
/// rejection it is checking for.
fn merged(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
) -> Result<String, String> {
    let skeleton = diff::diff(baseline, current);
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
) -> Result<Splices, JsError> {
    splices(baseline, current, decisions_json, default).map_err(|error| JsError::new(&error))
}

/// `merge_splices`' body — same reason as [`merged`].
fn splices(
    baseline: &str,
    current: &str,
    decisions_json: &str,
    default: &str,
) -> Result<Splices, String> {
    let skeleton = diff::diff(baseline, current);
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
pub fn locate(text: &str, utf16: u32) -> String {
    let source = text.as_bytes();
    let tokens = usfm_onion_2::lex(text);
    let byte = Utf16Index::new(source).to_byte(utf16);
    usfm_onion_2::toc::toc(source, &tokens)
        .locate(byte)
        .to_string()
}

/// The first `\id`'s book code — `"GEN"`. EMPTY when the document declares
/// none (real in the wild: BSB Ecclesiastes); the `missing-id` diagnostic is
/// where that becomes a finding, not here.
#[wasm_bindgen]
pub fn book(text: &str) -> String {
    let source = text.as_bytes();
    let tokens = usfm_onion_2::lex(text);
    let code = usfm_onion_2::toc::toc(source, &tokens).book;
    let end = code.iter().position(|b| *b == 0).unwrap_or(code.len());
    String::from_utf8_lossy(&code[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOK: &str = "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f* .\n\\c 2\n\\p \\v 1 λόγος\n";

    #[test]
    fn the_reads_come_out_of_one_call() {
        let a = reads(BOOK, wants_all(), None, None);
        assert_eq!(a.chapters.len(), 3 * 7, "front matter plus two chapters");
        assert_eq!(
            a.len_utf16,
            BOOK.chars().map(char::len_utf16).sum::<usize>() as u32
        );
    }

    #[test]
    fn a_clip_needs_both_ends() {
        let whole = reads(BOOK, wants_all(), None, None);
        assert_eq!(
            reads(BOOK, wants_all(), Some(0), None).token_spans.len(),
            whole.token_spans.len()
        );
        assert!(
            reads(BOOK, wants_all(), Some(0), Some(8)).token_spans.len() < whole.token_spans.len()
        );
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
        let json = diff(baseline, current);
        assert!(json.contains("\"unitId\""), "{json}");
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

    #[test]
    fn a_merge_decision_is_honoured_and_a_stale_id_rejected() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 one\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 ONE\n";
        assert_eq!(merged(baseline, current, "{}", "current").unwrap(), current);
        assert_eq!(
            merged(baseline, current, "{}", "baseline").unwrap(),
            baseline
        );
        assert!(merged(baseline, current, "{\"GEN 9:9\":\"baseline\"}", "current").is_err());
        assert!(merged(baseline, current, "{\"x\":\"sideways\"}", "current").is_err());
        assert!(merged(baseline, current, "not json", "current").is_err());
        assert!(merged(baseline, current, "{}", "middle").is_err());
    }

    #[test]
    fn splices_replay_the_merge() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 λόγος\n\\v 2 two\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 λόγος\n\\v 2 TWO\n";
        let splices = super::splices(baseline, current, "{}", "current").unwrap();
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
        assert_eq!(book(text), "MRK");
        assert_eq!(locate(text, 0), "MRK");
        assert_eq!(locate(text, 8), "MRK 6");
        let inside = text.find("λόγος").unwrap() + "λόγος".len();
        let utf16 = to_utf16(text, inside as u32);
        assert!(utf16 < inside as u32, "the offsets have drifted apart");
        assert_eq!(locate(text, utf16), "MRK 6:1");
        assert_eq!(
            locate(text, to_utf16(text, text.len() as u32 - 2)),
            "MRK 6:2-4"
        );
        // Total past the end, and honest about a book with no `\id`.
        assert_eq!(locate(text, u32::MAX), "MRK 6:2-4");
        assert_eq!(book("\\c 1\n"), "");
        assert_eq!(locate("\\c 1\n\\v 1 a\n", 8), "### 1:1");
    }

    #[test]
    fn the_new_reads_come_out_of_the_same_call() {
        let a = reads(BOOK, wants_all(), None, None);
        assert_eq!(a.lines.len(), 5 * 4, "five marked lines, stride 4");
        assert!(!a.note_parts.is_empty(), "the footnote's interior");
        assert_eq!(a.usfm_version, u32::MAX, "no `\\usfm` line");
        assert_eq!(
            reads("\\id GEN\n\\usfm 3.2\n", wants_all(), None, None).usfm_version,
            1
        );
    }
}
