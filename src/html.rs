//! HTML export: the CST folded into a VIEW — the third serializer over the same
//! walk as [`crate::usj`] and [`crate::usx`].
//!
//! ```text
//! \id GEN                     <span class="usfm-id" data-marker="id"
//! \c 1                          data-usfm-type="book" data-code="GEN"></span>
//! \p                          <span class="chapter-num usfm-c usfm-lifted"
//! \v 1 verse one                data-marker="c" data-number="1"
//!                               data-sid="GEN 1">1</span>
//!                             <div class="usfm-p" data-marker="p" …>
//!                               <sup class="verse-num usfm-v usfm-lifted"
//!                                 data-marker="v" data-number="1"
//!                                 data-sid="GEN 1:1">1</sup>verse one</div>
//! ```
//!
//! ```text
//! \p \w gracious|lemma="grace"\w*   <span class="usfm-w" data-marker="w"
//!                                     data-lemma="grace">gracious</span>
//! \f + \ft note\f*                  <span class="note usfm-f" role="note"
//!                                     data-caller="+" data-note-kind="footnote">
//!                                     <sup class="note-caller …">1</sup>…</span>
//! \qt-s |who="Pilate"\*             <span class="ms usfm-qt-s" data-who="Pilate">
//!                                     </span>
//! \tr \tc1 a\tcr2 b                 <table class="usfm-table"><tr class="usfm-tr">
//!                                     <td class="usfm-tc1 align-start" …>a</td>…
//! ```
//!
//! # No config, and everything in `data-*` (RULED 2026-08-21, Will)
//!
//! v1 has NO options — no footnote style, no element overrides, no caller
//! scheme. Instead the fold SPLATS every fact it knows into `data-*`
//! attributes, so a consumer attaches behavior, lookups, styling, or a
//! post-hoc DOM remap keyed off them ("if you want to demote the imt1/mt1,
//! just crawl your DOM based on marker data attrs we splat out"). The concrete
//! element types therefore matter less than the payload: the authored tables
//! (`MarkerRow::html_element`, [`crate::tables::schema::HEADING_BASE_LEVEL`],
//! both audited and ACCEPTED 2026-08-21) give a defensible default, and the
//! data attributes give everything else.
//!
//! Every element carries `data-marker` (the marker AS SPELLED, `q1`/`qt-s`),
//! `data-usfm-type` (the USJ type projection) and `data-usfm-category` (the
//! spec's fine category). On top of that: every interpreter attribute as
//! `data-<name>`, the lift-table targets (`data-altnumber`, `data-pubnumber`,
//! `data-category`, `data-code`, `data-caller`, `data-alt`), chapter/verse
//! `data-number` + `data-sid`, `data-align` on cells and `data-level` on
//! headings.
//!
//! # HTML is a VIEW export — no oracle, no round trip
//!
//! There is no reference HTML to compare against (sketches/html-export.md's
//! scope ruling), so what pins this module is the zoo below, a
//! render-without-panic smoke, and the TEXT-IDENTITY INVARIANT in
//! `tests/html_corpus.rs`: strip the tags from our HTML and the remaining text
//! must equal the string content of our own [`crate::usj`] output. Everything
//! USJ lifts OUT of content and this fold renders VISIBLY (a chapter number, a
//! note caller, `\cat`, `\usfm`'s version, `\periph`'s title, `\rb`'s gloss)
//! is marked `class="usfm-lifted"` so that test can carve it out mechanically —
//! that class is the CONTRACT between the two, not decoration.
//!
//! # Whitespace: USJ's rules, all four
//!
//! Shared with USJ unchanged (see that module): one space per run, delimiters
//! are not content, block seams drop, a whitespace-only run is not content.
//! HTML collapses runs when it renders anyway, so USX's whitespace-is-content
//! inversion would only bloat the output. USX's eid/vid two-pass is likewise
//! absent: sids are `data-sid`, and nothing here needs to know where a verse
//! ENDS.
//!
//! # What the build had to decide that the tables did not cover
//!
//! Recorded in full in planning/sketches/html-export.md; the short list:
//!
//! - `HtmlElement::Para` renders `<div>` (RULED 2026-08-21; it first shipped
//!   as `<p>`): `<figure>` inside `<p>` is invalid and figures-in-paragraphs
//!   are the common case, so `<p>` made a browser reparse split the paragraph.
//!   The p-vs-b distinction survives in class/data-marker.
//! - `<ul>` is synthesized around a run of `\li`/`\lim` exactly the way
//!   `<table>` is synthesized around a run of `\tr` (`HtmlElement::ListContainer`
//!   says "synthesized by export, never mapped from a row").
//! - Cell alignment is THREE-way here (`tcc`/`thc` → `center`), where USJ/USX
//!   have only `start`/`end`.
//! - `\fig`'s `src` is NOT renamed (USJ calls it `file`): `<img src>` wants the
//!   real name.
//! - An unknown marker in PARAGRAPH position takes the block spelling
//!   (`<div class="usfm-s5">`), not row 0's `Span` — row 0's value governs its
//!   inline/milestone spelling.

use std::borrow::Cow;

use crate::attributes::{AttrEvent, AttrResolution, attrs, resolve};
use crate::cst::{CloseReason, Cst, NODE_ID_BIT, Node, ROOT_TOKEN, container_kind};
use crate::export::{
    canonical, is_ws, marker_name, note_peers, text_at, trim, trim_end, trim_start,
};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::{Category, HtmlElement, MarkerKind, heading_level};
use crate::{Token, TokenKind};

/// The class that marks text this fold renders VISIBLY but USJ lifted into an
/// attribute (or dropped). `tests/html_corpus.rs`'s text-identity invariant
/// strips the content of every element carrying it — see the module doc.
const LIFTED: &str = "usfm-lifted";

/// Folds a lexed + built document into HTML.
///
/// `tokens` must be `lex(source)`'s output and `cst` must be
/// [`crate::cst::build`]'s over those tokens — the fold reads token spans out of
/// `source` and trusts the CST's shape.
pub fn html(source: &[u8], tokens: &[Token], cst: &Cst) -> String {
    let mut export = Export {
        source,
        tokens,
        cst,
        out: Out {
            out: String::with_capacity(source.len() * 3),
        },
        lists: Vec::new(),
        book: None,
        chapter: None,
        pending: None,
        absorb: None,
        callers: [0, 0],
        cursor: 0,
    };
    export.run();
    export.out.out
}

// ---------------------------------------------------------------------------
// The writer
// ---------------------------------------------------------------------------

/// The hand-rolled HTML writer: a `String` and two escapers. Same no-serde /
/// no-template rationale as USJ's and USX's — the shape is small and closed, and
/// the library stays dependency-free.
struct Out {
    out: String,
}

impl Out {
    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Text content. `&` and `<` must be escaped; `>` is escaped too because a
    /// uniform rule is cheaper to trust than a contextual one. C0 controls pass
    /// through — unlike XML, HTML has no problem holding them.
    fn text(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '&' => self.raw("&amp;"),
                '<' => self.raw("&lt;"),
                '>' => self.raw("&gt;"),
                ch => self.out.push(ch),
            }
        }
    }

    /// An attribute VALUE, quote always double — so `"` escapes and `'` does
    /// not.
    fn value(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '&' => self.raw("&amp;"),
                '<' => self.raw("&lt;"),
                '>' => self.raw("&gt;"),
                '"' => self.raw("&quot;"),
                ch => self.out.push(ch),
            }
        }
    }

    /// ` name="value"`.
    fn attr(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.raw(name);
        self.raw("=\"");
        self.value(value);
        self.out.push('"');
    }

    /// ` data-<name>="value"`, the name already sanitized.
    fn data(&mut self, name: &str, value: &str) {
        self.raw(" data-");
        self.raw(name);
        self.raw("=\"");
        self.value(value);
        self.out.push('"');
    }
}

/// One attribute name, made legal for HTML: lowercased, and every byte outside
/// `[a-z0-9-_.]` folded to `-`. USFM's own names (`lemma`, `link-href`, `x-foo`)
/// pass through untouched; a deformed one (`a b`, `a:b`) is sanitized and its
/// RAW spelling is preserved beside it in `data-usfm-raw-attrs` — see
/// [`Export::write_data_attrs`], which is what "keep the raw name if
/// sanitization is lossy" buys.
fn sanitize(name: &str) -> Cow<'_, str> {
    let clean = |byte: u8| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
    };
    if !name.is_empty() && name.bytes().all(clean) {
        return Cow::Borrowed(name);
    }
    let mut out = String::with_capacity(name.len().max(1));
    for byte in name.bytes() {
        let byte = byte.to_ascii_lowercase();
        out.push(if clean(byte) { byte as char } else { '-' });
    }
    if out.is_empty() {
        out.push('-');
    }
    Cow::Owned(out)
}

/// The spec's fine category as a class-shaped name, for `data-usfm-category`.
/// Exhaustive so a new [`Category`] variant must name itself here.
fn category_name(category: Category) -> &'static str {
    use Category as C;
    match category {
        C::Unknown => "unknown",
        C::ParaIdentification => "para-identification",
        C::ParaIntroductions => "para-introductions",
        C::ParaTitlesSections => "para-titles-sections",
        C::ParaBody => "para-body",
        C::ParaPoetry => "para-poetry",
        C::ParaLists => "para-lists",
        C::ParaTables => "para-tables",
        C::ParaPeripheral => "para-peripheral",
        C::CharTextFeatures => "char-text-features",
        C::CharFormatting => "char-formatting",
        C::CharBreaks => "char-breaks",
        C::CharIntroductions => "char-introductions",
        C::CharPoetry => "char-poetry",
        C::CharLists => "char-lists",
        C::CharTables => "char-tables",
        C::CharNotes => "char-notes",
        C::NoteFootnote => "note-footnote",
        C::NoteCrossReference => "note-cross-reference",
        C::MilestoneList => "milestone-list",
        C::MilestoneTable => "milestone-table",
        C::MilestoneQt => "milestone-qt",
        C::MilestoneTs => "milestone-ts",
        C::MilestoneVid => "milestone-vid",
        C::ChapterVerse => "chapter-verse",
        C::Sidebar => "sidebar",
        C::Meta => "meta",
        C::Peripheral => "peripheral",
        C::DocumentStructure => "document-structure",
        C::Figure => "figure",
    }
}

/// The element a row's [`HtmlElement`] renders as, and the string that closes
/// it. The AUTHORED table picks the value; this is the one place a value becomes
/// bytes.
fn element(
    html_element: HtmlElement,
    marker_idx: MarkerIdx,
    level: u8,
) -> (&'static str, &'static str) {
    match html_element {
        // No row uses `Transparent` after the 2026-08-21 audit; the policy
        // (children emit, no wrapper of our own) is still honoured.
        HtmlElement::Transparent => ("", ""),
        // `<div>`, not `<p>` (RULED 2026-08-21): `\fig` inside a paragraph is
        // the COMMON case post-F1, and `<figure>` inside `<p>` makes a browser
        // reparse hoist the figure and split the paragraph — our output must
        // parse to the DOM we wrote. The p/b distinction lives on in
        // class/data-marker, which is where the v1 ruling says consumers look.
        HtmlElement::Para => ("div", "</div>"),
        HtmlElement::Heading => match level {
            1 => ("h1", "</h1>"),
            2 => ("h2", "</h2>"),
            3 => ("h3", "</h3>"),
            4 => ("h4", "</h4>"),
            5 => ("h5", "</h5>"),
            // `s4` lands on h6 EXACTLY (the audited table's deepest legal
            // level); anything past it would be a table bug, and `<h7>` is not
            // an element, so it clamps here rather than emitting nonsense.
            _ => ("h6", "</h6>"),
        },
        HtmlElement::Span | HtmlElement::SelfClosingSpan => ("span", "</span>"),
        HtmlElement::ListItem => ("li", "</li>"),
        HtmlElement::ListContainer => ("ul", "</ul>"),
        HtmlElement::Table => ("table", "</table>"),
        HtmlElement::TableRow => ("tr", "</tr>"),
        // `<th>` when the cell marker's own name starts `th` — derivable from
        // the row's name, which is why it costs no second column.
        HtmlElement::TableCell => {
            if generated::name(marker_idx).starts_with("th") {
                ("th", "</th>")
            } else {
                ("td", "</td>")
            }
        }
        HtmlElement::Aside => ("aside", "</aside>"),
        HtmlElement::Sup => ("sup", "</sup>"),
        HtmlElement::Anchor => ("a", "</a>"),
        // The column names the OUTER element; the interior is a fixed template
        // (`<img>` + `<figcaption>` around the caption content).
        HtmlElement::Figure => ("figure", "</figcaption></figure>"),
        HtmlElement::Image => ("img", ""),
        HtmlElement::Section => ("section", "</section>"),
        HtmlElement::Ruby => ("ruby", "</ruby>"),
        HtmlElement::Bold => ("b", "</b>"),
        HtmlElement::Italic => ("i", "</i>"),
        HtmlElement::Em => ("em", "</em>"),
        HtmlElement::Div => ("div", "</div>"),
    }
}

/// Which cell alignment the cell marker's own name spells. THREE-way here, where
/// USJ and USX have only `start`/`end`: `tcc`/`thc` are the CENTERED spellings
/// and HTML has somewhere to put that (a class the app's CSS reads).
///
/// Read off the suffix AFTER the `tc`/`th` stem, not off the last byte the way
/// USJ's `ends_with('r')` can afford to be — `tc` itself ends in `c`.
fn align(marker_idx: MarkerIdx) -> &'static str {
    let name = generated::name(marker_idx);
    match name.strip_prefix("tc").or_else(|| name.strip_prefix("th")) {
        Some("r") => "end",
        Some("c") => "center",
        _ => "start",
    }
}

// ---------------------------------------------------------------------------
// One content list
// ---------------------------------------------------------------------------

/// A container this fold SYNTHESIZES around a run of siblings, because USFM has
/// no marker that opens one ([`HtmlElement::Table`] / [`HtmlElement::ListContainer`]
/// are "synthesized by export, never mapped from a row").
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Wrap {
    #[default]
    None,
    Table,
    List,
}

/// The state of ONE element's children while they are being written. Its own
/// stack rather than a frame field, because a TRANSPARENT node (a U25003
/// container) writes into its PARENT's: the frame stack and the list stack have
/// different depths on purpose.
#[derive(Default)]
struct ListState {
    /// The text run being accumulated between two elements.
    run: String,
    /// Whitespace read but not yet committed to `run`, held VERBATIM until what
    /// follows it is known — at a block seam the whole of it is dropped, and
    /// anywhere else it is one space.
    ws: String,
    /// Leading whitespace here DELIMITS a payload and is not content.
    at_boundary: bool,
    /// The synthesized container currently open in this list.
    wrap: Wrap,
}

// ---------------------------------------------------------------------------
// The fold
// ---------------------------------------------------------------------------
//
// A CONCRETE walker, like usj.rs's and usx.rs's, and deliberately not a shared
// `Visit` trait: the three writers' per-element state (a JSON array's comma
// bookkeeping, an XML element's rewind-to-self-closing, and this one's
// synthesized containers plus caller counters) is exactly what a trait would
// have to abstract over, and there is no fourth format asking for it. What the
// folds genuinely SHARE already lives in `src/export.rs`.

/// One frame of the driver's own stack — the same ChildCursor shape lint's walk
/// uses (no recursion), plus which element's children this node writes into.
struct Frame {
    next: u32,
    end: u32,
    /// The node's own opening-marker token id, so the leaf loop can skip it.
    token: u32,
    /// Index into [`Export::lists`]. A transparent node shares its parent's.
    list: usize,
    /// This frame pushed the list it points at, and pops it at close.
    owns_list: bool,
    /// What this frame owes the writer when it ends — `"</div>"`, or `""` for a
    /// node that opened no element at all.
    close: &'static str,
    /// `\rb`'s gloss, written as `<rt>` after the base text and before the
    /// close. `None` for every other element.
    rt: Option<String>,
    /// The one Text token this element rendered EARLY, as a lifted span:
    /// `\periph My Title|id="x"`'s title. `u32::MAX` when there is none.
    lifted_text: u32,
    /// This frame is a NOTE element: its own family's PEER markers, so the F3
    /// graft can tell a peer from an inline span.
    peers: Option<&'static [&'static str]>,
    /// This frame is an UNCLOSED note-text element (`\ft`, `\fqa`, `\xo`, …), so
    /// an explicitly-closed sibling GRAFTS into it instead of sealing it.
    adopts: bool,
    /// It has already grafted one span, so the direct note content that FOLLOWS
    /// that span resumes this element rather than starting a new one.
    grafted: bool,
}

/// A chapter or verse held back until its `\ca`/`\cp`/`\va`/`\vp` annotations
/// are known — they FOLLOW the marker they decorate.
struct Pending {
    verse: bool,
    number: String,
    altnumber: Option<String>,
    pubnumber: Option<String>,
    sid: Option<String>,
}

/// Which lifted slot the next payload token belongs to. `\cp` and `\vp` are
/// LEAVES (no scope, no closer), so their value arrives as the following token
/// rather than as node content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Absorb {
    AltNumber,
    PubNumber,
}

struct Export<'a> {
    source: &'a [u8],
    tokens: &'a [Token],
    cst: &'a Cst,
    out: Out,
    lists: Vec<ListState>,
    book: Option<String>,
    chapter: Option<String>,
    pending: Option<Pending>,
    absorb: Option<Absorb>,
    /// Auto-caller counters, `[footnote, cross-reference]` — a `+` caller
    /// numbers per note KIND (the two families count separately), and both reset
    /// at each `\id`.
    callers: [u32; 2],
    /// The next token the walk will deliver — how a node close asks what comes
    /// AFTER it without looking at the stack.
    cursor: u32,
}

impl<'a> Export<'a> {
    fn run(&mut self) {
        let root = &self.cst.nodes[0];
        self.lists.push(ListState {
            at_boundary: true,
            ..ListState::default()
        });
        let mut cur = Frame {
            next: root.children.start,
            end: root.children.end,
            token: ROOT_TOKEN,
            list: 0,
            owns_list: true,
            close: "",
            rt: None,
            lifted_text: u32::MAX,
            peers: None,
            adopts: false,
            grafted: false,
        };
        let mut stack: Vec<Frame> = Vec::new();

        loop {
            if cur.next == cur.end {
                // F3, the graft: an unclosed note-text element does NOT seal
                // just because its child list ran out — it STEALS the note's
                // next child when that child is an explicitly-closed span (or
                // the direct note content that follows one), one at a time.
                if cur.adopts && stack.last().is_some_and(|parent| self.grafts(parent, &cur)) {
                    let parent = stack.last_mut().expect("just checked");
                    let at = parent.next;
                    parent.next += 1;
                    cur.next = at;
                    cur.end = at + 1;
                    cur.grafted |= self.cst.child_ids[at as usize] & NODE_ID_BIT != 0;
                    continue;
                }
                self.close_frame(&mut cur);
                match stack.pop() {
                    Some(parent) => cur = parent,
                    None => break,
                }
                continue;
            }
            let child = self.cst.child_ids[cur.next as usize];
            cur.next += 1;

            if child & NODE_ID_BIT != 0 {
                if let Some(frame) = self.open_node(child & !NODE_ID_BIT, &cur) {
                    stack.push(cur);
                    cur = frame;
                }
                continue;
            }
            self.leaf(child, &cur);
        }
    }

    // -- leaves ------------------------------------------------------------

    fn leaf(&mut self, idx: u32, frame: &Frame) {
        self.cursor = idx + 1;
        if idx == frame.token {
            return; // the node's own opening marker
        }
        if idx == frame.lifted_text {
            // Already rendered at this element's open, as a lifted span
            // (`\periph`'s title); like the book code and the note caller it
            // only re-arms the delimiter rule for what follows.
            self.lists[frame.list].at_boundary = true;
            return;
        }
        let token = &self.tokens[idx as usize];
        let list = frame.list;
        match token.kind() {
            TokenKind::Text => {
                let text = self.span(token);
                self.push_text(list, text);
            }
            TokenKind::Newline => self.push_ws(list, "\n"),
            TokenKind::OptBreak => {
                self.seal_run(list, false);
                self.begin_item(list, Wrap::None);
                self.out.raw("<br class=\"usfm-optbreak\">");
                self.lists[list].at_boundary = false;
            }
            // Both were already read at their node's open; here they only
            // re-arm the delimiter rule for the text that follows them.
            TokenKind::BookCode => self.lists[list].at_boundary = true,
            TokenKind::NoteCaller => self.lists[list].at_boundary = true,
            TokenKind::Designator => match self.absorb.take() {
                Some(slot) => {
                    self.lift(slot, trim(self.span(token)).to_string(), list);
                    self.lists[list].at_boundary = true;
                }
                None => {
                    let number = self.span(token).to_string();
                    // A designator with no chapter/verse in front of it is
                    // lint's finding; the projection drops it rather than
                    // guessing an owner.
                    if let Some(verse) = self.pending.as_ref().map(|pending| pending.verse) {
                        if !verse {
                            self.chapter = Some(number.clone());
                        }
                        let sid = self.sid_for(verse, &number);
                        let pending = self.pending.as_mut().expect("just read");
                        pending.number = number;
                        pending.sid = sid;
                    }
                    self.lists[list].at_boundary = true;
                }
            },
            TokenKind::Marker { .. } => self.marker_leaf(idx, token, list),
            // Whitespace in FRONT of an explicit closer is CONTENT — `\k ostrich
            // \k*bird` keeps "ostrich " — so the closer commits it rather than
            // letting the element's close decide.
            TokenKind::ClosingMarker { .. } => self.commit_ws(list),
            TokenKind::MilestoneTerminator => self.lists[list].ws.clear(),
            TokenKind::AttrList => {}
            TokenKind::Milestone { .. } => {}
        }
    }

    /// A marker that opened no node: `\c`, `\v`, the lifted leaves, `\esbe`, an
    /// unknown marker, and the bare-`\*` milestone spelling.
    fn marker_leaf(&mut self, idx: u32, token: &Token, list: usize) {
        let marker = self.marker_name(token);
        if token.marker_idx == generated::UNRESOLVED {
            // `\zms\*`: the `\*` SPELLING says milestone, whatever the row does
            // not know — and row 0's `Span` is that spelling's element. An
            // unknown marker in PARAGRAPH position is a block instead: it stands
            // where a `\p` stands, and USJ/USX both give it the para shape.
            let milestone = matches!(
                self.tokens.get(idx as usize + 1).map(Token::kind),
                Some(TokenKind::MilestoneTerminator)
            );
            self.flush_pending(list);
            self.seal_run(list, !milestone);
            self.begin_item(list, Wrap::None);
            if milestone {
                self.out.raw("<span");
                self.class(&["ms"], &marker);
                self.out.data("marker", &marker);
                self.out.data("usfm-type", "ms");
                self.out.raw("></span>");
            } else {
                self.out.raw("<div");
                self.class(&[], &marker);
                self.out.data("marker", &marker);
                self.out.data("usfm-type", "para");
                self.out.raw("></div>");
            }
            self.lists[list].at_boundary = false;
            return;
        }

        match generated::kind(token.marker_idx) {
            MarkerKind::Chapter | MarkerKind::Verse => {
                let verse = generated::kind(token.marker_idx) == MarkerKind::Verse;
                self.flush_pending(list);
                // A chapter is a block seam; a verse is not.
                self.seal_run(list, !verse);
                self.pending = Some(Pending {
                    verse,
                    number: String::new(),
                    altnumber: None,
                    pubnumber: None,
                    sid: None,
                });
            }
            _ => match marker.as_str() {
                "ca" | "va" => self.absorb = Some(Absorb::AltNumber),
                "cp" | "vp" => self.absorb = Some(Absorb::PubNumber),
                _ => {}
            },
        }
    }

    /// Puts a lifted value on the pending chapter/verse. With no pending owner
    /// the projection must not guess one, so the value becomes an ordinary span
    /// and lint carries the placement complaint — the same answer USJ gives.
    fn lift(&mut self, slot: Absorb, value: String, list: usize) {
        match (&mut self.pending, slot) {
            (Some(pending), Absorb::AltNumber) => pending.altnumber = Some(value),
            (Some(pending), Absorb::PubNumber) => pending.pubnumber = Some(value),
            (None, _) => {
                self.seal_run(list, false);
                self.begin_item(list, Wrap::None);
                let marker = match slot {
                    Absorb::AltNumber => "ca",
                    Absorb::PubNumber => "cp",
                };
                // `<sup>` is what all four of `ca`/`cp`/`va`/`vp` carry in the
                // table — `cp`'s Sup is the 2026-08-21 audit's, replacing the
                // `Heading` it wrongly had — and this orphan path is the only
                // place a `\cp` reaches an element of its own at all.
                self.out.raw("<sup");
                self.class(&[], marker);
                self.out.data("marker", marker);
                self.out.data("usfm-type", "char");
                self.out
                    .data("usfm-category", category_name(Category::ChapterVerse));
                self.out.raw(">");
                self.out.text(&value);
                self.out.raw("</sup>");
                self.lists[list].at_boundary = false;
            }
        }
    }

    // -- the note graft (F3) -----------------------------------------------

    /// Does the note's next child GRAFT into `open`, the note-text element that
    /// just ran out of children? The rule and its measurements live on
    /// [`crate::usj`]'s twin; this is the same projection, spelled in HTML.
    fn grafts(&self, parent: &Frame, open: &Frame) -> bool {
        let Some(peers) = parent.peers else {
            return false;
        };
        if parent.next == parent.end {
            return false;
        }
        let child = self.cst.child_ids[parent.next as usize];
        if child & NODE_ID_BIT != 0 {
            let node = &self.cst.nodes[(child & !NODE_ID_BIT) as usize];
            let token = &self.tokens[node.token as usize];
            return node.close_reason() == CloseReason::Explicit
                && generated::kind(token.marker_idx) == MarkerKind::Character
                && !peers.contains(&self.marker_name(token).as_str());
        }
        open.grafted
            && matches!(
                self.tokens[child as usize].kind(),
                TokenKind::Text | TokenKind::Newline | TokenKind::OptBreak
            )
    }

    // -- nodes -------------------------------------------------------------

    /// Opens one child node. Returns the frame to descend into, or `None` when
    /// the node emits nothing at all and its subtree is skipped.
    fn open_node(&mut self, id: u32, parent: &Frame) -> Option<Frame> {
        let node = &self.cst.nodes[id as usize];
        let token = &self.tokens[node.token as usize];
        let marker_idx = token.marker_idx;
        let marker = self.marker_name(token);
        let list = parent.list;

        // A U25003 container (`\list-s … \list-e`) is walker bookkeeping, not an
        // element: its points emit inline where they sit and its children land
        // beside them.
        if container_kind(marker_idx).is_some()
            && self
                .cst
                .child_ids
                .get(node.children.start as usize)
                .is_some_and(|child| child & NODE_ID_BIT != 0)
        {
            return Some(Frame {
                next: node.children.start,
                end: node.children.end,
                token: node.token,
                list,
                owns_list: false,
                close: "",
                rt: None,
                lifted_text: u32::MAX,
                peers: None,
                adopts: false,
                grafted: false,
            });
        }

        // `\ca`/`\va`/`\cp`/`\vp` emit NOTHING of their own: their content
        // becomes `data-altnumber`/`data-pubnumber` on the chapter or verse, and
        // they must not break the text run around them either. Unless the
        // content is not PLAIN TEXT — an attribute cannot hold markup, so `\vp
        // \+it 21\+it*\vp*` stays an ordinary span with its nesting intact (the
        // rule both fixture formats agree on; see usj.rs).
        //
        // `\cat` and `\usfm` are NOT in this list, where USJ drops both: the
        // 2026-08-21 audit gave them `Span` precisely so a consumer has a class
        // to find and hide them by, so they render — marked LIFTED, since their
        // text is a `data-*` value on some other element in USJ's reading.
        let liftable = !self
            .cst
            .child_ids
            .get(node.children.start as usize..node.children.end as usize)
            .is_some_and(|kids| kids.iter().any(|child| child & NODE_ID_BIT != 0));
        match marker.as_str() {
            "ca" | "va" if liftable => {
                let value = self.node_text(id);
                self.lift(Absorb::AltNumber, value, list);
                return None;
            }
            "cp" | "vp" if liftable => {
                let value = self.node_text(id);
                self.lift(Absorb::PubNumber, value, list);
                return None;
            }
            _ => {}
        }

        let kind = generated::kind(marker_idx);
        // Both auto-caller counters reset at each `\id` — "per note kind, reset
        // per book" (sketches/html-export.md's caller table).
        if kind == MarkerKind::Header && marker == "id" {
            self.callers = [0, 0];
        }
        // F3: a note-text element that supplied NO closer of its own is the one
        // an explicitly-closed sibling grafts into (see `grafts`).
        let adopts = parent.peers.is_some()
            && kind == MarkerKind::Character
            && node.close_reason() != CloseReason::Explicit;
        let peers = (kind == MarkerKind::Note).then(|| note_peers(&marker));
        let is_milestone = matches!(token.kind(), TokenKind::Milestone { .. })
            || kind == MarkerKind::Milestone
            || marker_idx == generated::UNRESOLVED;
        let pairs = self.attr_pairs(node, marker_idx);

        self.flush_pending(list);
        self.seal_run(list, self.seam_at(Some(node.token)));
        self.begin_item(
            list,
            match kind {
                MarkerKind::TableRow => Wrap::Table,
                _ if generated::html_element(marker_idx) == Some(HtmlElement::ListItem) => {
                    Wrap::List
                }
                _ => Wrap::None,
            },
        );

        // A milestone POINT carries attributes and no content at all — an EMPTY
        // ADDRESSABLE SPAN (RULED 2026-08-21): it costs nothing, CSS hides it by
        // default, and alignment/quote milestones stay addressable for tooling.
        if is_milestone {
            self.out.raw("<span");
            self.class(&["ms"], &marker);
            self.out.data("marker", &marker);
            self.out.data("usfm-type", "ms");
            if marker_idx != generated::UNRESOLVED {
                self.out.data(
                    "usfm-category",
                    category_name(generated::category(marker_idx)),
                );
            }
            self.write_data_attrs(&pairs);
            self.out.raw("></span>");
            self.lists[list].at_boundary = false;
            return None;
        }

        let level = self.heading_level(marker_idx, &marker);
        let (tag, mut close) = match generated::html_element(marker_idx) {
            Some(html_element) => element(html_element, marker_idx, level),
            // No row has `None` today; "export decides" means the block default.
            None => ("div", "</div>"),
        };
        if tag.is_empty() {
            // `Transparent`: children flow into the parent's own list.
            return Some(Frame {
                next: node.children.start,
                end: node.children.end,
                token: node.token,
                list,
                owns_list: false,
                close: "",
                rt: None,
                lifted_text: u32::MAX,
                peers,
                adopts,
                grafted: false,
            });
        }

        let usfm_type = match kind {
            MarkerKind::Header if marker == "id" => "book",
            MarkerKind::Character if marker == "ref" => "ref",
            MarkerKind::Paragraph | MarkerKind::Header => "para",
            MarkerKind::Character => "char",
            MarkerKind::Note => "note",
            MarkerKind::Figure => "figure",
            MarkerKind::Sidebar => "sidebar",
            MarkerKind::Periph => "periph",
            MarkerKind::TableRow => "table:row",
            MarkerKind::TableCell => "table:cell",
            MarkerKind::Meta => "cat",
            // Chapter/Verse never open a scope, Milestone went above, Unknown is
            // row 0 (handled as a leaf). A row that opened a scope and lands
            // here is a table bug, not damage in the document.
            _ => "para",
        };

        // The extra classes: what a stylesheet keys on beyond the marker, and —
        // for `usfm-lifted` — what the text-identity invariant carves out.
        let mut extra: Vec<&str> = Vec::new();
        if kind == MarkerKind::Note {
            extra.push("note");
        }
        if kind == MarkerKind::Meta || marker == "usfm" {
            extra.push(LIFTED);
        }
        let align_class = (kind == MarkerKind::TableCell).then(|| match align(marker_idx) {
            "end" => "align-end",
            "center" => "align-center",
            _ => "align-start",
        });
        if let Some(class) = align_class {
            extra.push(class);
        }

        self.out.raw("<");
        self.out.raw(tag);
        self.class(&extra, &marker);
        if kind == MarkerKind::Note {
            self.out.attr("role", "note");
        }
        self.out.data("marker", &marker);
        self.out.data("usfm-type", usfm_type);
        self.out.data(
            "usfm-category",
            category_name(generated::category(marker_idx)),
        );
        if generated::html_element(marker_idx) == Some(HtmlElement::Heading) {
            self.out.data("level", &level.to_string());
        }

        if usfm_type == "book"
            && let Some(code) = self.payload_child(node, TokenKind::BookCode)
        {
            self.book = Some(code.clone());
            self.out.data("code", &code);
        }
        let caller = (kind == MarkerKind::Note)
            .then(|| self.payload_child(node, TokenKind::NoteCaller))
            .flatten();
        let xref = generated::category(marker_idx) == Category::NoteCrossReference;
        if let Some(caller) = &caller {
            self.out.data("caller", caller);
            self.out
                .data("note-kind", if xref { "crossref" } else { "footnote" });
        }
        if let Some(category) = self.category(node) {
            self.out.data("category", &category);
        }
        if marker == "usfm" {
            let version = self.node_text(id);
            self.out.data("version", &version);
        }
        // `\periph My Title|id="x"`: the TITLE TEXT is the division's `alt`
        // (usx.rng writes it as an attribute), so it becomes `data-alt` — and,
        // unlike USJ, it is ALSO rendered, as a lifted span, because a `<section>`
        // whose title is invisible is not a view.
        let mut lifted_text = u32::MAX;
        let mut title = None;
        let title_at = (kind == MarkerKind::Periph)
            .then(|| {
                self.direct_children(node)
                    .find(|child| self.tokens[*child as usize].kind() == TokenKind::Text)
            })
            .flatten();
        if let Some(at) = title_at {
            let value = trim(self.span(&self.tokens[at as usize])).to_string();
            self.out.data("alt", &value);
            lifted_text = at;
            title = Some(value);
        }
        if let Some(class) = align_class {
            self.out.data("align", class.trim_start_matches("align-"));
        }
        self.write_data_attrs(&pairs);
        self.out.raw(">");

        // The fixed interiors: everything the column's OUTER element implies.
        let mut rt = None;
        match generated::html_element(marker_idx) {
            Some(HtmlElement::Figure) => {
                if let Some(src) = value_of(&pairs, "src") {
                    self.out.raw("<img");
                    self.out.attr("src", src);
                    if let Some(alt) = value_of(&pairs, "alt") {
                        self.out.attr("alt", alt);
                    }
                    self.out.raw(">");
                }
                self.out.raw("<figcaption>");
            }
            // `\rb BASE|gloss="g"` — the gloss is an ATTRIBUTE, so its `<rt>`
            // text is lifted, and it must land AFTER the base text.
            Some(HtmlElement::Ruby) => {
                rt = value_of(&pairs, "gloss").map(str::to_string);
                if rt.is_none() {
                    close = "</ruby>";
                }
            }
            _ => {}
        }
        if let Some(title) = title {
            self.out.raw("<span class=\"periph-title ");
            self.out.raw(LIFTED);
            self.out.raw("\">");
            self.out.text(&title);
            self.out.raw("</span>");
        }
        if let Some(caller) = caller {
            self.write_caller(&caller, xref);
        }

        self.lists.push(ListState {
            at_boundary: true,
            ..ListState::default()
        });
        Some(Frame {
            next: node.children.start,
            end: node.children.end,
            token: node.token,
            list: self.lists.len() - 1,
            owns_list: true,
            close,
            rt,
            lifted_text,
            peers,
            adopts,
            grafted: false,
        })
    }

    /// The note caller, per the kind-keyed table (sketches/html-export.md):
    /// `+` auto-numbers per note KIND, `-` renders nothing visible, and anything
    /// else renders the literal the author wrote. Both rendered forms are text
    /// USJ holds in an attribute, hence [`LIFTED`]; the auto one additionally
    /// carries `note-caller-generated`, which is the only text in the whole
    /// output that no token supplied.
    fn write_caller(&mut self, caller: &str, xref: bool) {
        match caller {
            "-" => {}
            "+" => {
                let slot = usize::from(xref);
                self.callers[slot] += 1;
                let number = self.callers[slot].to_string();
                self.out
                    .raw("<sup class=\"note-caller note-caller-generated ");
                self.out.raw(LIFTED);
                self.out.raw("\">");
                self.out.text(&number);
                self.out.raw("</sup>");
            }
            literal => {
                self.out.raw("<sup class=\"note-caller ");
                self.out.raw(LIFTED);
                self.out.raw("\">");
                self.out.text(literal);
                self.out.raw("</sup>");
            }
        }
    }

    fn close_frame(&mut self, frame: &mut Frame) {
        let list = frame.list;
        self.flush_pending(list);
        // What ends this element decides its trailing whitespace: the next token
        // in DOCUMENT order, which the cursor is already sitting on.
        self.seal_run(list, self.seam_at(Some(self.cursor)));
        self.begin_item(list, Wrap::None);
        if let Some(gloss) = frame.rt.take() {
            self.out.raw("<rt class=\"");
            self.out.raw(LIFTED);
            self.out.raw("\">");
            self.out.text(&gloss);
            self.out.raw("</rt>");
        }
        self.out.raw(frame.close);
        if frame.owns_list {
            self.lists.pop();
            // The element just written is a child of its PARENT's list, and the
            // parent's boundary rule ends with it.
            if let Some(parent) = self.lists.last_mut() {
                parent.at_boundary = false;
            }
        }
    }

    // -- attributes --------------------------------------------------------

    /// One node's OWN attribute lists, resolved to name/value pairs. Later
    /// definition wins (the interpreter's merge rule); `=` spacing and quote
    /// style are the documented lossy step. No per-format RENAME here, unlike
    /// USJ/USX's `src`→`file`: `<img src>` wants the real name.
    fn attr_pairs(&self, node: &Node, marker_idx: MarkerIdx) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for child in self.direct_children(node) {
            let token = &self.tokens[child as usize];
            if token.kind() != TokenKind::AttrList {
                continue;
            }
            for event in attrs(self.source, token) {
                let AttrEvent::Attr(attr) = event else { break };
                let name = if attr.name.is_empty() {
                    match resolve(attr.name, marker_idx) {
                        AttrResolution::Defined { defined, .. } => defined.to_string(),
                        // A bare value on a row with no default attribute
                        // (`\fig |a.png`) names nothing; lint says so and the
                        // projection has nowhere to put it.
                        _ => continue,
                    }
                } else {
                    String::from_utf8_lossy(attr.name).into_owned()
                };
                let value = String::from_utf8_lossy(attr.value).into_owned();
                match pairs.iter_mut().find(|(key, _)| *key == name) {
                    Some(slot) => slot.1 = value,
                    None => pairs.push((name, value)),
                }
            }
        }
        pairs
    }

    /// Splats the pairs as `data-<name>`. A name that [`sanitize`] had to change
    /// is ALSO reported verbatim in `data-usfm-raw-attrs`, so nothing the author
    /// wrote is lost — the sanitized name is a lookup key, the raw list is the
    /// record.
    fn write_data_attrs(&mut self, pairs: &[(String, String)]) {
        let mut raw: Option<String> = None;
        for (name, value) in pairs {
            let clean = sanitize(name);
            if clean != *name {
                let entry = raw.get_or_insert_with(String::new);
                if !entry.is_empty() {
                    entry.push(' ');
                }
                entry.push_str(name);
                entry.push('=');
                entry.push_str(value);
            }
            self.out.data(&clean, value);
        }
        if let Some(raw) = raw {
            self.out.data("usfm-raw-attrs", &raw);
        }
    }

    /// A carved payload token's text (`\id`'s book code, a note's caller), read
    /// off the node's DIRECT children so it can be written before the open tag
    /// closes.
    fn payload_child(&self, node: &Node, kind: TokenKind) -> Option<String> {
        self.direct_children(node)
            .find(|child| self.tokens[*child as usize].kind() == kind)
            .map(|child| trim(self.span(&self.tokens[child as usize])).to_string())
    }

    /// A `\cat` child's content, lifted to `data-category` on this note/sidebar.
    /// The `\cat` element itself still RENDERS (marked [`LIFTED`]) — this is the
    /// attribute copy, not a replacement for it.
    fn category(&self, node: &Node) -> Option<String> {
        for child in node.children.clone() {
            let id = self.cst.child_ids[child as usize];
            if id & NODE_ID_BIT == 0 {
                continue;
            }
            let inner = &self.cst.nodes[(id & !NODE_ID_BIT) as usize];
            let token = &self.tokens[inner.token as usize];
            if self.marker_name(token) == "cat" {
                return Some(self.node_text(id & !NODE_ID_BIT));
            }
        }
        None
    }

    fn direct_children(&self, node: &Node) -> impl Iterator<Item = u32> + '_ {
        self.cst.child_ids[node.children.start as usize..node.children.end as usize]
            .iter()
            .copied()
            .filter(|id| id & NODE_ID_BIT == 0)
    }

    /// `class="<extra…> usfm-<marker as spelled>"`. The marker class is the
    /// scheme's whole hook: `q1`, `qt-s` and an unknown `\s5` all reach the page
    /// under the spelling the author wrote.
    fn class(&mut self, extra: &[&str], marker: &str) {
        self.out.raw(" class=\"");
        for class in extra {
            self.out.raw(class);
            self.out.raw(" ");
        }
        self.out.raw("usfm-");
        self.out.value(marker);
        self.out.raw("\"");
    }

    /// The `<hN>` level for a heading family: [`HEADING_BASE_LEVEL`]'s base plus
    /// the occurrence's own digit minus one (`mt1`→h1, `s2`→h4, `s4`→h6).
    ///
    /// [`HEADING_BASE_LEVEL`]: crate::tables::schema::HEADING_BASE_LEVEL
    fn heading_level(&self, marker_idx: MarkerIdx, spelled: &str) -> u8 {
        let family = generated::name(marker_idx);
        let digit = spelled
            .strip_prefix(family)
            .and_then(|rest| rest.parse::<u8>().ok());
        heading_level(family, digit)
    }

    // -- text runs ---------------------------------------------------------

    /// Whitespace joins the pending run; AT A BOUNDARY it is a delimiter and
    /// never becomes content.
    fn push_ws(&mut self, list: usize, text: &str) {
        let state = &mut self.lists[list];
        if !state.at_boundary {
            state.ws.push_str(text);
        }
    }

    /// Whitespace becomes content, canonicalized to one space.
    fn commit_ws(&mut self, list: usize) {
        let state = &mut self.lists[list];
        if state.ws.is_empty() {
            return;
        }
        if !state.run.ends_with(' ') {
            state.run.push(' ');
        }
        state.ws.clear();
    }

    fn push_text(&mut self, list: usize, text: &str) {
        let text = &canonical(text);
        let body = trim_start(text);
        let lead = &text[..text.len() - body.len()];
        let body = trim_end(body);
        let trail = &text[lead.len() + body.len()..];
        self.push_ws(list, lead);
        // Whitespace-only text is held, not emitted — and it must not force a
        // held-back chapter/verse out before its `\ca`/`\va` annotations have
        // been read.
        if body.is_empty() {
            return;
        }
        self.flush_pending(list);
        let state = &mut self.lists[list];
        if state.at_boundary {
            state.ws.clear();
            state.at_boundary = false;
        }
        self.commit_ws(list);
        let state = &mut self.lists[list];
        state.run.push_str(body);
        state.ws.push_str(trail);
    }

    /// Resolves the held whitespace against what comes next, then writes the
    /// run. THE whitespace decision, in one place.
    fn seal_run(&mut self, list: usize, seam: bool) {
        if seam {
            self.lists[list].ws.clear();
        } else {
            self.commit_ws(list);
        }
        self.flush_run(list);
    }

    /// Is the token at `idx` a BLOCK SEAM — a boundary whose whitespace was
    /// layout, not content? Same set as USJ's.
    fn seam_at(&self, idx: Option<u32>) -> bool {
        let Some(token) = idx.and_then(|idx| self.tokens.get(idx as usize)) else {
            return true;
        };
        match token.kind() {
            TokenKind::Marker { .. } => {
                token.marker_idx == generated::UNRESOLVED
                    || matches!(
                        generated::kind(token.marker_idx),
                        MarkerKind::Paragraph
                            | MarkerKind::Chapter
                            | MarkerKind::Header
                            | MarkerKind::Sidebar
                            | MarkerKind::Periph
                            | MarkerKind::TableRow
                    )
            }
            _ => false,
        }
    }

    /// Writes the accumulated run as text. A run that is whitespace ONLY is
    /// DROPPED — USJ's rule 4, kept here because HTML collapses runs when it
    /// renders and a whitespace-only text node buys a view nothing.
    fn flush_run(&mut self, list: usize) {
        let run = core::mem::take(&mut self.lists[list].run);
        if run.is_empty() || run.bytes().all(is_ws) {
            return;
        }
        self.out.text(&run);
        self.lists[list].at_boundary = false;
    }

    /// Opens or closes the SYNTHESIZED container this list currently wants: a
    /// `<table>` around a run of consecutive `\tr`, a `<ul>` around a run of
    /// consecutive `\li`/`\lim`. Neither has a marker that opens it, which is
    /// exactly why export synthesizes them.
    fn begin_item(&mut self, list: usize, want: Wrap) {
        if self.lists[list].wrap == want {
            return;
        }
        match self.lists[list].wrap {
            Wrap::Table => self.out.raw("</table>"),
            Wrap::List => self.out.raw("</ul>"),
            Wrap::None => {}
        }
        match want {
            Wrap::Table => self.out.raw("<table class=\"usfm-table\">"),
            Wrap::List => self.out.raw("<ul class=\"usfm-list\">"),
            Wrap::None => {}
        }
        self.lists[list].wrap = want;
    }

    // -- chapter / verse ---------------------------------------------------

    fn sid_for(&self, verse: bool, number: &str) -> Option<String> {
        let book = self.book.as_deref()?;
        if !verse {
            return Some(format!("{book} {number}"));
        }
        let chapter = self.chapter.as_deref()?;
        Some(format!("{book} {chapter}:{number}"))
    }

    /// Writes the held-back chapter or verse. Chapter and verse are MARKERS, not
    /// headings (sketches/html-export.md, and the 2026-08-21 audit that dropped
    /// `c` from both the heading table and `Heading`): a `<span class="chapter-num">`
    /// and a `<sup class="verse-num">`, and the app's CSS decides how they show.
    /// The visible number is the SEQUENTIAL one; `data-pubnumber` carries the
    /// published override for a consumer that prefers it.
    fn flush_pending(&mut self, list: usize) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.begin_item(list, Wrap::None);
        let (tag, class, marker, kind) = if pending.verse {
            ("sup", "verse-num", "v", "verse")
        } else {
            ("span", "chapter-num", "c", "chapter")
        };
        self.out.raw("<");
        self.out.raw(tag);
        self.class(&[class, LIFTED], marker);
        self.out.data("marker", marker);
        self.out.data("usfm-type", kind);
        self.out
            .data("usfm-category", category_name(Category::ChapterVerse));
        self.out.data("number", &pending.number);
        if let Some(altnumber) = &pending.altnumber {
            self.out.data("altnumber", altnumber);
        }
        if let Some(pubnumber) = &pending.pubnumber {
            self.out.data("pubnumber", pubnumber);
        }
        if let Some(sid) = &pending.sid {
            self.out.data("sid", sid);
        }
        self.out.raw(">");
        self.out.text(&pending.number);
        self.out.raw("</");
        self.out.raw(tag);
        self.out.raw(">");
        let state = &mut self.lists[list];
        state.at_boundary = true;
        state.ws.clear();
    }

    // -- spans -------------------------------------------------------------

    fn span(&self, token: &Token) -> &'a str {
        text_at(self.source, token.start..token.end())
    }

    fn marker_name(&self, token: &Token) -> String {
        marker_name(self.source, token)
    }

    /// One node's text content, whitespace-canonicalized the same way an
    /// ordinary content run is — this is what a lift reads.
    fn node_text(&self, node: u32) -> String {
        let mut out = String::new();
        let opener = self.cst.nodes[node as usize].token;
        for idx in self.cst.in_order_of(node) {
            if idx == opener {
                continue;
            }
            let token = &self.tokens[idx as usize];
            match token.kind() {
                TokenKind::Text | TokenKind::Designator => out.push_str(self.span(token)),
                TokenKind::Newline => out.push(' '),
                _ => {}
            }
        }
        trim(&out).to_string()
    }
}

/// One resolved attribute's value by name.
fn value_of<'p>(pairs: &'p [(String, String)], name: &str) -> Option<&'p str> {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

// ---------------------------------------------------------------------------
// THE ZOO: one hand-checked case per mapping row (sketches/html-tables.md).
// ---------------------------------------------------------------------------
//
// No oracle exists for HTML (the scope ruling), so these strings ARE the pin.
// They are read through `brief`, which drops the two attributes every element
// carries — `data-usfm-type` and `data-usfm-category` — so a case shows only
// what it is about. `the_full_data_splat_is_on_every_element` below is the one
// test that reads the unabridged bytes, and it is what pins those two.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cst, lex};

    fn render(source: &str) -> String {
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        html(source.as_bytes(), &tokens, &cst)
    }

    /// `render`, minus the two always-present attributes — see the module note.
    fn brief(source: &str) -> String {
        let mut out = render(source);
        for key in [" data-usfm-type=\"", " data-usfm-category=\""] {
            while let Some(at) = out.find(key) {
                let end = out[at + key.len()..].find('"').expect("closed") + at + key.len() + 1;
                out.replace_range(at..end, "");
            }
        }
        out
    }

    #[test]
    fn the_full_data_splat_is_on_every_element() {
        // The always-emit convention (`HtmlElement`'s own doc): a consumer
        // restyles or remaps off these without needing a different element.
        assert_eq!(
            render("\\p x"),
            r#"<div class="usfm-p" data-marker="p" data-usfm-type="para" data-usfm-category="para-body">x</div>"#
        );
    }

    #[test]
    fn a_node_takes_its_rows_element_and_a_marker_class() {
        // `Span`, `Bold`, `Italic`, `Em`, `Sup` — the character rows whose
        // element the 2026-08-21 audit CONFIRMED.
        assert_eq!(
            brief("\\p plain \\nd LORD\\nd* \\bd b\\bd*\\it i\\it*\\em e\\em*\\sup s\\sup*"),
            r#"<div class="usfm-p" data-marker="p">plain <span class="usfm-nd" data-marker="nd">LORD</span><b class="usfm-bd" data-marker="bd">b</b><i class="usfm-it" data-marker="it">i</i><em class="usfm-em" data-marker="em">e</em><sup class="usfm-sup" data-marker="sup">s</sup></div>"#
        );
    }

    #[test]
    fn headings_come_off_the_audited_base_levels_table() {
        // mt 1, ms 2, is/iot/s 3, qa 4, `base + digit - 1` — and `s4` lands on
        // `<h6>` EXACTLY, the cap the audit called out.
        assert_eq!(
            brief(
                "\\mt1 T\n\\mt3 T3\n\\ms Major\n\\ms2 M2\n\\is Intro\n\\iot Outline\n\\s1 S\n\\s2 S2\n\\s4 S4\n\\qa A"
            ),
            r#"<h1 class="usfm-mt1" data-marker="mt1" data-level="1">T</h1><h3 class="usfm-mt3" data-marker="mt3" data-level="3">T3</h3><h2 class="usfm-ms" data-marker="ms" data-level="2">Major</h2><h3 class="usfm-ms2" data-marker="ms2" data-level="3">M2</h3><h3 class="usfm-is" data-marker="is" data-level="3">Intro</h3><h3 class="usfm-iot" data-marker="iot" data-level="3">Outline</h3><h3 class="usfm-s1" data-marker="s1" data-level="3">S</h3><h4 class="usfm-s2" data-marker="s2" data-level="4">S2</h4><h6 class="usfm-s4" data-marker="s4" data-level="6">S4</h6><h4 class="usfm-qa" data-marker="qa" data-level="4">A</h4>"#
        );
    }

    #[test]
    fn c_cp_and_cl_are_no_longer_headings() {
        // The three rows the 2026-08-21 audit removed from `HEADING_BASE_LEVEL`:
        // `\c` is a chapter-num span, `\cp` lifts to `data-pubnumber`, and `\cl`
        // is an ordinary paragraph. Not one `<h…>` in sight.
        let out = brief("\\c 1\n\\cp M\n\\cl Chapter\n\\p x");
        assert!(!out.contains("<h"), "{out}");
        assert_eq!(
            out,
            r#"<span class="chapter-num usfm-lifted usfm-c" data-marker="c" data-number="1" data-pubnumber="M">1</span><div class="usfm-cl" data-marker="cl">Chapter</div><div class="usfm-p" data-marker="p">x</div>"#
        );
    }

    #[test]
    fn text_escapes_three_characters_and_attribute_values_four() {
        assert_eq!(
            brief("\\p a<b> & c \\w x|lemma=\"A&B<>\"\\w*"),
            r#"<div class="usfm-p" data-marker="p">a&lt;b&gt; &amp; c <span class="usfm-w" data-marker="w" data-lemma="A&amp;B&lt;&gt;">x</span></div>"#
        );
        // The quote is the one that only an ATTRIBUTE has to escape.
        assert!(render("\\p \\w x|lemma=\"q\"\\w*").contains(r#"data-lemma="q""#));
    }

    #[test]
    fn the_caller_trio_numbers_per_kind_and_resets_per_book() {
        // `+` auto-numbers, and the footnote and cross-reference families count
        // SEPARATELY (`\x` gets 1 while `\f` is on 2); `-` renders no visible
        // caller at all; anything else renders the literal. Both counters reset
        // at the second `\id`.
        assert_eq!(
            brief(
                "\\id GEN\n\\p \\f + \\ft one\\f*\\f + \\ft two\\f*\\x + \\xt r\\x*\\f - \\ft q\\f*\\f * \\ft star\\f*\n\\id MAT\n\\p \\f + \\ft again\\f*"
            ),
            r#"<span class="usfm-id" data-marker="id" data-code="GEN"></span><div class="usfm-p" data-marker="p"><span class="note usfm-f" role="note" data-marker="f" data-caller="+" data-note-kind="footnote"><sup class="note-caller note-caller-generated usfm-lifted">1</sup><span class="usfm-ft" data-marker="ft">one</span></span><span class="note usfm-f" role="note" data-marker="f" data-caller="+" data-note-kind="footnote"><sup class="note-caller note-caller-generated usfm-lifted">2</sup><span class="usfm-ft" data-marker="ft">two</span></span><span class="note usfm-x" role="note" data-marker="x" data-caller="+" data-note-kind="crossref"><sup class="note-caller note-caller-generated usfm-lifted">1</sup><a class="usfm-xt" data-marker="xt">r</a></span><span class="note usfm-f" role="note" data-marker="f" data-caller="-" data-note-kind="footnote"><span class="usfm-ft" data-marker="ft">q</span></span><span class="note usfm-f" role="note" data-marker="f" data-caller="*" data-note-kind="footnote"><sup class="note-caller usfm-lifted">*</sup><span class="usfm-ft" data-marker="ft">star</span></span></div><span class="usfm-id" data-marker="id" data-code="MAT"></span><div class="usfm-p" data-marker="p"><span class="note usfm-f" role="note" data-marker="f" data-caller="+" data-note-kind="footnote"><sup class="note-caller note-caller-generated usfm-lifted">1</sup><span class="usfm-ft" data-marker="ft">again</span></span></div>"#
        );
    }

    #[test]
    fn a_note_is_a_span_and_never_an_aside() {
        // The MUST-CHANGE of the audit: `<aside>` is flow content and cannot sit
        // inside the `<p>` a caller lands in.
        let out = render("\\p mid\\f + \\ft n\\f*sentence");
        assert!(!out.contains("aside"), "{out}");
        assert!(
            out.contains(r#"<span class="note usfm-f" role="note""#),
            "{out}"
        );
    }

    #[test]
    fn milestones_are_empty_addressable_spans() {
        assert_eq!(
            brief("\\p \\ts-s |sid=\"x\"\\* a \\zaln-e\\*"),
            r#"<div class="usfm-p" data-marker="p"><span class="ms usfm-ts-s" data-marker="ts-s" data-sid="x"></span> a <span class="ms usfm-zaln-e" data-marker="zaln-e"></span></div>"#
        );
    }

    #[test]
    fn tables_are_real_tables_with_three_way_alignment() {
        // The `<table>` is SYNTHESIZED around the run of rows; `th…` cells are
        // `<th>`; and the suffix after the `tc`/`th` stem is the alignment
        // (`c` → center, which USJ and USX have nowhere to put).
        assert_eq!(
            brief("\\tr \\tc1 a\\tcc2 b\\tcr3 c\n\\tr \\th1 h\\thc2 hc\\thr3 hr\n\\p after"),
            r#"<table class="usfm-table"><tr class="usfm-tr" data-marker="tr"><td class="align-start usfm-tc1" data-marker="tc1" data-align="start">a</td><td class="align-center usfm-tcc2" data-marker="tcc2" data-align="center">b</td><td class="align-end usfm-tcr3" data-marker="tcr3" data-align="end">c</td></tr><tr class="usfm-tr" data-marker="tr"><th class="align-start usfm-th1" data-marker="th1" data-align="start">h</th><th class="align-center usfm-thc2" data-marker="thc2" data-align="center">hc</th><th class="align-end usfm-thr3" data-marker="thr3" data-align="end">hr</th></tr></table><div class="usfm-p" data-marker="p">after</div>"#
        );
    }

    #[test]
    fn a_run_of_list_items_gets_a_synthesized_ul() {
        // `\lh`/`\lf` are paragraphs and therefore end the run, which is exactly
        // where a list header and footer belong.
        assert_eq!(
            brief("\\lh head\n\\li one\n\\lim2 two\n\\lf foot"),
            r#"<div class="usfm-lh" data-marker="lh">head</div><ul class="usfm-list"><li class="usfm-li" data-marker="li">one</li><li class="usfm-lim2" data-marker="lim2">two</li></ul><div class="usfm-lf" data-marker="lf">foot</div>"#
        );
    }

    #[test]
    fn a_figure_is_an_img_plus_a_figcaption() {
        assert_eq!(
            brief("\\ip see \\fig caption|src=\"a.png\" size=\"col\" alt=\"A\"\\fig* end"),
            r#"<div class="usfm-ip" data-marker="ip">see <figure class="usfm-fig" data-marker="fig" data-src="a.png" data-size="col" data-alt="A"><img src="a.png" alt="A"><figcaption>caption</figcaption></figure> end</div>"#
        );
    }

    #[test]
    fn a_ruby_puts_its_gloss_in_an_rt_after_the_base() {
        // The gloss is an ATTRIBUTE, so its `<rt>` text is LIFTED; with no gloss
        // there is no `<rt>` at all.
        assert_eq!(
            brief("\\p \\rb BASE|gloss=\"g\"\\rb*\\rb X\\rb*"),
            r#"<div class="usfm-p" data-marker="p"><ruby class="usfm-rb" data-marker="rb" data-gloss="g">BASE<rt class="usfm-lifted">g</rt></ruby><ruby class="usfm-rb" data-marker="rb">X</ruby></div>"#
        );
    }

    #[test]
    fn a_sidebar_is_an_aside_and_cat_both_renders_and_lifts() {
        // `\esb` KEEPS `Aside` (it interrupts between paragraphs, where `<aside>`
        // is legal); `\cat` renders as the char-shaped span the audit gave it AND
        // copies to `data-category`, so it is marked LIFTED.
        assert_eq!(
            brief("\\esb \\cat People\\cat*\n\\p who\n\\esbe"),
            r#"<aside class="usfm-esb" data-marker="esb" data-category="People"><span class="usfm-lifted usfm-cat" data-marker="cat">People</span><div class="usfm-p" data-marker="p">who</div></aside>"#
        );
    }

    #[test]
    fn a_periph_renders_the_title_it_also_lifts() {
        assert_eq!(
            brief("\\periph Title|id=\"x\"\n\\p body"),
            r#"<section class="usfm-periph" data-marker="periph" data-alt="Title" data-id="x"><span class="periph-title usfm-lifted">Title</span><div class="usfm-p" data-marker="p">body</div></section>"#
        );
    }

    #[test]
    fn chapter_and_verse_are_markers_with_every_number_splatted() {
        assert_eq!(
            brief("\\id GEN\n\\c 1\n\\ca 2\\ca*\n\\cp M\n\\p \\v 1 \\va 3\\va* \\vp 1b\\vp* text"),
            r#"<span class="usfm-id" data-marker="id" data-code="GEN"></span><span class="chapter-num usfm-lifted usfm-c" data-marker="c" data-number="1" data-altnumber="2" data-pubnumber="M" data-sid="GEN 1">1</span><div class="usfm-p" data-marker="p"><sup class="verse-num usfm-lifted usfm-v" data-marker="v" data-number="1" data-altnumber="3" data-pubnumber="1b" data-sid="GEN 1:1">1</sup>text</div>"#
        );
    }

    #[test]
    fn an_orphan_lift_is_a_sup_of_its_own() {
        // No chapter to own it: the projection must not guess an owner, and `Sup`
        // is what the audited row says `\cp` is.
        assert_eq!(
            brief("\\p \\cp M"),
            r#"<div class="usfm-p" data-marker="p"><sup class="usfm-cp" data-marker="cp">M</sup></div>"#
        );
    }

    #[test]
    fn an_optbreak_is_a_br() {
        assert_eq!(
            brief("\\p a // b"),
            r#"<div class="usfm-p" data-marker="p">a <br class="usfm-optbreak"> b</div>"#
        );
    }

    #[test]
    fn an_unknown_marker_takes_the_block_or_the_milestone_spelling() {
        assert_eq!(
            brief("\\s5\n\\p a\\zms\\* b"),
            r#"<div class="usfm-s5" data-marker="s5"></div><div class="usfm-p" data-marker="p">a</div><span class="ms usfm-zms" data-marker="zms"></span> b"#
        );
    }

    #[test]
    fn the_nonpublishable_fields_keep_a_class_so_css_can_hide_them() {
        // The whole point of killing `Transparent` (audit §1): `\id`'s code,
        // `\usfm`'s version, `\rem`'s remark and the rest are addressable now.
        assert_eq!(
            brief(
                "\\id GEN Genesis\n\\usfm 3.1\n\\ide UTF-8\n\\h Gen\n\\toc1 The Book\n\\rem note\n\\sts draft"
            ),
            r#"<span class="usfm-id" data-marker="id" data-code="GEN">Genesis</span><span class="usfm-lifted usfm-usfm" data-marker="usfm" data-version="3.1">3.1</span><span class="usfm-ide" data-marker="ide">UTF-8</span><span class="usfm-h" data-marker="h">Gen</span><span class="usfm-toc1" data-marker="toc1">The Book</span><span class="usfm-rem" data-marker="rem">note</span><span class="usfm-sts" data-marker="sts">draft</span>"#
        );
    }

    #[test]
    fn b_and_ib_are_empty_divs() {
        assert_eq!(
            brief("\\q1 line\\b\n\\ib \n\\p x"),
            r#"<div class="usfm-q1" data-marker="q1">line</div><div class="usfm-b" data-marker="b"></div><div class="usfm-ib" data-marker="ib"></div><div class="usfm-p" data-marker="p">x</div>"#
        );
    }

    #[test]
    fn a_closed_span_nests_in_the_open_note_text() {
        // F3, the graft — the same projection USJ and USX make, spelled in HTML.
        assert_eq!(
            brief("\\p \\f + \\ft alpha \\xt ref\\xt* beta\\f*"),
            r#"<div class="usfm-p" data-marker="p"><span class="note usfm-f" role="note" data-marker="f" data-caller="+" data-note-kind="footnote"><sup class="note-caller note-caller-generated usfm-lifted">1</sup><span class="usfm-ft" data-marker="ft">alpha <a class="usfm-xt" data-marker="xt">ref</a> beta</span></span></div>"#
        );
    }

    #[test]
    fn whitespace_follows_usjs_rules() {
        // Every run is one space, a newline mid-paragraph is a space, and the
        // whitespace at a block seam is layout rather than text.
        assert_eq!(
            brief("\\p a\nb  \tc \\p d"),
            r#"<div class="usfm-p" data-marker="p">a b c</div><div class="usfm-p" data-marker="p">d</div>"#
        );
        // `~` is the non-breaking space it names.
        assert!(render("\\p a~b").contains("a\u{a0}b"));
    }

    #[test]
    fn attribute_names_are_sanitized_and_the_raw_spelling_kept() {
        assert_eq!(
            brief("\\p \\w x|X-Y=\"2\"\\w*"),
            r#"<div class="usfm-p" data-marker="p"><span class="usfm-w" data-marker="w" data-x-y="2" data-usfm-raw-attrs="X-Y=2">x</span></div>"#
        );
    }

    #[test]
    fn links_are_anchors() {
        assert_eq!(
            brief("\\p \\jmp text|href=\"#a\"\\jmp*\\ref Mark 1:4|MRK 1:4\\ref*"),
            r##"<div class="usfm-p" data-marker="p"><a class="usfm-jmp" data-marker="jmp" data-href="#a">text</a><a class="usfm-ref" data-marker="ref" data-loc="MRK 1:4">Mark 1:4</a></div>"##
        );
    }

    #[test]
    fn an_empty_document_is_an_empty_string() {
        assert_eq!(render(""), "");
    }
}
