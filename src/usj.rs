//! USJ export: the CST folded into usfm-grammar's JSON shape.
//!
//! ```text
//! \id GEN                        {"type":"USJ","version":"3.1","content":[
//! \c 1                             {"type":"book","marker":"id","code":"GEN","content":[]},
//! \p                               {"type":"chapter","marker":"c","number":"1","sid":"GEN 1"},
//! \v 1 verse one                   {"type":"para","marker":"p","content":[
//! \v 2 verse two                     {"type":"verse","marker":"v","number":"1","sid":"GEN 1:1"},
//!                                    "verse one ",
//!                                    {"type":"verse","marker":"v","number":"2","sid":"GEN 1:2"},
//!                                    "verse two"]}]}
//! ```
//!
//! ```text
//! \p \w gracious|lemma="grace"\w*    {"type":"char","marker":"w","lemma":"grace",
//!                                     "content":["gracious"]}
//! \c 1 \ca 2\ca* \cp M              chapter + altnumber:"2" + pubnumber:"M"
//! \f + \ft note\f*                  {"type":"note","marker":"f","caller":"+",…}
//! \qt-s |who="Pilate"\*             {"type":"ms","marker":"qt-s","who":"Pilate"}
//! \tr \tc1 a\tcr2 b                 {"type":"table","content":[{"type":"table:row",…
//!                                     {"type":"table:cell","marker":"tcr2","align":"end",…
//! ```
//!
//! This is THE LOSSY VIEW (settled-facts: "Exports are folds over the CST").
//! The token stream stays byte-identical; everything below — attribute key
//! order, quote style, content lifted to attributes, whitespace
//! canonicalization — is thrown away HERE and nowhere upstream. The fold never
//! feeds back into the core and never repairs: damage is lint's answer, so no
//! `unmatched` element is ever emitted (sketches/usj-export.md).
//!
//! # What the projection changes
//!
//! - **Content → attribute lifts**, the one table that cannot be a marker-row
//!   column (usx.md's rule): `\ca`/`\va` → `altnumber`, `\cp`/`\vp` →
//!   `pubnumber`, `\cat` → `category` on the enclosing note/sidebar, a note's
//!   caller token → `caller`, and `\periph My Title|id="x"`'s title → `alt`
//!   (usx.rng writes it as an attribute; the pipe carries `id`).
//! - **One attribute RENAME**: `\fig`'s `src` is USJ's `file`.
//! - **`\usfm` is dropped** — the envelope's `version` is USJ's own.
//! - **Milestones keep the author's spelling** (`qt-s`, `zaln-e`), and the
//!   U25003 `\list-s`/`\table-s` CONTAINER is not an element at all: its
//!   points emit inline where they sit and its children land beside them.
//! - **A `table` wrapper is SYNTHESIZED** around each run of consecutive
//!   rows — the only element in the output with no marker behind it.
//! - **A note's closed spans are RE-PARENTED** (F3, below).
//!
//! # Inside a note: the graft (RULED 2026-08-20)
//!
//! ```text
//! \ft alpha \xt ref\xt* beta   CST: note{ ft["alpha"], xt["ref"], "beta" }
//!                              USJ: note{ ft["alpha ", xt["ref"], " beta"] }
//! \xo 1.1 \xt Ps 135\x*        CST: note{ xo["1.1 "], xt["Ps 135"] }
//!                              USJ: the same — two PEERS, no closer, no graft
//! ```
//!
//! The CST keeps the flat peer reading; the graft is projection-only and lives
//! entirely in [`Export::grafts`]. Two things must both be true of the sibling
//! for it to nest: it supplied its OWN closer, and it is not one of this note
//! family's peer markers ([`FOOTNOTE_PEERS`], [`XREF_PEERS`]) — `\xo 1.1 \xop
//! L\xop*` and `\ft … \fqa …\fqa*` are closed and still peers.
//!
//! # Whitespace (RULED: testData's canonicalization, so the oracle is exact)
//!
//! Four rules, each read off the fixtures rather than reasoned from the prose:
//!
//! 1. **Every whitespace run collapses to ONE space** — `"son of  david"` is
//!    `"son of david"`, and a newline mid-paragraph is a space
//!    (`"verse one "`). Outside four known-quirky fixtures, no testData string
//!    holds a tab, a newline, or two spaces in a row.
//! 2. **A DELIMITER is not content**: the space a marker folds after its own
//!    name, the space after a designator (`\v 1␠`), a book code (`\id GEN␠`)
//!    or a note caller, and any whitespace at the very start of an element's
//!    content.
//! 3. **At a BLOCK SEAM the whitespace is dropped**: in front of a paragraph,
//!    a chapter, a table row, a sidebar, a periph, an unknown marker, or the
//!    end of input. In front of anything inline — a verse, a character
//!    marker, a note, a milestone, a closer — it is TEXT. That is the whole
//!    difference between `"1.1: "` (kept, `\ft` follows) and `"something"`
//!    (dropped, `\p` follows).
//! 4. **A whitespace-only run is not content at all** — the 236-fixture
//!    corpus holds no whitespace-only string, and that is the pin.
//!
//! `~` becomes the non-breaking space it names, and `//` its own `optbreak`.

use core::ops::Range;
use std::borrow::Cow;

use crate::attributes::{AttrEvent, AttrResolution, attrs, resolve};
use crate::cst::{CloseReason, Cst, NODE_ID_BIT, Node, ROOT_TOKEN, container_kind};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind};

/// USJ's own envelope version. testData and scripture-editors' `USJ_VERSION`
/// agree on "3.1"; usfmtc's "3.0" was overruled by the fixtures.
const VERSION: &str = "3.1";

/// The `content` key, named because the `\b` rewind measures it.
const CONTENT: &str = ",\"content\":[";

/// A FOOTNOTE's own PEER markers — the note-text elements that sit BESIDE each
/// other inside `\f`/`\fe`/`\ef`. `\fv` is deliberately absent: the spec writes
/// it `\fv ...\fv*` (an embedded verse number INSIDE footnote text), and
/// specExamples/footnote reads it that way in both its `origin.json` and its
/// `origin.xml`. `\fm` is absent for the reason its row already states — it is
/// char-like, not a note peer.
const FOOTNOTE_PEERS: &[&str] = &["fdc", "fk", "fl", "fp", "fq", "fqa", "fr", "ft", "fw"];

/// A CROSS-REFERENCE's own peer markers, inside `\x`/`\ex`. `\xt` is one of
/// THESE and not a footnote's, which is the whole of why `\xo 1.1 \xt Ps 135`
/// siblings while `\ft … \xt ref\xt*` nests.
const XREF_PEERS: &[&str] = &["xdc", "xk", "xnt", "xo", "xop", "xot", "xq", "xt", "xta"];

/// Which peer family a note marker belongs to. `\x`/`\ex` are the
/// cross-reference notes; `\f`/`\fe`/`\ef` are the footnotes.
fn note_peers(marker: &str) -> &'static [&'static str] {
    match marker {
        "x" | "ex" => XREF_PEERS,
        _ => FOOTNOTE_PEERS,
    }
}

/// Folds a lexed + built document into USJ JSON.
///
/// `tokens` must be `lex(source)`'s output and `cst` must be
/// [`crate::cst::build`]'s over those tokens — the fold reads token spans out
/// of `source` and trusts the CST's shape.
pub fn usj(source: &[u8], tokens: &[Token], cst: &Cst) -> String {
    let mut export = Export {
        source,
        tokens,
        cst,
        json: Json {
            out: String::with_capacity(source.len() * 2),
        },
        lists: Vec::new(),
        book: None,
        chapter: None,
        pending: None,
        absorb: None,
        cursor: 0,
    };
    export.run();
    export.json.out
}

// ---------------------------------------------------------------------------
// The writer
// ---------------------------------------------------------------------------

/// The hand-rolled JSON writer: a `String` and one escaper. USJ is a small
/// CLOSED shape (a dozen element types, known keys), so serde would buy
/// generality nothing here needs — and the library stays dependency-free.
struct Json {
    out: String,
}

impl Json {
    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// One JSON string, quotes included. Escapes exactly what RFC 8259
    /// requires: `"`, `\`, and the C0 controls (as `\u00XX`, except the five
    /// with short forms). Everything else — every non-ASCII byte — passes
    /// through as UTF-8, which is what the fixtures hold.
    fn string(&mut self, text: &str) {
        self.out.push('"');
        for ch in text.chars() {
            match ch {
                '"' => self.out.push_str("\\\""),
                '\\' => self.out.push_str("\\\\"),
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                '\t' => self.out.push_str("\\t"),
                '\u{8}' => self.out.push_str("\\b"),
                '\u{c}' => self.out.push_str("\\f"),
                ch if (ch as u32) < 0x20 => {
                    self.out.push_str("\\u00");
                    const HEX: &[u8; 16] = b"0123456789abcdef";
                    let byte = ch as u32;
                    self.out.push(HEX[(byte >> 4) as usize] as char);
                    self.out.push(HEX[(byte & 0xf) as usize] as char);
                }
                ch => self.out.push(ch),
            }
        }
        self.out.push('"');
    }

    /// `"key":"value"` after a comma — every key in an element but `type`.
    fn field(&mut self, key: &str, value: &str) {
        self.raw(",");
        self.string(key);
        self.raw(":");
        self.string(value);
    }
}

// ---------------------------------------------------------------------------
// One content list
// ---------------------------------------------------------------------------

/// The state of ONE JSON content array while it is being written.
///
/// Its own stack rather than a frame field, because a TRANSPARENT node (a
/// U25003 container) writes into its PARENT's list: the frame stack and the
/// list stack have different depths on purpose.
#[derive(Default)]
struct ListState {
    /// An item has been written into this array (so the next needs a comma).
    sep: bool,
    /// The text run being accumulated between two elements.
    run: String,
    /// Whitespace read but not yet committed to `run`, held VERBATIM until
    /// what follows it is known. A whitespace run containing a NEWLINE
    /// collapses to one space; at a block seam (before a paragraph, a
    /// chapter, a table row, an explicit closer) the whole of it is dropped.
    /// Holding it is the only way to tell `"1.1: "` (kept, `\ft` follows)
    /// from `"something"` (dropped, `\p` follows) — the bytes are identical.
    ws: String,
    /// Leading whitespace here DELIMITS a payload and is not content: set at
    /// an element's content start, after a chapter/verse element, and after a
    /// book code, note caller or designator.
    at_boundary: bool,
    /// A synthesized `table` wrapper is open, and items go inside it.
    table_open: bool,
    table_sep: bool,
}

/// Space, tab, CR, LF — the scanner's structural whitespace, and deliberately
/// not `char::is_whitespace`: a no-break space in the text is CONTENT.
fn is_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn trim_start(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() && is_ws(bytes[at]) {
        at += 1;
    }
    &text[at..]
}

fn trim_end(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut to = bytes.len();
    while to > 0 && is_ws(bytes[to - 1]) {
        to -= 1;
    }
    &text[..to]
}

fn trim(text: &str) -> &str {
    trim_end(trim_start(text))
}

/// One text token, canonicalized the way the fixtures are: every run of
/// structural whitespace becomes ONE space (`"son of  david"` is
/// `"son of david"`), and `~` — USFM's non-breaking space — becomes the
/// character it names.
fn canonical(text: &str) -> Cow<'_, str> {
    // The common case is text that needs nothing done to it, and an export
    // that allocated per TEXT TOKEN would allocate once per word of scripture.
    let bytes = text.as_bytes();
    let untouched = !bytes.iter().enumerate().any(|(at, &byte)| {
        byte == b'~'
            || (is_ws(byte) && (byte != b' ' || bytes.get(at + 1).is_some_and(|next| is_ws(*next))))
    });
    if untouched {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for byte in text.chars() {
        match byte {
            ch if ch.is_ascii() && is_ws(ch as u8) => {
                if !in_ws {
                    out.push(' ');
                }
                in_ws = true;
            }
            '~' => {
                out.push('\u{a0}');
                in_ws = false;
            }
            ch => {
                out.push(ch);
                in_ws = false;
            }
        }
    }
    Cow::Owned(out)
}

// ---------------------------------------------------------------------------
// The fold
// ---------------------------------------------------------------------------
//
// The sketch drew this as a `Visit` trait with a shared `fold(doc, &mut impl
// Visit)` driver. It is a CONCRETE walker here, on purpose: the driver is
// twelve lines and USJ is its only implementor, so a trait today would be a
// generalization written against one example. USX and HTML are the second and
// third; the trait is worth extracting when the SHARED half is visible in two
// bodies at once, not before (settled-facts' net-deletion rule).

/// One frame of the driver's own stack — the same ChildCursor shape lint's
/// walk uses (no recursion), plus which content array this node writes into.
struct Frame {
    next: u32,
    end: u32,
    /// The node's own opening-marker token id, so the leaf loop can skip it
    /// (the walker files it as the node's first child).
    token: u32,
    /// Index into [`Export::lists`]. A transparent node shares its parent's.
    list: usize,
    /// This frame pushed the list it points at, and pops it at close.
    owns_list: bool,
    /// What this frame owes the writer when it ends: `"]}"` for an ordinary
    /// element and `""` for a TRANSPARENT node that opened no object at all.
    close: &'static str,
    /// Where `,"content":[` began, for the one element whose EMPTY form has
    /// no `content` key at all (`\b`): if nothing was written into the
    /// array, the writer rewinds here and closes the object instead.
    /// `usize::MAX` disables the rewind.
    rewind: usize,
    /// The one Text token this element LIFTED into a key instead of content —
    /// `\periph My Title|id="x"`, whose title becomes `alt`. `u32::MAX` when
    /// nothing was lifted (every element but `periph`).
    lifted_text: u32,
    /// This frame is a NOTE element: its own family's PEER markers, so the
    /// graft rule below can tell a peer from an inline span. `None` for every
    /// frame that is not a note.
    peers: Option<&'static [&'static str]>,
    /// This frame is an UNCLOSED note-text element (`\ft`, `\fqa`, `\xo`, …),
    /// so an explicitly-closed sibling GRAFTS into it instead of sealing it.
    adopts: bool,
    /// It has already grafted one span, so the direct note content that
    /// FOLLOWS that span resumes this element rather than starting a new one.
    grafted: bool,
}

/// A chapter or verse element held back until its `\ca`/`\cp`/`\va`/`\vp`
/// annotations are known — they FOLLOW the marker they decorate, so the
/// element cannot be written the moment it is seen.
struct Pending {
    verse: bool,
    number: String,
    altnumber: Option<String>,
    pubnumber: Option<String>,
    sid: Option<String>,
}

/// Which lifted slot the next payload token belongs to. `\cp` and `\vp` are
/// LEAVES (no scope, no closer), so their value arrives as the following
/// token rather than as node content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Absorb {
    AltNumber,
    PubNumber,
}

struct Export<'a> {
    source: &'a [u8],
    tokens: &'a [Token],
    cst: &'a Cst,
    json: Json,
    lists: Vec<ListState>,
    book: Option<String>,
    chapter: Option<String>,
    pending: Option<Pending>,
    absorb: Option<Absorb>,
    /// The next token the walk will deliver — how a node close asks what
    /// comes AFTER it without looking at the stack.
    cursor: u32,
}

impl<'a> Export<'a> {
    fn run(&mut self) {
        self.json.raw("{\"type\":\"USJ\",\"version\":");
        self.json.string(VERSION);
        self.json.raw(",\"content\":[");

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
            rewind: usize::MAX,
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
                // the direct note content that follows one), one child at a
                // time. Re-pointing this frame's cursor into the PARENT's range
                // is the whole mechanism; the child lists are separate regions
                // of `child_ids`, so there is nothing to extend.
                if cur.adopts && stack.last().is_some_and(|parent| self.grafts(parent, &cur)) {
                    let parent = stack.last_mut().expect("just checked");
                    let at = parent.next;
                    parent.next += 1;
                    cur.next = at;
                    cur.end = at + 1;
                    cur.grafted |= self.cst.child_ids[at as usize] & NODE_ID_BIT != 0;
                    continue;
                }
                self.close_frame(&cur);
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

        self.json.raw("]}");
    }

    // -- leaves ------------------------------------------------------------

    fn leaf(&mut self, idx: u32, frame: &Frame) {
        self.cursor = idx + 1;
        if idx == frame.token {
            return; // the node's own opening marker
        }
        if idx == frame.lifted_text {
            // Already written as a key at this element's open (`\periph`'s
            // title → `alt`); like the book code and the note caller it only
            // re-arms the delimiter rule for what follows.
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
                self.begin_item(list, false);
                self.json.raw("{\"type\":\"optbreak\"}");
                self.lists[list].at_boundary = false;
            }
            // Both were already lifted at their node's open (the `code` and
            // `caller` keys precede `content`); here they only re-arm the
            // delimiter rule for the text that follows them.
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
            // An orphan closer or `\*`, an attribute list already read by its
            // owner: none of them are content.
            // Whitespace in FRONT of an explicit closer is CONTENT — `\k
            // ostrich \k*bird` keeps "ostrich " — so the closer commits it
            // rather than letting the element's close decide.
            TokenKind::ClosingMarker { .. } => self.commit_ws(list),
            TokenKind::MilestoneTerminator => {
                self.lists[list].ws.clear();
            }
            TokenKind::AttrList => {}
            TokenKind::Milestone { .. } => {}
        }
    }

    /// A marker that opened no node: `\c`, `\v`, the lifted leaves, an
    /// unknown marker, and the bare-`\*` milestone spelling.
    fn marker_leaf(&mut self, idx: u32, token: &Token, list: usize) {
        let marker = self.marker_name(token);
        if token.marker_idx == generated::UNRESOLVED {
            // `\zms\*`: the `\*` SPELLING says milestone, whatever the row
            // does not know. Otherwise an unknown marker in paragraph
            // position takes usfm-grammar's para shape.
            let milestone = matches!(
                self.tokens.get(idx as usize + 1).map(Token::kind),
                Some(TokenKind::MilestoneTerminator)
            );
            self.flush_pending(list);
            self.seal_run(list, !milestone);
            self.begin_item(list, false);
            if milestone {
                self.json.raw("{\"type\":\"ms\",\"marker\":");
                self.json.string(&marker);
                self.json.raw("}");
            } else {
                self.json.raw("{\"type\":\"para\",\"marker\":");
                self.json.string(&marker);
                self.json.raw(",\"content\":[]}");
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
                "ca" => self.absorb = Some(Absorb::AltNumber),
                "cp" | "vp" => self.absorb = Some(Absorb::PubNumber),
                "va" => self.absorb = Some(Absorb::AltNumber),
                // Any other markerless leaf (`\usfm`'s payload never gets
                // here, a `\pb` opens its own node) is not an element.
                _ => {}
            },
        }
    }

    /// Puts a lifted value on the pending chapter/verse. With no pending
    /// owner the projection must not guess one, so the value becomes an
    /// ordinary `char` element and lint carries the placement complaint.
    fn lift(&mut self, slot: Absorb, value: String, list: usize) {
        match (&mut self.pending, slot) {
            (Some(pending), Absorb::AltNumber) => pending.altnumber = Some(value),
            (Some(pending), Absorb::PubNumber) => pending.pubnumber = Some(value),
            (None, _) => {
                self.seal_run(list, false);
                self.begin_item(list, false);
                let marker = match slot {
                    Absorb::AltNumber => "ca",
                    Absorb::PubNumber => "cp",
                };
                self.json.raw("{\"type\":\"char\",\"marker\":");
                self.json.string(marker);
                self.json.raw(",\"content\":[");
                self.json.string(&value);
                self.json.raw("]}");
                self.lists[list].at_boundary = false;
            }
        }
    }

    // -- the note graft (F3) -----------------------------------------------

    /// Does the note's next child GRAFT into `open`, the note-text element that
    /// just ran out of children?
    ///
    /// RULED 2026-08-20: inside a note the EXPLICIT CLOSER is the
    /// discriminator, and it is a PROJECTION rule — the CST keeps its flat peer
    /// reading and this fold re-parents.
    ///
    /// The closer is NECESSARY BUT NOT SUFFICIENT (measured, not reasoned: on
    /// the closer alone, five passing fixtures regress). A closed marker that is
    /// one of THIS note family's own peers is still a peer — `\xo 1.1 \xop
    /// L\xop*` and `\ft … \fqa …\fqa*` are both written with their closers and
    /// both sibling. Hence the second half of the test below.
    ///
    /// ```text
    /// \ft alpha \xt ref\xt* beta    CST:  note{ ft["alpha"], xt["ref"], "beta" }
    ///                               USJ:  note{ ft["alpha ", xt["ref"], " beta"] }
    /// \xo 1.1 \xt Ps 135\x*         CST:  note{ xo["1.1 "], xt["Ps 135"] }
    ///                               USJ:  the same — UNCLOSED `\xt` stays a PEER
    /// ```
    ///
    /// So: an explicitly-closed char sibling is an INLINE SPAN and grafts; the
    /// direct note content after one RESUMES the element it grafted into; an
    /// UNCLOSED note-content marker, the note's end, and anything that is not
    /// note content all seal as before.
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
        // Text after a grafted span is the RESUMPTION of the open element. With
        // nothing grafted yet there is nothing to resume — the closer of an
        // element that WAS explicitly closed (`\ft x\ft* more`) must not pull
        // the note's own text back inside it.
        open.grafted
            && matches!(
                self.tokens[child as usize].kind(),
                TokenKind::Text | TokenKind::Newline | TokenKind::OptBreak
            )
    }

    // -- nodes -------------------------------------------------------------

    /// Opens one child node. Returns the frame to descend into, or `None`
    /// when the node emits nothing at all and its subtree is skipped.
    fn open_node(&mut self, id: u32, parent: &Frame) -> Option<Frame> {
        let node = &self.cst.nodes[id as usize];
        let token = &self.tokens[node.token as usize];
        let marker_idx = token.marker_idx;
        let marker = self.marker_name(token);
        let list = parent.list;

        // A U25003 container (`\list-s … \list-e`) is walker bookkeeping, not
        // an element: its points emit inline where they sit and its children
        // land beside them. Its first child is the `-s` POINT node, which is
        // how it is told apart from that point (whose first child is the
        // token).
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
                rewind: usize::MAX,
                lifted_text: u32::MAX,
                peers: None,
                adopts: false,
                grafted: false,
            });
        }

        // The lifted markers emit NOTHING: their content becomes an
        // attribute somewhere else, and they must not break the text run
        // around them either (`\v 1 \va 3\va* text` is one delimiter).
        match marker.as_str() {
            "ca" | "va" => {
                let value = self.node_text(id);
                self.lift(Absorb::AltNumber, value, list);
                return None;
            }
            "cp" | "vp" => {
                let value = self.node_text(id);
                self.lift(Absorb::PubNumber, value, list);
                return None;
            }
            // `\cat` was already read by the enclosing note/sidebar's open,
            // and `\usfm` is dropped outright (the envelope's version is
            // USJ's own).
            "cat" | "usfm" => return None,
            _ => {}
        }

        let kind = generated::kind(marker_idx);
        // F3: a note-text element that supplied NO closer of its own is the one
        // an explicitly-closed sibling grafts into (see `grafts`).
        let adopts = parent.peers.is_some()
            && kind == MarkerKind::Character
            && node.close_reason() != CloseReason::Explicit;
        let peers = (kind == MarkerKind::Note).then(|| note_peers(&marker));
        let is_milestone = matches!(token.kind(), TokenKind::Milestone { .. })
            || kind == MarkerKind::Milestone
            || marker_idx == generated::UNRESOLVED;
        let is_row = kind == MarkerKind::TableRow;

        self.flush_pending(list);
        self.seal_run(list, self.seam_at(Some(node.token)));
        self.begin_item(list, is_row);

        // A milestone POINT carries attributes and no content at all.
        if is_milestone {
            self.json.raw("{\"type\":\"ms\",\"marker\":");
            self.json.string(&marker);
            self.write_attrs(node, marker_idx, &marker);
            self.json.raw("}");
            self.lists[list].at_boundary = false;
            return None;
        }

        let element = match kind {
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
            // Chapter/Verse never open a scope, Milestone went above, Meta is
            // only `\cat`, Unknown is row 0 (handled as a leaf). A row that
            // opened a scope and lands here is a table bug, not damage in the
            // document — take the para shape and let lint speak.
            _ => "para",
        };

        self.json.raw("{\"type\":");
        self.json.string(element);
        if element != "ref" && element != "periph" {
            self.json.raw(",\"marker\":");
            self.json.string(&marker);
        }

        // `\id`'s book code and a note's caller are both LIFTED out of
        // content, and both keys precede it — so they are read here, off the
        // node's direct children, rather than patched in later.
        if element == "book"
            && let Some(code) = self.payload_child(node, TokenKind::BookCode)
        {
            self.book = Some(code.clone());
            self.json.field("code", &code);
        }
        if kind == MarkerKind::Note
            && let Some(caller) = self.payload_child(node, TokenKind::NoteCaller)
        {
            self.json.field("caller", &caller);
        }
        if let Some(category) = self.category(node) {
            self.json.field("category", &category);
        }
        // `\periph My Title|id="x"`: the TITLE TEXT is the `alt` attribute
        // (usx.rng's `PeripheralDivision` writes it as one), so it is lifted
        // out of content the way `\id`'s code and a note's caller are. It is
        // the node's first Text child — everything after the attribute list
        // is the division's paragraphs.
        let mut lifted_text = u32::MAX;
        if kind == MarkerKind::Periph {
            let title = self
                .direct_children(node)
                .find(|child| self.tokens[*child as usize].kind() == TokenKind::Text);
            if let Some(title) = title {
                let value = trim(self.span(&self.tokens[title as usize])).to_string();
                self.json.field("alt", &value);
                lifted_text = title;
            }
        }
        if kind == MarkerKind::TableCell {
            // `tcr`/`thr` vs `tc`/`th`: the `r` in the ROW NAME is the whole
            // of the alignment fact.
            let align = if generated::name(marker_idx).ends_with('r') {
                "end"
            } else {
                "start"
            };
            self.json.field("align", align);
        }
        self.write_attrs(node, marker_idx, &marker);
        // `\b` is the ONE element whose EMPTY form the fixtures write with
        // no `content` key at all (83 of them; every other empty element,
        // `\p` included, keeps `content: []`) — while its NON-empty form
        // (`\b` then `\v 18 …`, specExamples/poetry) keeps it. So the array
        // is written speculatively and REWOUND if nothing lands in it.
        let rewind = if marker == "b" {
            self.json.out.len()
        } else {
            usize::MAX
        };
        self.json.raw(CONTENT);

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
            close: "]}",
            rewind,
            lifted_text,
            peers,
            adopts,
            grafted: false,
        })
    }

    fn close_frame(&mut self, frame: &Frame) {
        let list = frame.list;
        self.flush_pending(list);
        // What ends this element decides its trailing whitespace: the next
        // token in DOCUMENT order, which the cursor is already sitting on.
        self.seal_run(list, self.seam_at(Some(self.cursor)));
        if self.lists[list].table_open {
            self.json.raw("]}");
            self.lists[list].table_open = false;
        }
        if frame.rewind != usize::MAX && self.json.out.len() == frame.rewind + CONTENT.len() {
            self.json.out.truncate(frame.rewind);
            self.json.raw("}");
        } else {
            self.json.raw(frame.close);
        }
        if frame.owns_list {
            self.lists.pop();
            // The element just written is an item of its PARENT's list, and
            // the parent's boundary rule ends with it.
            if let Some(parent) = self.lists.last_mut() {
                parent.at_boundary = false;
            }
        }
    }

    // -- attributes --------------------------------------------------------

    /// Splats one node's OWN attribute lists into keys. Later definition
    /// wins (the interpreter's merge rule); key order, `=` spacing and quote
    /// style are the documented lossy step.
    fn write_attrs(&mut self, node: &Node, marker_idx: MarkerIdx, marker: &str) {
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
                // The one PER-FORMAT RENAME: `\fig`'s `src` is USJ's `file`.
                let name = if marker == "fig" && name == "src" {
                    "file".to_string()
                } else {
                    name
                };
                let value = String::from_utf8_lossy(attr.value).into_owned();
                match pairs.iter_mut().find(|(key, _)| *key == name) {
                    Some(slot) => slot.1 = value,
                    None => pairs.push((name, value)),
                }
            }
        }
        for (name, value) in &pairs {
            self.json.field(name, value);
        }
    }

    /// A carved payload token's text (`\id`'s book code, a note's caller),
    /// read off the node's DIRECT children so the key can be written before
    /// `content` opens.
    fn payload_child(&self, node: &Node, kind: TokenKind) -> Option<String> {
        self.direct_children(node)
            .find(|child| self.tokens[*child as usize].kind() == kind)
            .map(|child| trim(self.span(&self.tokens[child as usize])).to_string())
    }

    /// A `\cat` child's content, lifted to `category` on this note/sidebar —
    /// read BEFORE the element's `content` opens, which is why it is a
    /// prescan and not a patch.
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
        // EVERY whitespace run collapses to ONE space, and two runs that
        // meet across a lifted element collapse together. Read off the
        // corpus: outside four known-quirky fixtures, no testData string
        // holds a tab, a newline, or two spaces in a row.
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
        // held-back chapter/verse element out before its `\ca`/`\va`
        // annotations have been read.
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
    /// layout, not content?
    ///
    /// Paragraphs, chapters, table rows, sidebars and periphs start a new
    /// block, and end of input ends the last one. Verses, characters, notes,
    /// figures and milestones do NOT: they sit inside a line, and the space
    /// in front of them is text (`"1.1: "`, `"Day "`, `"verse one "`).
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

    /// Writes the accumulated run as one JSON string. A run that is
    /// whitespace only is DROPPED — the whole 236-fixture corpus holds no
    /// whitespace-only string, which is the pin behind this line.
    fn flush_run(&mut self, list: usize) {
        let run = core::mem::take(&mut self.lists[list].run);
        if run.is_empty() || run.bytes().all(is_ws) {
            return;
        }
        self.begin_item(list, false);
        self.json.string(&run);
        self.lists[list].at_boundary = false;
    }

    /// Comma bookkeeping, plus the SYNTHESIZED `table` wrapper: it opens at
    /// the first of a run of consecutive rows and closes at the first item
    /// that is not one.
    fn begin_item(&mut self, list: usize, is_row: bool) {
        let state = &mut self.lists[list];
        if is_row && !state.table_open {
            if state.sep {
                self.json.raw(",");
            }
            self.json.raw("{\"type\":\"table\",\"content\":[");
            let state = &mut self.lists[list];
            state.sep = true;
            state.table_open = true;
            state.table_sep = false;
        } else if !is_row && state.table_open {
            self.json.raw("]}");
            self.lists[list].table_open = false;
        }
        let state = &mut self.lists[list];
        if state.table_open {
            let sep = state.table_sep;
            state.table_sep = true;
            if sep {
                self.json.raw(",");
            }
        } else {
            let sep = state.sep;
            state.sep = true;
            if sep {
                self.json.raw(",");
            }
        }
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

    fn flush_pending(&mut self, list: usize) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.begin_item(list, false);
        self.json.raw("{\"type\":");
        self.json
            .string(if pending.verse { "verse" } else { "chapter" });
        self.json.raw(",\"marker\":");
        self.json.string(if pending.verse { "v" } else { "c" });
        self.json.field("number", &pending.number);
        if let Some(altnumber) = &pending.altnumber {
            self.json.field("altnumber", altnumber);
        }
        if let Some(pubnumber) = &pending.pubnumber {
            self.json.field("pubnumber", pubnumber);
        }
        if let Some(sid) = &pending.sid {
            self.json.field("sid", sid);
        }
        self.json.raw("}");
        let state = &mut self.lists[list];
        state.at_boundary = true;
        state.ws.clear();
    }

    // -- spans -------------------------------------------------------------

    fn span(&self, token: &Token) -> &'a str {
        self.text(token.start..token.end())
    }

    fn text(&self, range: Range<u32>) -> &'a str {
        let bytes = &self.source[range.start as usize..range.end as usize];
        // The scanner never splits a UTF-8 sequence, so this only ever fails
        // on a source that was not UTF-8 to begin with.
        core::str::from_utf8(bytes).unwrap_or("")
    }

    /// The marker as the AUTHOR spelled it: `q1`, `qt-s`, `zaln-e` — read off
    /// the token's own bytes, not off the row (numbered markers share one
    /// row, and milestones share one row with both halves of the pair).
    fn marker_name(&self, token: &Token) -> String {
        let text = self.span(token);
        let text = text.strip_prefix('\\').unwrap_or(text);
        let text = text.strip_prefix('+').unwrap_or(text);
        let text = trim_end(text);
        let text = text.strip_suffix('*').unwrap_or(text);
        trim_end(text).to_string()
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

// ---------------------------------------------------------------------------
// THE ZOO: one hand-written case per mapping row (sketches/usj-export.md).
// ---------------------------------------------------------------------------
//
// These pin INTENT independently of testData: the corpus test
// (tests/usj_corpus.rs) is the oracle, and it compares as `serde_json::Value`,
// so it forgives key order. The zoo compares the STRING, which is what pins the
// writer's fixed key order (type, marker, attrs, number/sid, content last) as
// well as the shape.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cst, lex};

    /// `source` → its USJ, with the envelope stripped so a case reads as just
    /// the content it is about.
    fn content(source: &str) -> String {
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        let out = usj(source.as_bytes(), &tokens, &cst);
        let head = "{\"type\":\"USJ\",\"version\":\"3.1\",\"content\":[";
        let body = out
            .strip_prefix(head)
            .and_then(|rest| rest.strip_suffix("]}"))
            .unwrap_or_else(|| panic!("envelope missing from {out}"));
        body.to_string()
    }

    #[test]
    fn the_envelope_carries_usj_3_1() {
        assert_eq!(
            usj(b"", &lex(""), &cst::build(&lex(""))),
            "{\"type\":\"USJ\",\"version\":\"3.1\",\"content\":[]}"
        );
    }

    #[test]
    fn book_keeps_its_code_and_an_empty_content_array() {
        assert_eq!(
            content("\\id GEN"),
            r#"{"type":"book","marker":"id","code":"GEN","content":[]}"#
        );
        assert_eq!(
            content("\\id GEN Genesis"),
            r#"{"type":"book","marker":"id","code":"GEN","content":["Genesis"]}"#
        );
    }

    #[test]
    fn paragraph_markers_keep_their_own_spelling() {
        // The row is shared (`q1` resolves to `q`, `mt2` to `mt`); the
        // NUMBER is the author's and comes off the token's bytes.
        assert_eq!(
            content("\\q1 line\n\\mt2 title\n\\ide UTF-8"),
            r#"{"type":"para","marker":"q1","content":["line"]},"#.to_owned()
                + r#"{"type":"para","marker":"mt2","content":["title"]},"#
                + r#"{"type":"para","marker":"ide","content":["UTF-8"]}"#
        );
    }

    #[test]
    fn b_is_the_one_element_whose_empty_form_drops_content() {
        assert_eq!(
            content("\\p one\\b\\p two"),
            r#"{"type":"para","marker":"p","content":["one"]},"#.to_owned()
                + r#"{"type":"para","marker":"b"},"#
                + r#"{"type":"para","marker":"p","content":["two"]}"#
        );
        // …and its NON-empty form keeps it (specExamples/poetry: a `\v` whose
        // context mask allows Para lands inside the `\b`).
        assert_eq!(
            content("\\b\n\\v 1 x"),
            r#"{"type":"para","marker":"b","content":[{"type":"verse","marker":"v","number":"1"},"x"]}"#
        );
    }

    #[test]
    fn characters_nest_and_keep_the_nested_spelling_off_the_row() {
        assert_eq!(
            content("\\p \\add outer \\+nd inner\\+nd* tail\\add*"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"add","content":["outer ",{"type":"char","marker":"nd","content":["inner"]}," tail"]}]}"#
        );
    }

    #[test]
    fn a_note_lifts_its_caller_out_of_content() {
        assert_eq!(
            content("\\p \\f + \\fr 1:1 \\ft note\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"fr","content":["1:1 "]},{"type":"char","marker":"ft","content":["note"]}]}]}"#
        );
    }

    #[test]
    fn chapter_and_verse_are_elements_with_sids_and_no_content() {
        assert_eq!(
            content("\\id GEN\n\\c 2\n\\p \\v 3-4 x"),
            r#"{"type":"book","marker":"id","code":"GEN","content":[]},"#.to_owned()
                + r#"{"type":"chapter","marker":"c","number":"2","sid":"GEN 2"},"#
                + r#"{"type":"para","marker":"p","content":[{"type":"verse","marker":"v","number":"3-4","sid":"GEN 2:3-4"},"x"]}"#
        );
        // No `\id`, no book code, so no sid to derive — the projection never
        // invents one.
        assert_eq!(
            content("\\c 1"),
            r#"{"type":"chapter","marker":"c","number":"1"}"#
        );
    }

    #[test]
    fn ca_cp_va_vp_lift_onto_the_chapter_and_the_verse() {
        assert_eq!(
            content("\\c 1\n\\ca 2\\ca*\n\\cp M\n\\p \\v 1 \\va 3\\va* \\vp 1b\\vp* text"),
            r#"{"type":"chapter","marker":"c","number":"1","altnumber":"2","pubnumber":"M"},"#
                .to_owned()
                + r#"{"type":"para","marker":"p","content":[{"type":"verse","marker":"v","number":"1","altnumber":"3","pubnumber":"1b"},"text"]}"#
        );
    }

    #[test]
    fn an_orphan_lift_stays_an_ordinary_char_element() {
        // No chapter to own it: the projection must not guess an owner, and
        // lint carries the placement complaint.
        assert_eq!(
            content("\\p \\cp M"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"cp","content":["M"]}]}"#
        );
    }

    #[test]
    fn milestones_keep_their_spelling_and_own_no_content() {
        assert_eq!(
            content("\\p \\ts-s |sid=\"x\"\\* a \\ts\\* \\zaln-e\\*"),
            r#"{"type":"para","marker":"p","content":[{"type":"ms","marker":"ts-s","sid":"x"}," a ",{"type":"ms","marker":"ts"},{"type":"ms","marker":"zaln-e"}]}"#
        );
    }

    #[test]
    fn a_u25003_container_emits_its_points_and_not_itself() {
        assert_eq!(
            content("\\list-s\\*\n\\li one\n\\list-e\\*"),
            r#"{"type":"ms","marker":"list-s"},"#.to_owned()
                + r#"{"type":"para","marker":"li","content":["one "]},"#
                + r#"{"type":"ms","marker":"list-e"}"#
        );
    }

    #[test]
    fn consecutive_rows_share_one_synthesized_table() {
        // Two rows → ONE wrapper; the `\p` splits the run, so the third row
        // gets a wrapper of its own. `align` is the `r` in the ROW name.
        assert_eq!(
            content("\\tr \\tc1 a\\tcr2 b\n\\tr \\tc1 c\n\\p after\n\\tr \\th1 h"),
            r#"{"type":"table","content":[{"type":"table:row","marker":"tr","content":[{"type":"table:cell","marker":"tc1","align":"start","content":["a"]},{"type":"table:cell","marker":"tcr2","align":"end","content":["b"]}]},{"type":"table:row","marker":"tr","content":[{"type":"table:cell","marker":"tc1","align":"start","content":["c"]}]}]},"#.to_owned()
                + r#"{"type":"para","marker":"p","content":["after"]},"#
                + r#"{"type":"table","content":[{"type":"table:row","marker":"tr","content":[{"type":"table:cell","marker":"th1","align":"start","content":["h"]}]}]}"#
        );
    }

    #[test]
    fn a_sidebar_lifts_its_cat_to_category() {
        assert_eq!(
            content("\\esb \\cat People\\cat*\n\\p who\n\\esbe"),
            r#"{"type":"sidebar","marker":"esb","category":"People","content":[{"type":"para","marker":"p","content":["who"]}]}"#
        );
        assert_eq!(
            content("\\esb \\p who\n\\esbe"),
            r#"{"type":"sidebar","marker":"esb","content":[{"type":"para","marker":"p","content":["who"]}]}"#
        );
    }

    #[test]
    fn a_figure_renames_src_to_file() {
        assert_eq!(
            content("\\ip before \\fig caption|src=\"a.png\" size=\"col\"\\fig* after"),
            r#"{"type":"para","marker":"ip","content":["before ",{"type":"figure","marker":"fig","file":"a.png","size":"col","content":["caption"]}," after"]}"#
        );
    }

    #[test]
    fn ref_is_its_own_type_with_loc_as_the_default_attribute() {
        assert_eq!(
            content("\\p see \\ref Mark 1:4|MRK 1:4\\ref* now"),
            r#"{"type":"para","marker":"p","content":["see ",{"type":"ref","loc":"MRK 1:4","content":["Mark 1:4"]}," now"]}"#
        );
    }

    #[test]
    fn usfm_is_dropped_and_its_version_never_echoed() {
        assert_eq!(
            content("\\usfm 3.1\n\\p x"),
            r#"{"type":"para","marker":"p","content":["x"]}"#
        );
    }

    #[test]
    fn an_unknown_marker_takes_the_para_shape() {
        assert_eq!(
            content("\\s5\n\\p x"),
            r#"{"type":"para","marker":"s5","content":[]},{"type":"para","marker":"p","content":["x"]}"#
        );
        // …unless its own `\*` spells it a milestone (`\zms\*`).
        assert_eq!(
            content("\\p a\\zms\\* b"),
            r#"{"type":"para","marker":"p","content":["a"]},{"type":"ms","marker":"zms"}," b""#
        );
    }

    #[test]
    fn an_optbreak_is_its_own_element() {
        assert_eq!(
            content("\\p a // b"),
            r#"{"type":"para","marker":"p","content":["a ",{"type":"optbreak"}," b"]}"#
        );
    }

    #[test]
    fn attributes_splat_and_the_later_definition_wins() {
        // Bare default value, named pairs, an `x-` passthrough, and a
        // duplicate whose SECOND definition is the one that survives.
        assert_eq!(
            content("\\p \\w In|in\\w*"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"w","lemma":"in","content":["In"]}]}"#
        );
        assert_eq!(
            content("\\p \\w x|lemma=\"a\" x-s=\"1\" lemma=\"b\"\\w*"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"w","lemma":"b","x-s":"1","content":["x"]}]}"#
        );
    }

    #[test]
    fn whitespace_is_canonicalized_the_way_the_fixtures_are() {
        // A newline inside a paragraph is ONE space; the delimiter after
        // `\v 1` never was content; a run of spaces collapses; `~` is the
        // non-breaking space it names.
        assert_eq!(
            content("\\p a\nb\n\\p c"),
            r#"{"type":"para","marker":"p","content":["a b"]},{"type":"para","marker":"p","content":["c"]}"#
        );
        assert_eq!(
            content("\\p \\v 1 verse one\n\\v 2 verse two"),
            r#"{"type":"para","marker":"p","content":[{"type":"verse","marker":"v","number":"1"},"verse one ",{"type":"verse","marker":"v","number":"2"},"verse two"]}"#
        );
        assert_eq!(
            content("\\p a  \t b"),
            r#"{"type":"para","marker":"p","content":["a b"]}"#
        );
        assert_eq!(
            content("\\p a~b"),
            "{\"type\":\"para\",\"marker\":\"p\",\"content\":[\"a\u{a0}b\"]}"
        );
        // A whitespace-only run is not content at all (no fixture in
        // testData holds a whitespace-only string).
        assert_eq!(
            content("\\p \\add a\\add* \\add b\\add*"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"add","content":["a"]},{"type":"char","marker":"add","content":["b"]}]}"#
        );
        // Whitespace in front of a BLOCK seam is layout, not text; in front
        // of a character marker or a closer it is text.
        assert_eq!(
            content("\\p x \\p y"),
            r#"{"type":"para","marker":"p","content":["x"]},{"type":"para","marker":"p","content":["y"]}"#
        );
        assert_eq!(
            content("\\p \\k ostrich \\k*bird"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"k","content":["ostrich "]},"bird"]}"#
        );
    }

    #[test]
    fn a_closed_span_nests_in_the_open_note_text_and_it_resumes() {
        // F3: `\xt*` makes the `\xt` an INLINE SPAN inside `\ft`, and the text
        // after it RESUMES `\ft` — the CST reads all three as note peers, this
        // is the projection re-parenting them.
        assert_eq!(
            content("\\p \\f + \\ft alpha \\xt ref\\xt* beta\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":["alpha ",{"type":"char","marker":"xt","content":["ref"]}," beta"]}]}]}"#
        );
        // Any other closed char is a span too — nothing about `\xt` is special.
        assert_eq!(
            content("\\p \\f + \\ft a \\add b\\add* c\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":["a ",{"type":"char","marker":"add","content":["b"]}," c"]}]}]}"#
        );
    }

    #[test]
    fn an_unclosed_note_marker_stays_a_peer() {
        // specExamples/cross-ref, which must NOT regress: no `\xt*`, so `\xt`
        // ends `\xo` the way it always did.
        assert_eq!(
            content("\\p \\x - \\xo 1.1 \\xt Ps 135\\x*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"x","caller":"-","content":[{"type":"char","marker":"xo","content":["1.1 "]},{"type":"char","marker":"xt","content":["Ps 135"]}]}]}"#
        );
        // And a CLOSED marker that is one of this note family's OWN peers is
        // still a peer: `\xop*` does not nest it in `\xo`.
        assert_eq!(
            content("\\p \\x - \\xo 1.1 \\xop L\\xop* \\xt Ps 135\\x*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"x","caller":"-","content":[{"type":"char","marker":"xo","content":["1.1 "]},{"type":"char","marker":"xop","content":["L"]},{"type":"char","marker":"xt","content":["Ps 135"]}]}]}"#
        );
        // An element that supplied its OWN closer adopts nothing: the note text
        // after `\ft*` belongs to the NOTE.
        assert_eq!(
            content("\\p \\f + \\ft x\\ft* more\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":["x"]}," more"]}]}"#
        );
    }

    #[test]
    fn a_closed_fv_nests_in_fqa_and_the_next_peer_still_seals_it() {
        // specExamples/footnote's shape: `\fv` is not one of the footnote's
        // peers, so it grafts; the UNCLOSED `\ft` after it seals `\fqa`.
        assert_eq!(
            content("\\p \\f + \\fqa a \\fv 38\\fv*\\ft b\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"fqa","content":["a ",{"type":"char","marker":"fv","content":["38"]}]},{"type":"char","marker":"ft","content":["b"]}]}]}"#
        );
    }

    #[test]
    fn a_graft_at_the_very_end_of_the_note_seals_with_the_note() {
        // biblica/CategoriesOnNotes: `\xt*` then only the note's own closer —
        // the graft is the LAST thing in `\ft`, and the space in front of `\f*`
        // is a whitespace-only run, so it is not content (rule 4).
        assert_eq!(
            content("\\p \\f + \\ft \\xt ref\\xt* \\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":[{"type":"char","marker":"xt","content":["ref"]}]}]}]}"#
        );
    }

    #[test]
    fn the_writer_escapes_what_json_requires_and_nothing_else() {
        // A quote, a backslash and a control byte in TEXT; the non-ASCII
        // bytes pass through as UTF-8, which is what the fixtures hold.
        let source = "\\p say \"hi\" \u{1}\u{7} — καί";
        assert_eq!(
            content(source),
            "{\"type\":\"para\",\"marker\":\"p\",\"content\":[\"say \\\"hi\\\" \\u0001\\u0007 — καί\"]}"
        );
    }
}
