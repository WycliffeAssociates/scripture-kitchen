//! The JS doorway over the workflows layer — the stateful half of the wall.
//!
//! ```text
//! import init, { Galley, parse } from "usfm-galley";
//! import { FindingsSnapshot } from "usfm-galley/sous-reader";
//!
//! const g = new Galley();                       // one handle, kept across edits
//! g.update("books/MRK.usfm", text);             // → "MRK"
//! FindingsSnapshot.open(g.publish());           // every finding in the corpus
//! deserialize(g.parse("books/MRK.usfm", 1, 1, 1));  // off the retained copy
//! deserialize(parse(text, true, true, true));   // the stateless door, same module
//! ```
//!
//! Every `onion-wasm` door stands on this module too, linked by [`onion`], so
//! a consumer imports ONE package and picks per call: the free functions
//! compute from scratch, [`Galley`] reuses what did not change. Nothing here is
//! a new rendering — [`Galley::parse`] returns the buffer `onion_wasm::parse`
//! returns, and [`Galley::publish`] returns the buffer the native
//! [`Expediter`] returns, byte for byte. Runbook, the door list, and the
//! equivalence claim: `galley/src/wasm.md`.

use wasm_bindgen::prelude::*;

use mise::utf16::Utf16Table;
use sous_core::Brigade;
use sous_core::judge::{Channels, JudgingConfig};
use sous_core::proportionality::LengthConfig;

use crate::find::Find;
use crate::mask::Recipe;
use crate::overlay::Side;
use crate::pantry::{BookId, Entry, Retain, Role};
use crate::sous::Expediter;

mod find;
mod mask;
pub mod onion;
pub mod overlay;

/// The two flags the find doors take, as the query Find prepares once.
///
/// `case_sensitive` rather than `case_insensitive` because that is the
/// checkbox a host draws; the engine's option is the negation of it.
fn query(needle: &str, case_sensitive: bool, whole_word: bool) -> Find<'_> {
    Find::literal(needle)
        .case_insensitive(!case_sensitive)
        .whole_word(whole_word)
}

/// Which roles `findAll`'s scope names, in the order their books are searched.
///
/// A string rather than a number, so the JS call reads as what it does; an
/// unknown one is an error rather than a silent fall back to the default.
fn scope_roles(scope: Option<String>, door: &str) -> Result<&'static [Role], JsError> {
    match scope.as_deref().unwrap_or("targets") {
        "targets" => Ok(&[Role::Target]),
        "references" => Ok(&[Role::Reference]),
        "all" => Ok(&[Role::Target, Role::Reference]),
        other => Err(JsError::new(&format!(
            "unknown {door} scope {other:?}; expected \"targets\", \"references\" or \"all\""
        ))),
    }
}

/// Products for roughly three of the largest books in the wild. Measured: the
/// biggest book in the corpus (en_ult PSA, 5.1 MB) is 3.27 MB warm, 4.62 MB
/// after fifty keystrokes. Deliberately modest — wasm linear memory grows and
/// never shrinks, so a budget is a permanent high-water mark, not a ceiling
/// the page falls back from.
const DEFAULT_BUDGET: usize = 16 << 20;

/// The corpus, resident: one [`Expediter`], one Pantry, one snapshot out.
///
/// One per project, not per document. Books go in whole by caller id and come
/// back as one complete publication; the Pantry inside owns the chunk cache,
/// so the onion methods read the same warm chunks the analysis does.
#[wasm_bindgen]
pub struct Galley {
    sous: Expediter<Brigade>,
}

#[wasm_bindgen]
impl Galley {
    /// `budgetBytes` bounds resident products; omit it for 16 MB.
    #[wasm_bindgen(constructor)]
    pub fn new(budget_bytes: Option<f64>) -> Galley {
        let budget = budget_bytes
            .filter(|b| b.is_finite() && *b >= 0.0)
            .map_or(DEFAULT_BUDGET, |b| b as usize);
        Galley {
            sous: Expediter::new(Brigade::default(), budget),
        }
    }

    // ── The corpus ──────────────────────────────────────────────────────

    /// Register or replace one whole book under the caller's `id`, as a
    /// target: it keeps its text, and it publishes findings.
    ///
    /// Returns the `\id` line's canonical book code — `"MRK"` — which is what
    /// orders the publication. Idempotent: the same text costs a checksum.
    ///
    /// `text` must be LF-normalized, the contract every door here documents.
    pub fn update(&mut self, id: &str, text: &str) -> Result<String, JsError> {
        self.sous
            .update(id, Role::Target, text)
            .map(|key| key.to_string())
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// The same, as a declared source: one projected grapheme length per
    /// verse and no text at all, so a reference costs a fraction of a target.
    ///
    /// A reference publishes no findings of its own; it is the denominator
    /// the length lane compares a target's verses against.
    ///
    /// `keepText` — omitted is `false` — makes it keep the text and the
    /// projection a target keeps too, which is what [`find`](Self::find) and
    /// `findAll`'s `"references"` scope read. It costs what a target costs
    /// minus the resident analysis; a source nobody searches should stay off
    /// it.
    #[wasm_bindgen(js_name = updateReference)]
    pub fn update_reference(
        &mut self,
        id: &str,
        text: &str,
        keep_text: Option<bool>,
    ) -> Result<String, JsError> {
        let retain = match keep_text.unwrap_or(false) {
            true => Retain::Text,
            false => Retain::ProductsOnly,
        };
        self.sous
            .update_with(id, Role::Reference, retain, text)
            .map(|key| key.to_string())
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Drop a book, its text, and its cached rows. `false` when the id was
    /// never registered.
    pub fn remove(&mut self, id: &str) -> bool {
        self.sous.remove(&BookId::from(id))
    }

    /// One complete corpus publication over every target, in canonical book
    /// order, in raw-book UTF-16 — the buffer `FindingsSnapshot.open` reads.
    ///
    /// A snapshot replaces the previous one whole; row positions are valid
    /// only inside the buffer they came from.
    pub fn publish(&mut self) -> Result<Vec<u8>, JsError> {
        self.sous
            .publish()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    // ── Find ────────────────────────────────────────────────────────────

    /// Every hit of `needle` in ONE registered book's verse-text projection,
    /// as the find buffer ([`crate::find::wire`] and `wasm.md` state the
    /// layout: magic and version, then little-endian `u32`, UTF-16 offsets,
    /// both coordinate spaces per hit).
    ///
    /// ```ts
    /// interface FindOptions {
    ///   caseSensitive?: boolean;                        // default false
    ///   wholeWord?: boolean;                            // default false
    ///   limit?: number;                                 // default 0: no bound
    ///   scope?: "targets" | "references" | "all";       // findAll only; default "targets"
    /// }
    /// find(id: string, needle: string, opts?: FindOptions): Uint8Array;
    /// findAll(needle: string, opts?: FindOptions): Uint8Array;
    /// ```
    ///
    /// The search runs over the PROJECTION — what a reader sees — so a needle
    /// inside a footnote is not found, and a needle that spans one comes back
    /// as one source range per contiguous piece. That is the whole reason the
    /// buffer carries a piece count per hit.
    ///
    /// Literal only: `needle` is never a pattern. `wholeWord` is the words
    /// rule galley restates in `find.md`; `caseSensitive` off is the simple
    /// lowercase fold, not a collator. `limit` bounds hits across the whole
    /// call, and `0` (the default) means no bound. `scope` names ONE book, so
    /// it belongs to `findAll` only — present here it throws. Any registered
    /// book that retains text and a projection may be searched — a target, or
    /// a reference registered with `keepText`. One that retains neither errors
    /// by name, because answering "no hits" would say it was clean.
    pub fn find(&mut self, id: &str, needle: &str, opts: JsValue) -> Result<Vec<u8>, JsError> {
        let opts = find::options(&opts)?;
        if opts.scope.is_some() {
            return Err(JsError::new("scope applies to findAll, not find"));
        }
        let wanted = self.projected(id)?;
        Ok(self.sous.find(
            &query(needle, opts.case_sensitive, opts.whole_word),
            &[wanted],
            opts.limit,
        ))
    }

    /// The same over every searchable book in `opts.scope`, in canonical book
    /// order — the project-wide find. See [`find`](Self::find) for
    /// `FindOptions`.
    ///
    /// `scope` is `"targets"` (the default when omitted), `"references"`, or
    /// `"all"`, which searches the targets and then the references. A
    /// reference registered without `keepText` is in no scope: it retains
    /// nothing to search, so it is not listed either.
    ///
    /// The buffer's `bookIndex` indexes its own id table, which names every
    /// book searched whether or not it matched, so a consumer never has to ask
    /// a second question to learn which book a hit is in.
    #[wasm_bindgen(js_name = findAll)]
    pub fn find_all(&mut self, needle: &str, opts: JsValue) -> Result<Vec<u8>, JsError> {
        let opts = find::options(&opts)?;
        let ids: Vec<BookId> = scope_roles(opts.scope, "find")?
            .iter()
            .flat_map(|role| self.sous.pantry().books_with_text(*role))
            .collect();
        Ok(self.sous.find(
            &query(needle, opts.case_sensitive, opts.whole_word),
            &ids,
            opts.limit,
        ))
    }

    // ── The census: what the project holds, without a parse ─────────────

    /// One registered book's census — its chapter rows and verse anchors, off
    /// the `Toc` that `update` built and the Pantry pins.
    ///
    /// Nothing is derived here: no chunk is resolved, no text is read, no wire
    /// is plated. `utf16` rebases every offset through the book's own retained
    /// table; the default is bytes.
    ///
    /// Read it with `usfm-galley/toc-reader`. The layout is generated from the
    /// same declaration the writer is, so no consumer learns one.
    pub fn toc(&self, id: &str, utf16: Option<bool>) -> Result<Vec<u8>, JsError> {
        let wanted = BookId::from(id);
        if self.sous.pantry().role(&wanted).is_none() {
            return Err(JsError::new(&format!("no book is registered as {id}")));
        }
        crate::toc::encode(self.sous.pantry(), &[wanted], utf16.unwrap_or(false))
            .map_err(census_refusal)
    }

    /// The same over every registered book in `scope`, in canonical book order
    /// — the project-wide census, and the call that takes one parse per book
    /// off a project's open.
    ///
    /// The scope is wider than `findAll`'s on purpose: a reference that kept no
    /// text still kept its `Toc`, so it is listed. The one thing it cannot
    /// answer is `utf16`, because the table that rebases offsets travels with
    /// the text.
    #[wasm_bindgen(js_name = tocAll)]
    pub fn toc_all(&self, scope: Option<String>, utf16: Option<bool>) -> Result<Vec<u8>, JsError> {
        let ids: Vec<BookId> = scope_roles(scope, "census")?
            .iter()
            .flat_map(|role| self.sous.pantry().books(*role))
            .map(|(id, _)| id.clone())
            .collect();
        crate::toc::encode(self.sous.pantry(), &ids, utf16.unwrap_or(false)).map_err(census_refusal)
    }

    // ── Match formatting: a target's skeleton made the source's ─────────

    /// One registered book's block structure, as JSON — either side, and the
    /// whole truth for drawing.
    ///
    /// ```ts
    /// interface Skeleton {
    ///   verses: { sid: string; from: number; to: number; textFrom: number; textTo: number }[];
    ///                  // the \v marker span, and the verse's own text span
    ///   blocks: SkeletonRow[];
    /// }
    /// interface SkeletonRow {
    ///   sid: string; where: "leading" | "inside"; ordinal: number;   // the address
    ///   marker: string;                                              // "q1"
    ///   from: number; to: number;                                    // the marker node's span
    ///   empty: boolean;                       // onion's empty paragraph; a source folds these away
    /// }
    /// ```
    ///
    /// `opts` is an [`OverlayOptions`](Self::overlay) — only `markers` is read
    /// here — and `utf16` asks for UTF-16 offsets instead of bytes. A block is
    /// LEADING when it sits immediately before its verse's `\v`, INSIDE when
    /// the verse's own text is above it; ordinals count from one per address.
    pub fn skeleton(
        &mut self,
        id: &str,
        opts: JsValue,
        utf16: Option<bool>,
    ) -> Result<String, JsError> {
        let (options, flag) = overlay::options(&opts)?;
        let wanted = BookId::from(id);
        let mut skeleton = self.sous.skeleton(&wanted, &options).map_err(door)?;
        let utf16 = utf16.unwrap_or(flag);
        let table = self.table(&wanted, utf16)?;
        overlay::rebase_skeleton(&mut skeleton, table.as_ref());
        overlay::json(&skeleton)
    }

    /// The edits that make `targetId`'s skeleton `sourceId`'s, exactly.
    ///
    /// ```ts
    /// interface OverlayOptions {
    ///   markers?: string[];                              // default: onion's paragraph+poetry block set, no titles
    ///   scope?: { chapter: number } | { sid: string };   // default: the whole book
    ///   utf16?: boolean;                                 // default false: byte offsets; true: UTF-16 units, like parse/find
    /// }
    /// ```
    ///
    /// A source block the target lacks is INSERTED — before the verse's `\v`
    /// when it is leading, EMPTY after the verse's text when it is inside,
    /// because where a verse's text splits is unknowable across languages and
    /// the translator pastes each line into place. A target block the source
    /// lacks is REMOVED and its text joins the block before it. Footnotes and
    /// cross-references never cross; their locations are the target's own.
    ///
    /// The transaction is ascending and non-overlapping, so a host applies it
    /// through the document as ONE undo step — it is `onion-wasm`'s own
    /// `Edits`, the class `formatEdits` answers with, so an editor applies an
    /// overlay exactly as it applies a fix. Its spans are BYTES here unless
    /// `utf16` asks otherwise; `formatEdits`'s are always UTF-16.
    pub fn overlay(
        &mut self,
        target_id: &str,
        source_id: &str,
        opts: JsValue,
    ) -> Result<onion_wasm::Edits, JsError> {
        let (options, utf16) = overlay::options(&opts)?;
        let target = BookId::from(target_id);
        let computed = self
            .sous
            .overlay(&target, &BookId::from(source_id), &options)
            .map_err(door)?;
        let table = self.table(&target, utf16)?;
        Ok(overlay::edits(&computed, table.as_ref()))
    }

    /// The same transaction applied — the target's own bytes under the
    /// source's structure. [`overlay`](Self::overlay) is what an editor wants;
    /// this is for a caller that only needs the string.
    #[wasm_bindgen(js_name = overlayText)]
    pub fn overlay_text(
        &mut self,
        target_id: &str,
        source_id: &str,
        opts: JsValue,
    ) -> Result<String, JsError> {
        let (options, _) = overlay::options(&opts)?;
        self.sous
            .overlay_text(&BookId::from(target_id), &BookId::from(source_id), &options)
            .map_err(door)
    }

    /// What the overlay did, and what it declined to do, as JSON.
    ///
    /// ```ts
    /// interface BlockAddress { sid: string; where: "leading" | "inside"; ordinal: number;
    ///                           marker: string }   // the spelling that position held
    /// interface OverlayReport {
    ///   inserted:  { address: BlockAddress; marker: string; at: number; empty: boolean }[];  // empty = Inside block awaiting text
    ///   removed:   { address: BlockAddress; marker: string; from: number; to: number }[];    // target blocks the source lacks
    ///   collapsed: { sid: string; marker: string; count: number }[];                         // source empty-block runs folded to one
    ///   unpaired:  { sid: string; side: "target" | "source"; reason: "absent" | "bridge" | "ambiguous" }[];
    /// }
    /// ```
    ///
    /// An overlay is a SUGGESTION applied on request, never a finding.
    #[wasm_bindgen(js_name = overlayReport)]
    pub fn overlay_report(
        &mut self,
        target_id: &str,
        source_id: &str,
        opts: JsValue,
    ) -> Result<String, JsError> {
        let (options, utf16) = overlay::options(&opts)?;
        let target = BookId::from(target_id);
        let mut report = self
            .sous
            .overlay(&target, &BookId::from(source_id), &options)
            .map_err(door)?
            .report;
        let table = self.table(&target, utf16)?;
        overlay::rebase_report(&mut report, table.as_ref());
        overlay::json(&report)
    }

    /// A SOURCE block's address, answered in the target: where it is, or
    /// where the overlay would put it.
    ///
    /// ```ts
    /// type Equivalent =
    ///   | { found: SkeletonRow }                                            // same address on the other side
    ///   | { absent: true; insertAt: number; where: "leading" | "inside" }   // where overlay would put it
    ///   | { unpaired: true; reason: "absent" | "bridge" | "ambiguous" };    // the verse itself has no pair
    /// ```
    ///
    /// `address.marker` is REQUIRED and is checked against the side the
    /// address was taken from: if that position still exists but now spells
    /// something else, the call THROWS ("… names q2 but the node there is q1
    /// — the address is stale") rather than answering about another node. The
    /// position is still the key; the name is only the check.
    #[wasm_bindgen(js_name = targetNodeFor)]
    pub fn target_node_for(
        &mut self,
        target_id: &str,
        source_id: &str,
        address: JsValue,
        opts: JsValue,
        utf16: Option<bool>,
    ) -> Result<String, JsError> {
        self.node_for(target_id, source_id, address, opts, utf16, Side::Target)
    }

    /// A TARGET block's address, answered in the source — the mirror of
    /// [`targetNodeFor`](Self::target_node_for), and the same three answers.
    #[wasm_bindgen(js_name = sourceNodeFor)]
    pub fn source_node_for(
        &mut self,
        target_id: &str,
        source_id: &str,
        address: JsValue,
        opts: JsValue,
        utf16: Option<bool>,
    ) -> Result<String, JsError> {
        self.node_for(target_id, source_id, address, opts, utf16, Side::Source)
    }

    // ── Judging ─────────────────────────────────────────────────────────

    /// A copy of the settings the next [`publish`](Self::publish) judges with.
    pub fn config(&self) -> SousSettings {
        SousSettings::from(&self.sous.config().1)
    }

    /// Replaces them. No chapter is remapped and no book refolded — judging
    /// reads the config, mapping does not — so a knob flip costs a re-judge.
    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(&mut self, settings: SousSettings) {
        let ((), mut substrate, mut words) = *self.sous.config();
        settings.apply(&mut substrate);
        settings.apply(&mut words);
        self.sous.set_config(((), substrate, words));
    }

    // ── Onion products, off the retained copy ───────────────────────────

    /// One registered book's diagnostics, off its retained text.
    ///
    /// Onion has no lint door of its own: a lint report crosses the wall as
    /// the `diagnostics` section of a parse buffer, so this is
    /// [`parse`](Self::parse) with that section alone asked for, read by the
    /// same `reader.ts`.
    pub fn lint(&mut self, id: &str) -> Result<Vec<u8>, JsError> {
        self.book_parse(
            id,
            crate::onion::wire::ParseOptions {
                diagnostics: true,
                ..Default::default()
            },
        )
    }

    /// One registered book, plated — the same buffer `onion_wasm::parse`
    /// returns for that text, with the lex, the tree and the lint walk reused
    /// for every chunk whose bytes did not change.
    ///
    /// Read it with the same `reader.ts` the stateless door's output uses:
    /// nothing here is a new rendering, only a cheaper route to the same
    /// bytes. A book that retains no text refuses.
    pub fn parse(
        &mut self,
        id: &str,
        diagnostics: bool,
        toc: bool,
        utf16: bool,
    ) -> Result<Vec<u8>, JsError> {
        self.book_parse(
            id,
            crate::onion::wire::ParseOptions {
                diagnostics,
                toc,
                utf16,
            },
        )
    }

    /// One registered book's verse text, off the projection it already
    /// retains — no mask is cut and no text crosses in.
    ///
    /// A reference registered without its text retains no projection and
    /// refuses.
    #[wasm_bindgen(js_name = verseText)]
    pub fn verse_text(&mut self, id: &str) -> Result<String, JsError> {
        let book = self.book(id)?;
        let mask = book.mask().map_err(refusal)?;
        let text = book.text().map_err(refusal)?;
        Ok(mask.text(text.as_bytes()))
    }

    /// One registered book's mask map — which source spans its projection is
    /// made of, in order. The projection is a pure concatenation of those
    /// spans, so a host holding the source rebuilds it from the map alone.
    ///
    /// ```ts
    /// interface MaskOptions {
    ///   recipe?: "verseText" | "structure" | "text";   // default "verseText"
    ///   utf16?: boolean;                               // default false
    /// }
    /// mask(id: string, opts?: MaskOptions): Uint8Array;
    /// maskOf(text: string, opts?: MaskOptions): Uint8Array;
    /// ```
    ///
    /// Each recipe is named for what SURVIVES it: `"verseText"` text inside
    /// verse extents only, `"structure"` the paragraph/chapter/verse skeleton,
    /// `"text"` every text byte anywhere with nothing removed — the cut a diff
    /// run's non-markup bytes are in.
    ///
    /// `"verseText"` is read off the retained projection; the other two cut
    /// the retained text. Same scope as `find`: a book that retains no text
    /// errors by name.
    ///
    /// Read it with `usfm-galley/mask-reader`. The layout is generated from the
    /// same declaration the writer is, so no consumer learns one.
    pub fn mask(&mut self, id: &str, opts: JsValue) -> Result<Vec<u8>, JsError> {
        let opts = mask::options(&opts)?;
        let wanted = self.projected(id)?;
        match opts.recipe {
            Recipe::VerseText => {
                let book = self.book(id)?;
                let cut = book.mask().map_err(refusal)?;
                let (table, source_len) = match opts.utf16 {
                    true => (
                        Some(book.utf16().map_err(refusal)?),
                        book.published_len().map_err(refusal)?,
                    ),
                    false => (None, book.text().map_err(refusal)?.len() as u32),
                };
                Ok(crate::mask::encode(cut, opts.recipe, table, source_len))
            }
            Recipe::Structure | Recipe::Text => {
                let (text, source_len) = {
                    let book = self.book(id)?;
                    let text = book.text().map_err(refusal)?.to_owned();
                    let len = match opts.utf16 {
                        true => book.published_len().map_err(refusal)?,
                        false => text.len() as u32,
                    };
                    (text, len)
                };
                let table = self.table(&wanted, opts.utf16)?;
                let cut = self.sous.masked(&text, &opts.recipe.filter());
                Ok(crate::mask::encode(
                    &cut,
                    opts.recipe,
                    table.as_ref(),
                    source_len,
                ))
            }
        }
    }

    // ── The same products over loose text ───────────────────────────────

    /// [`parse`](Self::parse) over text the host holds and has not registered
    /// — a preview pane, a file not yet in the project.
    ///
    /// The chunk cache keys on content, so an unregistered copy of a
    /// registered book still hits; what it costs over the id door is the
    /// string crossing the wall.
    #[wasm_bindgen(js_name = parseText)]
    pub fn parse_text(&mut self, text: &str, diagnostics: bool, toc: bool, utf16: bool) -> Vec<u8> {
        self.sous.parse(
            text,
            crate::onion::wire::ParseOptions {
                diagnostics,
                toc,
                utf16,
            },
        )
    }

    /// The verse text of loose text. See [`parse_text`](Self::parse_text).
    ///
    /// The string alone: a consumer that needs to get back to the source asks
    /// [`mask_of`](Self::mask_of) for the same cut's map.
    #[wasm_bindgen(js_name = verseTextOf)]
    pub fn verse_text_of(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .masked(text, &crate::onion::mask::Filter::verse_text());
        mask.text(text.as_bytes())
    }

    /// [`mask`](Self::mask) over text the host holds and has not registered,
    /// with the same options. Over a registered book's exact text the two
    /// doors answer the same buffer.
    #[wasm_bindgen(js_name = maskOf)]
    pub fn mask_of(&mut self, text: &str, opts: JsValue) -> Result<Vec<u8>, JsError> {
        let opts = mask::options(&opts)?;
        let cut = self.sous.masked(text, &opts.recipe.filter());
        let table = opts
            .utf16
            .then(|| mise::utf16::utf16_table(text.as_bytes()));
        let source_len = match opts.utf16 {
            true => mise::utf16::utf16_len(text.as_bytes()),
            false => text.len() as u32,
        };
        Ok(crate::mask::encode(
            &cut,
            opts.recipe,
            table.as_ref(),
            source_len,
        ))
    }

    /// The structure recipe's text, the verse-text mask's sibling. No book
    /// retains a structure projection, so this door takes text only.
    #[wasm_bindgen(js_name = structureTextOf)]
    pub fn structure_text_of(&mut self, text: &str) -> String {
        let mask = self
            .sous
            .masked(text, &crate::onion::mask::Filter::structure());
        mask.text(text.as_bytes())
    }

    // ── Dirty and rework ────────────────────────────────────────────────

    /// Chunk starts plus one checksum each, and no text — the ~1 KB baseline
    /// user land keeps beside a file on disk.
    pub fn fingerprint(&self, text: &str) -> Fingerprint {
        Fingerprint(crate::pantry::fingerprint(text))
    }

    /// Ranges of `text` whose chunk this book's last update never saw — the
    /// chunks a host would have to re-derive, in `text`'s own byte offsets,
    /// as `from, to` pairs.
    ///
    /// `undefined` when the id is not registered, which is the question a
    /// host asks before deciding to register it.
    #[wasm_bindgen(js_name = changedSinceUpdate)]
    pub fn changed_since_update(&self, id: &str, text: &str) -> Option<Vec<u32>> {
        self.sous
            .pantry()
            .changed_since_update(&BookId::from(id), text)
            .map(|ranges| pairs(&ranges))
    }

    // ── Telemetry, not contract ─────────────────────────────────────────

    /// Chunk units computed rather than reused, cumulative.
    pub fn misses(&self) -> f64 {
        self.sous.pantry().chunk_stats().misses as f64
    }

    /// Resident bytes across the whole handle: the Pantry's texts and
    /// products, and the Expediter's own cached rows.
    #[wasm_bindgen(js_name = residentBytes)]
    pub fn resident_bytes(&self) -> f64 {
        self.sous.resident_bytes() as f64
    }

    /// Cached chunk units. Zero for a one-chapter book: galley does not cache
    /// what it cannot reuse.
    #[wasm_bindgen(js_name = entryCount)]
    pub fn entry_count(&self) -> f64 {
        self.sous.pantry().chunk_stats().len as f64
    }

    /// Chapters mapped by the last [`publish`](Self::publish).
    #[wasm_bindgen(js_name = lastMapped)]
    pub fn last_mapped(&self) -> f64 {
        self.sous.last_mapped() as f64
    }

    /// Of those, the ones that kept an observation and re-walked only part.
    #[wasm_bindgen(js_name = lastRemapped)]
    pub fn last_remapped(&self) -> f64 {
        self.sous.last_remapped() as f64
    }

    /// Books rescanned for sites rather than replaying cached rows.
    #[wasm_bindgen(js_name = lastLocated)]
    pub fn last_located(&self) -> f64 {
        self.sous.last_located() as f64
    }

    /// Targets re-paired against their declared source.
    #[wasm_bindgen(js_name = lastPaired)]
    pub fn last_paired(&self) -> f64 {
        self.sous.last_paired() as f64
    }

    /// Declared sources the source-copy lane would have read and could not,
    /// because they were registered while `settings.source_copy` was off and so
    /// kept no word lane.
    ///
    /// Nonzero after turning the lane on means "re-send those references'
    /// text", not "nothing was found".
    #[wasm_bindgen(js_name = lastWordlessReferences)]
    pub fn last_wordless_references(&self) -> f64 {
        self.sous.last_wordless_references() as f64
    }
}

impl Galley {
    /// One registered book that retains a verse-text projection, or the
    /// refusal naming the argument that would fix it.
    ///
    /// Shared by the doors that read the projection — find and the mask map —
    /// because answering "nothing here" for a book that retains nothing would
    /// say it was clean.
    fn projected(&self, id: &str) -> Result<BookId, JsError> {
        let wanted = BookId::from(id);
        if self.sous.pantry().searchable(&wanted) {
            return Ok(wanted);
        }
        Err(JsError::new(&match self.sous.pantry().role(&wanted) {
            None => format!("no book is registered as {id}"),
            Some(Role::Reference) => {
                format!("reference {id} retains no text; register it with keepText")
            }
            Some(Role::Target) => format!("target {id} retains no verse-text projection"),
        }))
    }

    /// One registered book, or the refusal an unknown id earns.
    fn book(&mut self, id: &str) -> Result<Entry<'_>, JsError> {
        self.sous
            .book(&BookId::from(id))
            .ok_or_else(|| JsError::new(&format!("no book is registered as {id}")))
    }

    /// Shared by the two node doors, which differ only in which side answers.
    fn node_for(
        &mut self,
        target_id: &str,
        source_id: &str,
        address: JsValue,
        opts: JsValue,
        utf16: Option<bool>,
        want: Side,
    ) -> Result<String, JsError> {
        let (options, flag) = overlay::options(&opts)?;
        let address = overlay::address(&address)?;
        let target = BookId::from(target_id);
        let source = BookId::from(source_id);
        let mut answer = self
            .sous
            .node_for(&target, &source, &address, &options, want)
            .map_err(door)?;
        // The answer's offsets are in the side that answered.
        let named = match want {
            Side::Target => target,
            Side::Source => source,
        };
        let table = self.table(&named, utf16.unwrap_or(flag))?;
        overlay::rebase_equivalent(&mut answer, table.as_ref());
        overlay::json(&answer)
    }

    /// One book's byte → UTF-16 table, cloned only when a caller asked for
    /// UTF-16 offsets; `None` is the byte answer the doors default to.
    fn table(&mut self, id: &BookId, utf16: bool) -> Result<Option<Utf16Table>, JsError> {
        if !utf16 {
            return Ok(None);
        }
        Ok(Some(
            self.book(id.as_str())?.utf16().map_err(refusal)?.clone(),
        ))
    }

    fn book_parse(
        &mut self,
        id: &str,
        opts: crate::onion::wire::ParseOptions,
    ) -> Result<Vec<u8>, JsError> {
        self.book(id)?.parse(opts).map_err(refusal)
    }
}

/// An overlay refusal, as the JS error carrying its text.
fn door(error: crate::overlay::OverlayError) -> JsError {
    JsError::new(&error.to_string())
}

/// The census's one refusal, naming the argument that fixes it rather than
/// the state that caused it — the shape `find` set for a book that retains
/// too little to answer.
fn census_refusal(error: crate::pantry::PantryError) -> JsError {
    match error {
        crate::pantry::PantryError::NoProjection { id } => JsError::new(&format!(
            "{id} retains no UTF-16 table; register it with keepText, or ask for byte offsets"
        )),
        other => refusal(other),
    }
}

/// A Pantry refusal, as the JS error carrying its text.
fn refusal(error: crate::pantry::PantryError) -> JsError {
    JsError::new(&error.to_string())
}

/// Byte ranges as the flat `from, to` pairs a `Uint32Array` carries.
fn pairs(ranges: &[core::ops::Range<u32>]) -> Vec<u32> {
    ranges
        .iter()
        .flat_map(|range| [range.start, range.end])
        .collect()
}

/// One book's chunk starts and their checksums — no text, ~1 KB, opaque.
///
/// Dirty and rework are two questions, and this answers both separately:
/// [`differs_from`](Self::differs_from) is positional, so a chapter that only
/// moved is dirty; [`changed_chunks`](Self::changed_chunks) is set membership
/// over the checksums, so that same chapter needs no rework.
///
/// A handle, so JS frees it: `print.free()` when the baseline is dropped.
#[wasm_bindgen]
pub struct Fingerprint(crate::pantry::Fingerprint);

#[wasm_bindgen]
impl Fingerprint {
    /// Whether the two byte strings differ at all.
    #[wasm_bindgen(js_name = differsFrom)]
    pub fn differs_from(&self, current: &Fingerprint) -> bool {
        self.0.differs_from(&current.0)
    }

    /// Ranges of `current`'s text whose chunk this fingerprint never saw, as
    /// flat `from, to` pairs.
    #[wasm_bindgen(js_name = changedChunks)]
    pub fn changed_chunks(&self, current: &Fingerprint) -> Vec<u32> {
        pairs(&self.0.changed_chunks(&current.0))
    }

    /// Chunks in the text this fingerprint was taken from.
    #[wasm_bindgen(js_name = chunkCount)]
    pub fn chunk_count(&self) -> u32 {
        self.0.chunk_count() as u32
    }
}

/// The judging settings that cross the wall: every plain scalar of
/// [`JudgingConfig`], flat, so bindgen writes the getters and setters and JS
/// assigns `settings.casing = false`.
///
/// Not on the wall: `bands` and `word_bands` (a `Staircase` is a validated
/// ladder, not a plain field), `letters` and `doubles` (tri-state policies),
/// and the roster bounds. They have no plain-field shape and no consumer has
/// asked for them; everything not a knob keeps the current config's value
/// through [`apply`](SousSettings::apply), so widening this later breaks nothing.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SousSettings {
    // Channels.
    pub placement: bool,
    pub run_shape: bool,
    pub exact_neighbor: bool,
    pub pooled_neighbor: bool,
    pub rarity: bool,
    pub casing: bool,
    pub word_length: bool,
    pub doubled: bool,
    pub letter_runs: bool,
    pub sentence_start: bool,
    // Thresholds.
    pub support_floor: u32,
    pub word_support_floor: u32,
    pub terminal_upper_share_bp: u16,
    pub sentence_start_upper_bp: u16,
    pub word_length_sigma: u8,
    pub doubles_productive_bp: u16,
    // The source-compared lane.
    pub z_long: f32,
    pub z_short: f32,
    pub min_verses: u32,
    pub lengths_enabled: bool,
    pub presence: bool,
    pub source_copy: bool,
    pub source_copy_min_run: u32,
}

impl SousSettings {
    /// Every knob as this config holds it.
    pub fn from(config: &JudgingConfig) -> Self {
        let Channels {
            placement,
            run_shape,
            exact_neighbor,
            pooled_neighbor,
            rarity,
            casing,
            word_length,
            doubled,
            letter_runs,
            sentence_start,
        } = config.channels;
        Self {
            placement,
            run_shape,
            exact_neighbor,
            pooled_neighbor,
            rarity,
            casing,
            word_length,
            doubled,
            letter_runs,
            sentence_start,
            support_floor: config.support_floor,
            word_support_floor: config.word_support_floor,
            terminal_upper_share_bp: config.terminal_upper_share_bp,
            sentence_start_upper_bp: config.sentence_start_upper_bp,
            word_length_sigma: config.word_length_sigma,
            doubles_productive_bp: config.doubles_productive_bp,
            z_long: config.lengths.z_long,
            z_short: config.lengths.z_short,
            min_verses: config.lengths.min_verses,
            lengths_enabled: config.lengths.enabled,
            presence: config.lengths.presence,
            source_copy: config.lengths.source_copy,
            source_copy_min_run: config.lengths.source_copy_min_run,
        }
    }

    /// Writes them back, leaving every field that is not a knob alone.
    pub fn apply(self, config: &mut JudgingConfig) {
        config.channels = Channels {
            placement: self.placement,
            run_shape: self.run_shape,
            exact_neighbor: self.exact_neighbor,
            pooled_neighbor: self.pooled_neighbor,
            rarity: self.rarity,
            casing: self.casing,
            word_length: self.word_length,
            doubled: self.doubled,
            letter_runs: self.letter_runs,
            sentence_start: self.sentence_start,
        };
        config.support_floor = self.support_floor;
        config.word_support_floor = self.word_support_floor;
        config.terminal_upper_share_bp = self.terminal_upper_share_bp;
        config.sentence_start_upper_bp = self.sentence_start_upper_bp;
        config.word_length_sigma = self.word_length_sigma;
        config.doubles_productive_bp = self.doubles_productive_bp;
        config.lengths = LengthConfig {
            z_long: self.z_long,
            z_short: self.z_short,
            min_verses: self.min_verses,
            enabled: self.lengths_enabled,
            presence: self.presence,
            source_copy: self.source_copy,
            source_copy_min_run: self.source_copy_min_run,
        };
    }
}

impl Default for SousSettings {
    fn default() -> Self {
        Self::from(&JudgingConfig::default())
    }
}
