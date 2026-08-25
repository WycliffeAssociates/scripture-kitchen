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
//! This is THE LOSSY VIEW. The token stream stays byte-identical; everything
//! below — attribute key order, quote style, content lifted to attributes,
//! whitespace canonicalization — is thrown away HERE and nowhere upstream. The
//! fold never feeds back into the core and never repairs: damage is lint's
//! answer, so no `unmatched` element is ever emitted.
//!
//! # What the projection changes
//!
//! - **Content → attribute lifts**, the one table that cannot be a marker-row
//!   column: `\ca`/`\va` → `altnumber`, `\cp`/`\vp` → `pubnumber`, `\cat` →
//!   `category` on the enclosing note/sidebar, a note's caller token →
//!   `caller`, and `\periph My Title|id="x"`'s title → `alt`.
//! - **One attribute RENAME**: `\fig`'s `src` is USJ's `file`.
//! - **`\usfm` is dropped** — the envelope's `version` is USJ's own.
//! - **Milestones keep the author's spelling** (`qt-s`, `zaln-e`), and the
//!   U25003 `\list-s`/`\table-s` CONTAINER is not an element at all: its
//!   points emit inline where they sit and its children land beside them.
//! - **A `table` wrapper is SYNTHESIZED** around each run of consecutive
//!   rows — the only element in the output with no marker behind it.
//! - **Inside a note, a CLOSED span is re-parented into the open note text**
//!   ([`Export::grafts`]; the CST keeps the flat peer reading):
//!
//! ```text
//! \ft alpha \xt ref\xt* beta   note{ ft["alpha ", xt["ref"], " beta"] }  grafted
//! \xo 1.1 \xt Ps 135\x*        note{ xo["1.1 "], xt["Ps 135"] }          no closer, peers
//! ```
//!
//! # Whitespace — testData's canonicalization, so the oracle is exact
//!
//! ```text
//! \p a\nb\n\p c         ["a b"]      every run collapses to ONE space
//! \v 1␠verse            ["verse"]    a marker/designator/code/caller space DELIMITS
//! \p x␠\p y             ["x"]        dropped at a BLOCK seam: para, chapter,
//!                                    table row, sidebar, periph, unknown, EOF
//! \xo 1.1:␠\ft note     ["1.1: "]    kept in front of anything INLINE, closers too
//! \add a\add*␠\add b    no "␠"       a whitespace-only run is never content
//! ```
//!
//! `~` becomes the non-breaking space it names, and `//` its own `optbreak`.

use crate::attributes::{AttrEvent, AttrResolution, attrs, resolve};
use crate::cst::{CloseReason, Cst, NODE_ID_BIT, Node, ROOT_TOKEN, container_kind};
use crate::export::{
    canonical, is_ws, marker_name, note_peers, text_at, trim, trim_end, trim_start,
};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind};

/// USJ's own envelope version: "3.1" per testData and scripture-editors'
/// `USJ_VERSION`, not usfmtc's "3.0".
const VERSION: &str = "3.1";

/// The `content` key, named because the `\b` rewind measures it.
const CONTENT: &str = ",\"content\":[";

/// Folds a lexed + built document into USJ JSON.
///
/// `tokens` must be `lex(source)`'s output and `cst` [`crate::cst::build`]'s
/// over those tokens: the fold reads token spans out of `source` and trusts
/// the CST's shape.
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
/// generality nothing here needs, and the library stays dependency-free.
struct Json {
    out: String,
}

impl Json {
    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// One JSON string, quotes included. Escapes exactly what RFC 8259
    /// requires — `"`, `\`, and the C0 controls (as `\u00XX` bar the five with
    /// short forms) — and passes every non-ASCII byte through as UTF-8.
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
    run: String,
    /// Whitespace read but not yet committed to `run`, held VERBATIM until
    /// what follows it is known. Holding it is the only way to tell
    /// `"1.1: "` (kept, `\ft` follows) from `"something"` (dropped, `\p`
    /// follows) — the bytes are identical.
    ws: String,
    /// Leading whitespace here DELIMITS a payload and is not content: set at
    /// an element's content start, after a chapter/verse element, and after a
    /// book code, note caller or designator.
    at_boundary: bool,
    /// A synthesized `table` wrapper is open, and items go inside it.
    table_open: bool,
    table_sep: bool,
}

// ---------------------------------------------------------------------------
// The fold
// ---------------------------------------------------------------------------
//
// A CONCRETE walker, not a `Visit` trait: the driver is twelve lines, so a
// trait is worth extracting only once the SHARED half is visible in two
// bodies at once (USX, HTML).

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
    /// This frame pushed the list it points at, so it pops it at close.
    owns_list: bool,
    /// What this frame owes the writer when it ends: `"]}"` for an ordinary
    /// element and `""` for a TRANSPARENT node that opened no object at all.
    close: &'static str,
    /// Where `,"content":[` began, for the one element whose EMPTY form has no
    /// `content` key at all (`\b`): with nothing in the array the writer
    /// rewinds here and closes the object. `usize::MAX` disables the rewind.
    rewind: usize,
    /// The one Text token this element LIFTED into a key instead of content —
    /// `\periph My Title|id="x"`'s title becomes `alt`. `u32::MAX` on every
    /// element but `periph`.
    lifted_text: u32,
    /// A NOTE element's own family PEER markers, so `grafts` can tell a peer
    /// from an inline span. `None` on every non-note frame.
    peers: Option<&'static [&'static str]>,
    /// An UNCLOSED note-text element (`\ft`, `\fqa`, `\xo`, …), so an
    /// explicitly-closed sibling GRAFTS into it instead of sealing it.
    adopts: bool,
    /// It has already grafted one span, so the direct note content FOLLOWING
    /// that span resumes this element rather than starting a new one.
    grafted: bool,
}

/// A chapter or verse element held back until its `\ca`/`\cp`/`\va`/`\vp`
/// annotations are known: they FOLLOW the marker they decorate.
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
                // The graft: an unclosed note-text element does NOT seal just
                // because its child list ran out — it STEALS the note's next
                // child, one at a time, while `grafts` says so. Re-pointing
                // this frame's cursor into the PARENT's range is the whole
                // mechanism; child lists are separate regions of `child_ids`,
                // so there is nothing to extend.
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
            // title → `alt`), so it only re-arms the delimiter rule.
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
            // Both were lifted at their node's open (`code` and `caller`
            // precede `content`), so here they only re-arm the delimiter rule.
            TokenKind::BookCode => self.lists[list].at_boundary = true,
            TokenKind::NoteCaller => self.lists[list].at_boundary = true,
            TokenKind::Designator => match self.absorb.take() {
                Some(slot) => {
                    self.lift(slot, trim(self.span(token)).to_string(), list);
                    self.lists[list].at_boundary = true;
                }
                None => {
                    let number = trim(self.span(token)).to_string();
                    // A designator with no chapter/verse in front of it is
                    // dropped rather than given a guessed owner; lint speaks.
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
            // does not know. Otherwise an unknown marker takes the para shape.
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
                // Any other markerless leaf is not an element (`\usfm`'s
                // payload never gets here, a `\pb` opens its own node).
                _ => {}
            },
        }
    }

    /// Puts a lifted value on the pending chapter/verse. With no pending owner
    /// the value becomes an ordinary `char` element rather than a guess; lint
    /// carries the placement complaint.
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

    // -- the note graft ----------------------------------------------------

    /// Does the note's next child GRAFT into `open`, the note-text element that
    /// just ran out of children?
    ///
    /// ```text
    /// \ft alpha \xt ref\xt* beta   ft["alpha ", xt["ref"], " beta"]   grafts
    /// \xo 1.1 \xt Ps 135\x*        xo["1.1 "], xt["Ps 135"]           no closer
    /// \xo 1.1 \xop L\xop* \xt …    xo["1.1 "], xop["L"], xt[…]        own peer
    /// ```
    ///
    /// The closer is NECESSARY BUT NOT SUFFICIENT — a closed marker that is one
    /// of THIS note family's peers is still a peer (five fixtures regress on the
    /// closer alone). The direct note content after a graft RESUMES the element
    /// it grafted into; everything else seals.
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
        // Text after a grafted span RESUMES the open element. With nothing
        // grafted yet there is nothing to resume, so `\ft x\ft* more` must not
        // pull the note's own text back inside `\ft`.
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
        // an element. Its first child is the `-s` POINT node — that is how it
        // is told apart from the point itself, whose first child is a token.
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

        // The lifted markers emit NOTHING: their content becomes an attribute
        // elsewhere, and they must not break the text run around them either
        // (`\v 1 \va 3\va* text` is one delimiter).
        //
        // Unless the content is not PLAIN TEXT — an attribute value cannot
        // hold markup, so `\vp \+it \+wj 21\+wj*\+it* \vp*` stays an ordinary
        // char element with its nesting intact (both fixture formats of
        // biblica/PublishingVersesWithFormatting agree).
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
            // `\cat` was read by the enclosing note/sidebar's open; `\usfm` is
            // dropped outright.
            "cat" | "usfm" => return None,
            _ => {}
        }

        let kind = generated::kind(marker_idx);
        // A note-text element that supplied NO closer of its own is the one an
        // explicitly-closed sibling grafts into (see `grafts`).
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
            // only `\cat`, Unknown is row 0 (a leaf). Anything landing here is
            // a table bug, not document damage — take the para shape.
            _ => "para",
        };

        self.json.raw("{\"type\":");
        self.json.string(element);
        if element != "ref" && element != "periph" {
            self.json.raw(",\"marker\":");
            self.json.string(&marker);
        }

        // `\id`'s book code and a note's caller are LIFTED out of content and
        // their keys precede it, so they are read here, not patched in later.
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
        // (usx.rng's `PeripheralDivision`), and it is the node's FIRST Text
        // child — the rest is the division's paragraphs.
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
        // `\b` EMPTY is the one element the fixtures write with no `content`
        // key at all (`\p` empty keeps `content: []`), while `\b` non-empty
        // keeps it. So the array is written speculatively and REWOUND.
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
        // token in DOCUMENT order, where the cursor already sits.
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
            // The element just written is an item of its PARENT's list, so the
            // parent's boundary rule ends with it.
            if let Some(parent) = self.lists.last_mut() {
                parent.at_boundary = false;
            }
        }
    }

    // -- attributes --------------------------------------------------------

    /// Splats one node's OWN attribute lists into keys. Later definition wins;
    /// key order, `=` spacing and quote style are the lossy step.
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
                        // (`\fig |a.png`) names nothing; lint says so.
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

    /// A carved payload token's text (`\id`'s book code, a note's caller), read
    /// off DIRECT children so the key precedes `content`.
    fn payload_child(&self, node: &Node, kind: TokenKind) -> Option<String> {
        self.direct_children(node)
            .find(|child| self.tokens[*child as usize].kind() == kind)
            .map(|child| trim(self.span(&self.tokens[child as usize])).to_string())
    }

    /// A `\cat` child's content, lifted to `category` on this note/sidebar. A
    /// prescan, not a patch, because the key precedes `content`.
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
        // Two runs that meet across a lifted element collapse together.
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
        // Whitespace-only text is held, not emitted — it must not force a
        // pending chapter/verse out before its `\ca`/`\va` are read.
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
    /// layout, not content? Verses, characters, notes, figures and milestones
    /// are NOT: they sit inside a line, so the space in front of them is text.
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

    /// Writes the accumulated run as one JSON string. A whitespace-only run is
    /// DROPPED — no testData fixture holds a whitespace-only string.
    fn flush_run(&mut self, list: usize) {
        let run = core::mem::take(&mut self.lists[list].run);
        if run.is_empty() || run.bytes().all(is_ws) {
            return;
        }
        self.begin_item(list, false);
        self.json.string(&run);
        self.lists[list].at_boundary = false;
    }

    /// Comma bookkeeping, plus the SYNTHESIZED `table` wrapper: it opens at the
    /// first of a run of consecutive rows and closes at the first non-row.
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
        text_at(self.source, token.start..token.end())
    }

    fn marker_name(&self, token: &Token) -> String {
        marker_name(self.source, token)
    }

    /// One node's text content, canonicalized like an ordinary content run —
    /// what a lift reads.
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
// THE ZOO: one hand-written case per mapping row.
// ---------------------------------------------------------------------------
//
// These pin INTENT independently of testData: the corpus oracle
// (tests/usj_corpus.rs) compares as `serde_json::Value` and so forgives key
// order. The zoo compares the STRING, pinning the writer's fixed key order
// (type, marker, attrs, number/sid, content last) as well as the shape.
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
        // The row is shared (`q1` resolves to `q`, `mt2` to `mt`); the NUMBER
        // is the author's, off the token's bytes.
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
        // …and its NON-empty form keeps it: a `\v` whose context mask allows
        // Para lands inside the `\b` (specExamples/poetry).
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
        // No `\id`, no book code, so no sid — and none is invented.
        assert_eq!(
            content("\\c 1"),
            r#"{"type":"chapter","marker":"c","number":"1"}"#
        );
        // The designator gate: prose after `\v ` is Text, so the verse element
        // reports an EMPTY number and the words stay content. `number":"Then"`
        // is unwritable.
        assert_eq!(
            content("\\c 1\n\\p \\v Then He declared"),
            r#"{"type":"chapter","marker":"c","number":"1"},"#.to_owned()
                + r#"{"type":"para","marker":"p","content":[{"type":"verse","marker":"v","number":""},"Then He declared"]}"#
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
        // No chapter to own it, and the owner is never guessed; lint carries
        // the placement complaint.
        assert_eq!(
            content("\\p \\cp M"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"cp","content":["M"]}]}"#
        );
    }

    #[test]
    fn a_markup_bearing_lift_stays_an_ordinary_char_element() {
        // An attribute value cannot hold markup, so the lift does not happen
        // at all (biblica/PublishingVersesWithFormatting).
        assert_eq!(
            content("\\p \\vp \\+it 21\\+it*\\vp* text"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"vp","content":[{"type":"char","marker":"it","content":["21"]}]}," text"]}"#
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
        // gets its own. `align` is the `r` in the ROW name.
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
        // duplicate whose SECOND definition survives.
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
        // A newline inside a paragraph is ONE space; `\v 1`'s delimiter is not
        // content; a run of spaces collapses; `~` is the nbsp it names.
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
        // A whitespace-only run is not content at all.
        assert_eq!(
            content("\\p \\add a\\add* \\add b\\add*"),
            r#"{"type":"para","marker":"p","content":[{"type":"char","marker":"add","content":["a"]},{"type":"char","marker":"add","content":["b"]}]}"#
        );
        // Layout in front of a BLOCK seam, text in front of a char or closer.
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
        // `\xt*` makes the `\xt` an INLINE SPAN inside `\ft`, and the text
        // after it RESUMES `\ft`; the CST reads all three as peers.
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
        // specExamples/cross-ref: no `\xt*`, so `\xt` ends `\xo`.
        assert_eq!(
            content("\\p \\x - \\xo 1.1 \\xt Ps 135\\x*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"x","caller":"-","content":[{"type":"char","marker":"xo","content":["1.1 "]},{"type":"char","marker":"xt","content":["Ps 135"]}]}]}"#
        );
        // A CLOSED marker that is one of the family's OWN peers stays a peer:
        // `\xop*` does not nest it in `\xo`.
        assert_eq!(
            content("\\p \\x - \\xo 1.1 \\xop L\\xop* \\xt Ps 135\\x*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"x","caller":"-","content":[{"type":"char","marker":"xo","content":["1.1 "]},{"type":"char","marker":"xop","content":["L"]},{"type":"char","marker":"xt","content":["Ps 135"]}]}]}"#
        );
        // An element with its OWN closer adopts nothing: text after `\ft*`
        // belongs to the NOTE.
        assert_eq!(
            content("\\p \\f + \\ft x\\ft* more\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":["x"]}," more"]}]}"#
        );
    }

    #[test]
    fn a_closed_fv_nests_in_fqa_and_the_next_peer_seals_it() {
        // specExamples/footnote's shape: `\fv` is not one of the footnote's
        // peers, so it grafts; the UNCLOSED `\ft` after it seals `\fqa`.
        assert_eq!(
            content("\\p \\f + \\fqa a \\fv 38\\fv*\\ft b\\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"fqa","content":["a ",{"type":"char","marker":"fv","content":["38"]}]},{"type":"char","marker":"ft","content":["b"]}]}]}"#
        );
    }

    #[test]
    fn a_graft_at_the_very_end_of_the_note_seals_with_the_note() {
        // biblica/CategoriesOnNotes: the graft is the LAST thing in `\ft`, and
        // the space before `\f*` is a whitespace-only run, so not content.
        assert_eq!(
            content("\\p \\f + \\ft \\xt ref\\xt* \\f*"),
            r#"{"type":"para","marker":"p","content":[{"type":"note","marker":"f","caller":"+","content":[{"type":"char","marker":"ft","content":[{"type":"char","marker":"xt","content":["ref"]}]}]}]}"#
        );
    }

    #[test]
    fn the_writer_escapes_what_json_requires_and_nothing_else() {
        // A quote, a backslash and control bytes in TEXT; non-ASCII passes
        // through as UTF-8.
        let source = "\\p say \"hi\" \u{1}\u{7} — καί";
        assert_eq!(
            content(source),
            "{\"type\":\"para\",\"marker\":\"p\",\"content\":[\"say \\\"hi\\\" \\u0001\\u0007 — καί\"]}"
        );
    }
}
