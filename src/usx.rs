//! USX export: the CST folded into the USX XML shape.
//!
//! ```text
//! \id GEN                      <usx version="3.0">
//! \c 1                           <book code="GEN" style="id" />
//! \p                             <chapter number="1" style="c" sid="GEN 1" />
//! \v 1 verse one                 <para style="p"><verse number="1" style="v" sid="GEN 1:1"
//! \v 2 verse two                   />verse one <verse eid="GEN 1:1" /><verse number="2"
//!                                  style="v" sid="GEN 1:2" />verse two<verse eid="GEN 1:2"
//!                                  /></para>
//!                                <chapter eid="GEN 1" />
//!                              </usx>
//! ```
//!
//! ```text
//! \p \w gracious|lemma="grace"\w*   <char style="w" lemma="grace">gracious</char>
//! \c 1 \ca 2\ca* \cp M              <chapter … altnumber="2" pubnumber="M" />
//! \f + \ft note\f*                  <note caller="+" style="f">…</note>
//! \qt-s |who="Pilate"\*             <ms style="qt-s" who="Pilate" />
//! \tr \tc1 a\tcr2 b                 <table><row style="tr"><cell style="tc1" align="start">
//! ```
//!
//! Same fold, same lossy step and same laws as [`crate::usj`] — read that
//! module first; the lift table, the attribute splat, the synthesized `table`
//! wrapper, milestone spellings and the note graft are all true here
//! unchanged. What follows is only what USX adds or reads differently.
//!
//! # `style` is the marker, the type is the element
//!
//! USJ's `{"type":"para","marker":"p"}` is USX's `<para style="p">`, and the
//! transposition is otherwise 1:1. The two elements that carry no `style` are
//! the two that carry no marker in USJ either — `<ref loc>` and the synthesized
//! `<table>` — plus `<periph alt id>`.
//!
//! # The root version is the DOCUMENT's, not a constant
//!
//! ```text
//! (nothing)     <usx version="3.0">
//! \usfm 3.1     <usx version="3.1">
//! ```
//!
//! `\usfm` is not simply dropped the way USJ drops it: its payload IS the root
//! version, and "3.0" is only the default. (USJ's envelope version is USJ's
//! own — a different fact that happens to look similar.)
//!
//! # sid, eid, and vid — the decoration USJ has no room for
//!
//! A USJ chapter/verse is one element with a `sid`. A USX one is a PAIR: the
//! `sid` element where the marker sits and a separate `<verse eid="GEN 1:1" />`
//! at the point the verse ends. Where that point is, is the only genuinely new
//! mechanism in this module:
//!
//! ```text
//! \p                    <para style="p"><verse … sid="JHN 1:2" />It began …</para>
//! \v 2 It began …       <para style="q1" vid="JHN 1:2">“God said, …</para>
//! \q1 “God said, …      <para style="q2" vid="JHN 1:2">to open …<verse eid="JHN 1:2" /></para>
//! \q2 to open …         <para style="q1"><verse … sid="JHN 1:3" />Someone …</para>
//! \q1
//! \v 3 Someone …
//! ```
//!
//! A verse does NOT end at its own paragraph's close. It runs to the next `\v`,
//! the next `\c`, or the end of the document; every BLOCK it crosses on the way
//! carries `vid="<its sid>"`; and its `eid` lands at the end of the LAST block
//! it reaches. Two qualifications on that last clause:
//!
//! 1. **Trailing headings and empty blocks are TRIMMED.** `\v 2 …\n\s1 A
//!    heading\n\p \v 3 …` closes verse 2 at its own paragraph, and the `\s1`
//!    carries no `vid` (basic/section). A heading in the MIDDLE of a verse
//!    still carries one, so this is a backwards trim from the boundary, not a
//!    forward stop. An empty block (`\b`, `\s5`) trims the same way.
//! 2. **A CELL is a block and a `<table>` hoists the `vid`.** `<table vid="…">`
//!    with the `eid` inside the last `<cell>` (specExamples/table).
//!
//! Because the close point is only knowable once the NEXT verse is in sight,
//! this is a two-pass export: [`decorate`] scans the token stream and records
//! where every `eid` goes and which block nodes carry a `vid`; the fold then
//! writes. A projection artifact exactly like the synthesized `<table>` — the
//! never-synthesize law governs TOKENS, and no token is invented.
//!
//! # Whitespace
//!
//! Unlike usj, a whitespace-only run IS content here:
//!
//! ```text
//! \f …\f*\n\v 4     </note> <verse eid="MAT 1:3" />
//! ```
//!
//! # XML 1.0 cannot spell a C0 control
//!
//! The escaper handles `& < > "` and emits the C0 controls as `&#xNN;`. Those
//! references are not legal XML 1.0 — but neither is the raw byte, and the
//! alternative is silently deleting document content. Recorded, not repaired.

use std::collections::HashMap;

use crate::attributes::{AttrEvent, AttrResolution, attrs, resolve};
use crate::cst::{CloseReason, Cst, NODE_ID_BIT, Node, ROOT_TOKEN, container_kind};
use crate::export::{
    canonical, is_ws, marker_name, note_peers, span, text_at, trim, trim_end, trim_start,
};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::{Category, MarkerKind};
use crate::{Token, TokenKind};

/// The USX schema version a document that declares no `\usfm` is written as.
const DEFAULT_VERSION: &str = "3.0";

/// Folds a lexed + built document into USX XML.
///
/// `tokens` must be `lex(source)`'s output and `cst` must be
/// [`crate::cst::build`]'s over those tokens — the fold reads token spans out
/// of `source` and trusts the CST's shape.
pub fn usx(source: &[u8], tokens: &[Token], cst: &Cst) -> String {
    let decor = decorate(source, tokens, cst);
    let mut export = Export {
        source,
        tokens,
        cst,
        decor,
        xml: Xml {
            out: String::with_capacity(source.len() * 2),
        },
        lists: Vec::new(),
        book: None,
        chapter: None,
        open_chapter: None,
        pending: None,
        absorb: None,
        cursor: 0,
    };
    export.run();
    export.xml.out
}

// ---------------------------------------------------------------------------
// The writer
// ---------------------------------------------------------------------------

/// The hand-rolled XML writer: a `String` and one escaper per context. USX is a
/// small CLOSED shape, so the library stays dependency-free — and it only ever
/// writes XML, never reads it.
struct Xml {
    out: String,
}

impl Xml {
    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Character data. `&` and `<` must be escaped; `>` is escaped too because
    /// `]]>` would otherwise be ambiguous and a uniform rule is cheaper to
    /// trust than a contextual one.
    fn text(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '&' => self.raw("&amp;"),
                '<' => self.raw("&lt;"),
                '>' => self.raw("&gt;"),
                ch => self.control_or(ch),
            }
        }
    }

    /// ` name="value"` — one attribute, quote always double, so `"` escapes and
    /// `'` does not.
    fn attr(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.raw(name);
        self.raw("=\"");
        for ch in value.chars() {
            match ch {
                '&' => self.raw("&amp;"),
                '<' => self.raw("&lt;"),
                '>' => self.raw("&gt;"),
                '"' => self.raw("&quot;"),
                ch => self.control_or(ch),
            }
        }
        self.out.push('"');
    }

    /// A C0 control as a numeric character reference, anything else verbatim.
    /// The reference is not legal XML 1.0, but dropping content is worse.
    fn control_or(&mut self, ch: char) {
        if (ch as u32) < 0x20 && !matches!(ch, '\t' | '\n' | '\r') {
            self.raw("&#x");
            const HEX: &[u8; 16] = b"0123456789abcdef";
            self.out.push(HEX[((ch as u32) >> 4) as usize] as char);
            self.out.push(HEX[((ch as u32) & 0xf) as usize] as char);
            self.out.push(';');
        } else {
            self.out.push(ch);
        }
    }
}

// ---------------------------------------------------------------------------
// PASS ONE: where the eids go and which blocks carry a vid
// ---------------------------------------------------------------------------

/// Everything the fold cannot know when it gets there, because it depends on
/// what comes NEXT.
#[derive(Default)]
struct Decor {
    /// Block node id → the `vid` attribute it carries.
    vid: HashMap<u32, String>,
    /// Block node id → the verse `eid` to write just before its closing tag.
    eid_at_close: HashMap<u32, String>,
    /// Token index (always a `\v` or `\c`) → the verse `eid` to write just
    /// before that marker's own element.
    eid_before: HashMap<u32, String>,
    /// A verse still open at the end of a document that has no block to close
    /// it in — the `eid` goes at the root, last.
    root_eid: Option<String>,
}

/// One BLOCK in document order — the unit `vid` attaches to and `eid`s land at
/// the end of. Paragraphs, headers, table cells, and an unknown marker in
/// paragraph position (which has no node, hence `node == u32::MAX`).
struct Blk {
    node: u32,
    /// A title/section/introduction paragraph: eligible to carry a `vid` in the
    /// middle of a verse, never to be the block a verse ENDS in.
    heading: bool,
    /// Something a reader would see landed in it.
    content: bool,
}

/// Which paragraph families a verse never ENDS in. The spec's own categories are
/// already exactly the set the fixtures trim, so no hand-written list is needed.
fn is_heading(marker_idx: MarkerIdx) -> bool {
    matches!(
        generated::category(marker_idx),
        Category::ParaTitlesSections | Category::ParaIdentification | Category::ParaIntroductions
    )
}

/// Scans the token stream once and answers the questions the fold will ask out
/// of order.
fn decorate(source: &[u8], tokens: &[Token], cst: &Cst) -> Decor {
    let mut node_of_token = vec![u32::MAX; tokens.len()];
    for (id, node) in cst.nodes.iter().enumerate() {
        if node.token != ROOT_TOKEN && (node.token as usize) < tokens.len() {
            node_of_token[node.token as usize] = id as u32;
        }
    }

    let mut decor = Decor::default();
    let mut blocks: Vec<Blk> = Vec::new();
    /// The verse currently open: its sid and the block it started in.
    struct Open {
        sid: String,
        block: usize,
    }
    let mut open: Option<Open> = None;
    let mut book: Option<String> = None;
    let mut chapter: Option<String> = None;
    // `Some(true)` = a `\v` is waiting for its designator, `Some(false)` a `\c`.
    let mut awaiting: Option<bool> = None;
    // A SIDEBAR is its own scope: an open verse survives it and resumes in the
    // `\p` after `\esbe`, but the sidebar's own paragraphs carry NO vid
    // (usfmjsTests/esb). So the scan looks away while one is open.
    let mut in_sidebar = false;

    /// Closes the open verse: picks the block its `eid` belongs at the end of
    /// (the last non-heading block with content, never before its own), files a
    /// `vid` on every block it crossed, and records where the `eid` goes.
    fn close(
        decor: &mut Decor,
        blocks: &[Blk],
        open: &mut Option<Open>,
        at: Option<u32>,
        tokens: &[Token],
    ) {
        let Some(verse) = open.take() else { return };
        if verse.block == usize::MAX || blocks.is_empty() {
            match at {
                Some(token) => {
                    decor.eid_before.insert(token, verse.sid);
                }
                None => decor.root_eid = Some(verse.sid),
            }
            return;
        }
        let cur = blocks.len() - 1;
        let mut last = verse.block;
        for at in (verse.block..=cur).rev() {
            if blocks[at].content && !blocks[at].heading {
                last = at;
                break;
            }
        }
        for crossed in &blocks[verse.block + 1..=last] {
            if crossed.node != u32::MAX {
                decor.vid.insert(crossed.node, verse.sid.clone());
            }
        }
        // A `\v` sits INSIDE its block, so when the verse ends at the very
        // paragraph the next one starts in, the eid is written inline right
        // before it. A `\c` sits between blocks, so it never can be.
        let inline = at.is_some_and(|token| {
            last == cur && generated::kind(tokens[token as usize].marker_idx) == MarkerKind::Verse
        });
        if inline {
            decor
                .eid_before
                .insert(at.expect("inline implies a token"), verse.sid);
        } else if blocks[last].node != u32::MAX {
            decor.eid_at_close.insert(blocks[last].node, verse.sid);
        } else if let Some(token) = at {
            decor.eid_before.insert(token, verse.sid);
        } else {
            decor.root_eid = Some(verse.sid);
        }
    }

    for idx in 0..tokens.len() {
        let token = &tokens[idx];
        let mut content = false;
        if let TokenKind::Marker { .. } = token.kind()
            && generated::kind(token.marker_idx) == MarkerKind::Sidebar
        {
            // `\esb` opens, `\esbe` closes — one row, told apart by the
            // occurrence's own spelling.
            in_sidebar = marker_name(source, token) == "esb";
        }
        if in_sidebar {
            continue;
        }
        match token.kind() {
            TokenKind::Marker { .. } => {
                if token.marker_idx == generated::UNRESOLVED {
                    let milestone = matches!(
                        tokens.get(idx + 1).map(Token::kind),
                        Some(TokenKind::MilestoneTerminator)
                    );
                    if milestone {
                        content = true;
                    } else {
                        blocks.push(Blk {
                            node: u32::MAX,
                            heading: false,
                            content: false,
                        });
                    }
                } else if generated::category(token.marker_idx) == Category::ChapterVerse {
                    // `\c`/`\v` are boundaries; `\ca`/`\cp`/`\va`/`\vp` are
                    // lifted decoration and neither block nor content.
                    match generated::kind(token.marker_idx) {
                        MarkerKind::Chapter => {
                            close(&mut decor, &blocks, &mut open, Some(idx as u32), tokens);
                            awaiting = Some(false);
                        }
                        MarkerKind::Verse => {
                            close(&mut decor, &blocks, &mut open, Some(idx as u32), tokens);
                            awaiting = Some(true);
                        }
                        _ => {}
                    }
                } else {
                    match generated::kind(token.marker_idx) {
                        MarkerKind::Paragraph | MarkerKind::Header | MarkerKind::TableCell
                            // `\usfm` becomes the root's version attribute, so
                            // it is not a block even though its row is a header.
                            if generated::name(token.marker_idx) != "usfm" =>
                        {
                            blocks.push(Blk {
                                node: node_of_token[idx],
                                heading: is_heading(token.marker_idx),
                                content: false,
                            });
                        }
                        MarkerKind::Character
                        | MarkerKind::Note
                        | MarkerKind::Figure
                        | MarkerKind::Milestone => content = true,
                        _ => {}
                    }
                }
            }
            TokenKind::BookCode => book = Some(trim(span(source, token)).to_string()),
            TokenKind::Designator => match awaiting.take() {
                Some(false) => chapter = Some(trim(span(source, token)).to_string()),
                Some(true) => {
                    let number = trim(span(source, token));
                    if let (Some(book), Some(chapter)) = (book.as_deref(), chapter.as_deref()) {
                        open = Some(Open {
                            sid: format!("{book} {chapter}:{number}"),
                            block: blocks.len().checked_sub(1).unwrap_or(usize::MAX),
                        });
                    }
                }
                None => {}
            },
            TokenKind::Text => content = !span(source, token).bytes().all(is_ws),
            TokenKind::OptBreak => content = true,
            _ => {}
        }
        if content && let Some(block) = blocks.last_mut() {
            block.content = true;
        }
    }
    close(&mut decor, &blocks, &mut open, None, tokens);
    decor
}

/// The version `\usfm` declared, or [`DEFAULT_VERSION`].
fn declared_version(source: &[u8], tokens: &[Token]) -> String {
    let mut seen = false;
    for token in tokens {
        match token.kind() {
            TokenKind::Marker { .. } => {
                seen = token.marker_idx != generated::UNRESOLVED
                    && generated::name(token.marker_idx) == "usfm";
            }
            TokenKind::Text if seen => {
                let text = trim(span(source, token));
                if !text.is_empty() {
                    return text.to_string();
                }
            }
            _ => {}
        }
    }
    DEFAULT_VERSION.to_string()
}

// ---------------------------------------------------------------------------
// One content list
// ---------------------------------------------------------------------------

/// The state of ONE element's children while they are being written. Its own
/// stack rather than a frame field, because a TRANSPARENT node (a U25003
/// container) writes into its PARENT's: the frame stack and the list stack have
/// different depths on purpose.
#[derive(Default)]
struct ListState {
    /// The text run being accumulated between two elements.
    run: String,
    /// Whitespace read but not yet committed to `run`, held VERBATIM until what
    /// follows it is known: at a block seam the whole of it is dropped, and
    /// anywhere else it is one space. Holding it is the only way to tell
    /// `"1.1: "` (kept, `\ft` follows) from `"something"` (dropped, `\p`
    /// follows) — the bytes are identical.
    ws: String,
    /// Leading whitespace here DELIMITS a payload and is not content: set at an
    /// element's content start, after a chapter/verse element, and after a book
    /// code, note caller or designator.
    at_boundary: bool,
    /// A synthesized `<table>` is open, and items go inside it.
    table_open: bool,
}

// ---------------------------------------------------------------------------
// PASS TWO: the fold
// ---------------------------------------------------------------------------

/// One frame of the driver's own stack — the same ChildCursor shape lint's walk
/// uses (no recursion), plus which element's children this node writes into.
struct Frame {
    next: u32,
    end: u32,
    /// The node's own opening-marker token id, so the leaf loop can skip it.
    token: u32,
    /// The node's CST id, which is how [`Decor`] is keyed. `u32::MAX` for the
    /// root and for a transparent container.
    node: u32,
    /// Index into [`Export::lists`]. A transparent node shares its parent's.
    list: usize,
    /// This frame pushed the list it points at, and pops it at close.
    owns_list: bool,
    /// The element name to close, or `""` for a TRANSPARENT node that opened no
    /// element at all.
    tag: &'static str,
    /// Byte offset just past the open tag's `>`. If nothing has been written by
    /// the time the frame closes, the writer rewinds one byte and emits ` />` —
    /// which is how every empty element in the fixtures is spelled.
    open_end: usize,
    /// The one Text token this element LIFTED into an attribute instead of
    /// content — `\periph My Title|id="x"`, whose title becomes `alt`.
    lifted_text: u32,
    /// This frame is a NOTE element: its own family's PEER markers, so the graft
    /// can tell a peer from an inline span. `None` for anything else.
    peers: Option<&'static [&'static str]>,
    /// This frame is an UNCLOSED note-text element (`\ft`, `\fqa`, `\xo`, …), so
    /// an explicitly-closed sibling GRAFTS into it instead of sealing it.
    adopts: bool,
    /// It has already grafted one span, so the direct note content that FOLLOWS
    /// that span resumes this element rather than starting a new one.
    grafted: bool,
}

/// A chapter or verse element held back until its `\ca`/`\cp`/`\va`/`\vp`
/// annotations are known — they FOLLOW the marker they decorate.
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
    decor: Decor,
    xml: Xml,
    lists: Vec<ListState>,
    book: Option<String>,
    chapter: Option<String>,
    /// The sid of the chapter awaiting its `<chapter eid />`, which is written
    /// after the chapter's last block — at the next `\c` or at the document end.
    open_chapter: Option<String>,
    pending: Option<Pending>,
    absorb: Option<Absorb>,
    /// The next token the walk will deliver — how a node close asks what comes
    /// AFTER it without looking at the stack.
    cursor: u32,
}

impl<'a> Export<'a> {
    fn run(&mut self) {
        let version = declared_version(self.source, self.tokens);
        self.xml.raw("<usx");
        self.xml.attr("version", &version);
        self.xml.raw(">");

        let root = &self.cst.nodes[0];
        self.lists.push(ListState {
            at_boundary: true,
            ..ListState::default()
        });
        let mut cur = Frame {
            next: root.children.start,
            end: root.children.end,
            token: ROOT_TOKEN,
            node: u32::MAX,
            list: 0,
            owns_list: true,
            tag: "",
            open_end: usize::MAX,
            lifted_text: u32::MAX,
            peers: None,
            adopts: false,
            grafted: false,
        };
        let mut stack: Vec<Frame> = Vec::new();

        loop {
            if cur.next == cur.end {
                // An unclosed note-text element does NOT seal just because its
                // child list ran out: it STEALS the note's next child when that
                // child is an explicitly-closed span (or the direct note content
                // following one), one at a time.
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

        if let Some(sid) = self.decor.root_eid.take() {
            self.verse_eid(&sid);
        }
        self.close_chapter();
        self.xml.raw("</usx>");
    }

    // -- chapter / verse decoration ----------------------------------------

    fn verse_eid(&mut self, sid: &str) {
        self.xml.raw("<verse");
        self.xml.attr("eid", sid);
        self.xml.raw(" />");
    }

    fn close_chapter(&mut self) {
        if let Some(sid) = self.open_chapter.take() {
            self.xml.raw("<chapter");
            self.xml.attr("eid", &sid);
            self.xml.raw(" />");
        }
    }

    // -- leaves ------------------------------------------------------------

    fn leaf(&mut self, idx: u32, frame: &Frame) {
        self.cursor = idx + 1;
        if idx == frame.token {
            return; // the node's own opening marker
        }
        if idx == frame.lifted_text {
            // Already written as an attribute at this element's open; like the
            // book code and the note caller it only re-arms the delimiter rule.
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
                self.begin_item(list, None);
                self.xml.raw("<optbreak />");
                self.lists[list].at_boundary = false;
            }
            // Both were already lifted at their node's open; here they only
            // re-arm the delimiter rule for the text that follows them.
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
            // Whitespace in FRONT of an explicit closer is CONTENT — `\k
            // ostrich \k*bird` keeps "ostrich " — so the closer commits it
            // rather than letting the element's close decide.
            TokenKind::ClosingMarker { .. } => self.commit_ws(list),
            TokenKind::MilestoneTerminator => self.lists[list].ws.clear(),
            TokenKind::AttrList => {}
            TokenKind::Milestone { .. } => {}
        }
    }

    /// A marker that opened no node: `\c`, `\v`, the lifted leaves, an unknown
    /// marker, and the bare-`\*` milestone spelling.
    fn marker_leaf(&mut self, idx: u32, token: &Token, list: usize) {
        let marker = self.marker_name(token);
        if token.marker_idx == generated::UNRESOLVED {
            // `\zms\*`: the `\*` SPELLING says milestone, whatever the row does
            // not know. Otherwise an unknown marker in paragraph position takes
            // usfm-grammar's para shape.
            let milestone = matches!(
                self.tokens.get(idx as usize + 1).map(Token::kind),
                Some(TokenKind::MilestoneTerminator)
            );
            self.flush_pending(list);
            self.seal_run(list, !milestone);
            self.begin_item(list, None);
            self.xml.raw(if milestone { "<ms" } else { "<para" });
            self.xml.attr("style", &marker);
            self.xml.raw(" />");
            self.lists[list].at_boundary = false;
            return;
        }

        match generated::kind(token.marker_idx) {
            MarkerKind::Chapter | MarkerKind::Verse => {
                let verse = generated::kind(token.marker_idx) == MarkerKind::Verse;
                self.flush_pending(list);
                // A chapter is a block seam; a verse is not.
                self.seal_run(list, !verse);
                if let Some(sid) = self.decor.eid_before.remove(&idx) {
                    self.verse_eid(&sid);
                }
                if !verse {
                    self.close_chapter();
                }
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
    /// the projection must not guess one, so the value becomes an ordinary
    /// `char` element and lint carries the placement complaint.
    fn lift(&mut self, slot: Absorb, value: String, list: usize) {
        match (&mut self.pending, slot) {
            (Some(pending), Absorb::AltNumber) => pending.altnumber = Some(value),
            (Some(pending), Absorb::PubNumber) => pending.pubnumber = Some(value),
            (None, _) => {
                self.seal_run(list, false);
                self.begin_item(list, None);
                let marker = match slot {
                    Absorb::AltNumber => "ca",
                    Absorb::PubNumber => "cp",
                };
                self.xml.raw("<char");
                self.xml.attr("style", marker);
                self.xml.raw(">");
                self.xml.text(&value);
                self.xml.raw("</char>");
                self.lists[list].at_boundary = false;
            }
        }
    }

    // -- the note graft ----------------------------------------------------

    /// Does the note's next child GRAFT into `open`, the note-text element that
    /// just ran out of children? The rule is shared by all three exports — see
    /// planning/quirks.md.
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
                node: u32::MAX,
                list,
                owns_list: false,
                tag: "",
                open_end: usize::MAX,
                lifted_text: u32::MAX,
                peers: None,
                adopts: false,
                grafted: false,
            });
        }

        // The lifted markers emit NOTHING: their content becomes an attribute
        // somewhere else, and they must not break the text run around them.
        // Unless that content is not PLAIN TEXT — an XML attribute cannot hold
        // markup, so `\vp \+it 21\+it*\vp*` stays an ordinary `<char style="vp">`
        // with its nesting intact (biblica/PublishingVersesWithFormatting).
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
            // `\cat` was already read by the enclosing note/sidebar's open, and
            // `\usfm` became the ROOT's version attribute.
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

        self.flush_pending(list);
        self.seal_run(list, self.seam_at(Some(node.token)));
        self.begin_item(list, (kind == MarkerKind::TableRow).then_some(node));

        // A milestone POINT carries attributes and no content at all.
        if is_milestone {
            self.xml.raw("<ms");
            self.xml.attr("style", &marker);
            self.write_attrs(node, marker_idx, &marker);
            self.xml.raw(" />");
            self.lists[list].at_boundary = false;
            return None;
        }

        let tag = match kind {
            MarkerKind::Header if marker == "id" => "book",
            MarkerKind::Character if marker == "ref" => "ref",
            MarkerKind::Paragraph | MarkerKind::Header => "para",
            MarkerKind::Character => "char",
            MarkerKind::Note => "note",
            MarkerKind::Figure => "figure",
            MarkerKind::Sidebar => "sidebar",
            MarkerKind::Periph => "periph",
            MarkerKind::TableRow => "row",
            MarkerKind::TableCell => "cell",
            // Chapter/Verse never open a scope, Milestone went above, Meta is
            // only `\cat`, Unknown is row 0 (handled as a leaf). A row that
            // opened a scope and lands here is a table bug, not damage in the
            // document — take the para shape and let lint speak.
            _ => "para",
        };

        self.xml.raw("<");
        self.xml.raw(tag);

        // `\id`'s book code and a note's caller are both LIFTED out of content;
        // the fixtures write them in front of `style`, so they are read here off
        // the node's direct children rather than patched in later.
        if tag == "book"
            && let Some(code) = self.payload_child(node, TokenKind::BookCode)
        {
            self.book = Some(code.clone());
            self.xml.attr("code", &code);
        }
        let caller = (kind == MarkerKind::Note)
            .then(|| self.payload_child(node, TokenKind::NoteCaller))
            .flatten();
        if let Some(caller) = caller {
            self.xml.attr("caller", &caller);
        }
        if tag != "ref" && tag != "periph" {
            self.xml.attr("style", &marker);
        }
        if let Some(category) = self.category(node) {
            self.xml.attr("category", &category);
        }
        // `\periph My Title|id="x"`: the TITLE TEXT is the `alt` attribute
        // (usx.rng's `PeripheralDivision` writes it as one), so it is lifted out
        // of content the way `\id`'s code and a note's caller are.
        let mut lifted_text = u32::MAX;
        if kind == MarkerKind::Periph {
            let title = self
                .direct_children(node)
                .find(|child| self.tokens[*child as usize].kind() == TokenKind::Text);
            if let Some(title) = title {
                let value = trim(self.span(&self.tokens[title as usize])).to_string();
                self.xml.attr("alt", &value);
                lifted_text = title;
            }
        }
        if kind == MarkerKind::TableCell {
            // `tcr`/`thr` vs `tc`/`th`: the `r` in the ROW NAME is the whole of
            // the alignment fact.
            let align = if generated::name(marker_idx).ends_with('r') {
                "end"
            } else {
                "start"
            };
            self.xml.attr("align", align);
        }
        self.write_attrs(node, marker_idx, &marker);
        // A CELL's vid is hoisted to the enclosing `<table>` (specExamples/table)
        // and never written here.
        if kind != MarkerKind::TableCell
            && let Some(vid) = self.decor.vid.get(&id).cloned()
        {
            self.xml.attr("vid", &vid);
        }
        self.xml.raw(">");

        self.lists.push(ListState {
            at_boundary: true,
            ..ListState::default()
        });
        Some(Frame {
            next: node.children.start,
            end: node.children.end,
            token: node.token,
            node: id,
            list: self.lists.len() - 1,
            owns_list: true,
            tag,
            open_end: self.xml.out.len(),
            lifted_text,
            peers,
            adopts,
            grafted: false,
        })
    }

    fn close_frame(&mut self, frame: &Frame) {
        let list = frame.list;
        self.flush_pending(list);
        // What ends this element decides its trailing whitespace: the next token
        // in DOCUMENT order, which the cursor is already sitting on.
        self.seal_run(list, self.seam_at(Some(self.cursor)));
        if self.lists[list].table_open {
            self.xml.raw("</table>");
            self.lists[list].table_open = false;
        }
        // The verse this block is the LAST one of ends here, inside it.
        if frame.node != u32::MAX
            && let Some(sid) = self.decor.eid_at_close.remove(&frame.node)
        {
            self.verse_eid(&sid);
        }
        if !frame.tag.is_empty() {
            if self.xml.out.len() == frame.open_end {
                // Nothing landed inside: rewind over the `>` and self-close,
                // which is how every empty element in the fixtures is spelled.
                self.xml.out.truncate(frame.open_end - 1);
                self.xml.raw(" />");
            } else {
                self.xml.raw("</");
                self.xml.raw(frame.tag);
                self.xml.raw(">");
            }
        }
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

    /// Splats one node's OWN attribute lists into XML attributes. Later
    /// definition wins (the interpreter's merge rule); key order, `=` spacing
    /// and quote style are the documented lossy step.
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
                // The one PER-FORMAT RENAME: `\fig`'s `src` is USX's `file`.
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
            self.xml.attr(name, value);
        }
    }

    /// A carved payload token's text (`\id`'s book code, a note's caller), read
    /// off the node's DIRECT children so the attribute can be written before the
    /// open tag closes.
    fn payload_child(&self, node: &Node, kind: TokenKind) -> Option<String> {
        self.direct_children(node)
            .find(|child| self.tokens[*child as usize].kind() == kind)
            .map(|child| trim(self.span(&self.tokens[child as usize])).to_string())
    }

    /// A `\cat` child's content, lifted to `category` on this note/sidebar —
    /// read BEFORE the open tag closes, which is why it is a prescan.
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

    /// Writes the accumulated run as character data. Unlike USJ's twin, a run
    /// that is whitespace ONLY is KEPT: `</note> <verse eid=…/>`.
    fn flush_run(&mut self, list: usize) {
        let run = core::mem::take(&mut self.lists[list].run);
        if run.is_empty() {
            return;
        }
        self.begin_item(list, None);
        self.xml.text(&run);
        self.lists[list].at_boundary = false;
    }

    /// The SYNTHESIZED `<table>` wrapper: it opens at the first of a run of
    /// consecutive rows and closes at the first child that is not one. `row` is
    /// the row node when the child about to be written is one.
    fn begin_item(&mut self, list: usize, row: Option<&Node>) {
        match row {
            Some(row) if !self.lists[list].table_open => {
                self.xml.raw("<table");
                // A verse open across the table decorates the WRAPPER, not the
                // cells — so the vid is read off the row's first cell.
                if let Some(vid) = self.first_cell_vid(row) {
                    self.xml.attr("vid", &vid);
                }
                self.xml.raw(">");
                self.lists[list].table_open = true;
            }
            None if self.lists[list].table_open => {
                self.xml.raw("</table>");
                self.lists[list].table_open = false;
            }
            _ => {}
        }
    }

    /// The `vid` [`decorate`] filed on this row's first cell, which the
    /// synthesized `<table>` wears instead.
    fn first_cell_vid(&self, row: &Node) -> Option<String> {
        // The row node's own children start with its `\tr` TOKEN; the cells are
        // the node children after it.
        let cell = self.cst.child_ids[row.children.start as usize..row.children.end as usize]
            .iter()
            .find(|child| *child & NODE_ID_BIT != 0)?;
        self.decor.vid.get(&(cell & !NODE_ID_BIT)).cloned()
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
        self.begin_item(list, None);
        self.xml
            .raw(if pending.verse { "<verse" } else { "<chapter" });
        self.xml.attr("number", &pending.number);
        self.xml
            .attr("style", if pending.verse { "v" } else { "c" });
        if let Some(sid) = &pending.sid {
            self.xml.attr("sid", sid);
            if !pending.verse {
                self.open_chapter = Some(sid.clone());
            }
        }
        if let Some(altnumber) = &pending.altnumber {
            self.xml.attr("altnumber", altnumber);
        }
        if let Some(pubnumber) = &pending.pubnumber {
            self.xml.attr("pubnumber", pubnumber);
        }
        self.xml.raw(" />");
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

// ---------------------------------------------------------------------------
// THE ZOO: one hand-written case per mapping row.
// ---------------------------------------------------------------------------
//
// The corpus oracle (tests/usx_corpus.rs) compares STRUCTURALLY, so it forgives
// attribute order and indentation. These cases compare the STRING, which is what
// pins the writer's fixed attribute order (`code`/`caller` before `style`,
// splatted attributes after it, `vid` last) and the self-closing spelling of an
// empty element.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cst, lex};

    /// `source` → its USX, with the root envelope stripped so a case reads as
    /// just the content it is about.
    fn content(source: &str) -> String {
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        let out = usx(source.as_bytes(), &tokens, &cst);
        let head = "<usx version=\"3.0\">";
        out.strip_prefix(head)
            .and_then(|rest| rest.strip_suffix("</usx>"))
            .unwrap_or_else(|| panic!("envelope missing from {out}"))
            .to_string()
    }

    fn whole(source: &str) -> String {
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        usx(source.as_bytes(), &tokens, &cst)
    }

    #[test]
    fn the_root_version_defaults_to_3_0_and_echoes_usfm() {
        assert_eq!(whole(""), r#"<usx version="3.0"></usx>"#);
        // `\usfm` is not dropped the way USJ drops it — its payload IS the
        // root version.
        assert_eq!(
            whole("\\usfm 3.1\n\\p x"),
            r#"<usx version="3.1"><para style="p">x</para></usx>"#
        );
    }

    #[test]
    fn book_keeps_its_code_and_self_closes_when_empty() {
        assert_eq!(content("\\id GEN"), r#"<book code="GEN" style="id" />"#);
        assert_eq!(
            content("\\id GEN Genesis"),
            r#"<book code="GEN" style="id">Genesis</book>"#
        );
    }

    #[test]
    fn paragraph_markers_keep_their_own_spelling_as_style() {
        // The row is shared (`q1` resolves to `q`, `mt2` to `mt`); the NUMBER is
        // the author's and comes off the token's bytes.
        assert_eq!(
            content("\\q1 line\n\\mt2 title\n\\ide UTF-8"),
            r#"<para style="q1">line</para><para style="mt2">title</para><para style="ide">UTF-8</para>"#
        );
        // Every empty element self-closes — USJ's `\b` special case has no
        // counterpart here, because XML spells empty content one way.
        assert_eq!(
            content("\\p one\\b\\p two"),
            r#"<para style="p">one</para><para style="b" /><para style="p">two</para>"#
        );
    }

    #[test]
    fn characters_nest_and_keep_the_nested_spelling_off_the_row() {
        assert_eq!(
            content("\\p \\add outer \\+nd inner\\+nd* tail\\add*"),
            r#"<para style="p"><char style="add">outer <char style="nd">inner</char> tail</char></para>"#
        );
    }

    #[test]
    fn a_note_lifts_its_caller_and_its_cat_out_of_content() {
        assert_eq!(
            content("\\p \\f + \\fr 1:1 \\ft note\\f*"),
            r#"<para style="p"><note caller="+" style="f"><char style="fr">1:1 </char><char style="ft">note</char></note></para>"#
        );
        assert_eq!(
            content("\\esb \\cat People\\cat*\n\\p who\n\\esbe"),
            r#"<sidebar style="esb" category="People"><para style="p">who</para></sidebar>"#
        );
    }

    #[test]
    fn a_chapter_and_a_verse_are_a_sid_element_and_an_eid_element() {
        assert_eq!(
            content("\\id GEN\n\\c 2\n\\p \\v 3-4 x"),
            r#"<book code="GEN" style="id" /><chapter number="2" style="c" sid="GEN 2" />"#
                .to_owned()
                + r#"<para style="p"><verse number="3-4" style="v" sid="GEN 2:3-4" />x<verse eid="GEN 2:3-4" /></para>"#
                + r#"<chapter eid="GEN 2" />"#
        );
        // No `\id`, no book code, so no sid to derive — and with no sid there is
        // nothing an eid could name either. The projection never invents one.
        assert_eq!(content("\\c 1"), r#"<chapter number="1" style="c" />"#);
        assert_eq!(
            content("\\p \\v 1 verse one\n\\v 2 verse two"),
            r#"<para style="p"><verse number="1" style="v" />verse one <verse number="2" style="v" />verse two</para>"#
        );
    }

    #[test]
    fn a_verse_crosses_paragraphs_and_they_carry_its_vid() {
        // The verse runs to the paragraph BEFORE the one that starts the next
        // verse, and every block between wears `vid` (basic/multiple-paragraphs).
        assert_eq!(
            content(
                "\\id JHN\n\\c 1\n\\p\n\\v 1 one\n\\v 2 two\n\\q1 more\n\\q2 still\n\\q1\n\\v 3 three"
            ),
            r#"<book code="JHN" style="id" /><chapter number="1" style="c" sid="JHN 1" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="JHN 1:1" />one <verse eid="JHN 1:1" /><verse number="2" style="v" sid="JHN 1:2" />two</para>"#
                + r#"<para style="q1" vid="JHN 1:2">more</para>"#
                + r#"<para style="q2" vid="JHN 1:2">still<verse eid="JHN 1:2" /></para>"#
                + r#"<para style="q1"><verse number="3" style="v" sid="JHN 1:3" />three<verse eid="JHN 1:3" /></para>"#
                + r#"<chapter eid="JHN 1" />"#
        );
    }

    #[test]
    fn a_trailing_heading_is_trimmed_off_the_verse() {
        // The `\s1` between two verses carries NO vid, and the eid stays in the
        // paragraph the verse started in (basic/section).
        assert_eq!(
            content("\\id GEN\n\\c 1\n\\p\n\\v 1 one\n\\s1 A heading\n\\p\n\\v 2 two"),
            r#"<book code="GEN" style="id" /><chapter number="1" style="c" sid="GEN 1" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="GEN 1:1" />one<verse eid="GEN 1:1" /></para>"#
                + r#"<para style="s1">A heading</para>"#
                + r#"<para style="p"><verse number="2" style="v" sid="GEN 1:2" />two<verse eid="GEN 1:2" /></para>"#
                + r#"<chapter eid="GEN 1" />"#
        );
    }

    #[test]
    fn a_sidebar_carries_no_vid_and_the_verse_survives_it() {
        // The sidebar's own paragraphs are outside the verse; the paragraph
        // AFTER `\esbe` resumes it (usfmjsTests/esb).
        assert_eq!(
            content("\\id GEN\n\\c 1\n\\p \\v 1 x\n\\esb \\p inside\n\\esbe\n\\p after"),
            r#"<book code="GEN" style="id" /><chapter number="1" style="c" sid="GEN 1" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="GEN 1:1" />x</para>"#
                + r#"<sidebar style="esb"><para style="p">inside</para></sidebar>"#
                + r#"<para style="p" vid="GEN 1:1">after<verse eid="GEN 1:1" /></para>"#
                + r#"<chapter eid="GEN 1" />"#
        );
    }

    #[test]
    fn a_chapter_eid_comes_after_the_chapters_last_block() {
        assert_eq!(
            content("\\id GEN\n\\c 1\n\\p \\v 1\n\\c 2\n\\p \\v 1 x"),
            r#"<book code="GEN" style="id" /><chapter number="1" style="c" sid="GEN 1" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="GEN 1:1" /><verse eid="GEN 1:1" /></para>"#
                + r#"<chapter eid="GEN 1" /><chapter number="2" style="c" sid="GEN 2" />"#
                + r#"<para style="p"><verse number="1" style="v" sid="GEN 2:1" />x<verse eid="GEN 2:1" /></para>"#
                + r#"<chapter eid="GEN 2" />"#
        );
    }

    #[test]
    fn ca_cp_va_vp_lift_onto_the_chapter_and_the_verse() {
        assert_eq!(
            content("\\id GEN\n\\c 1\n\\ca 2\\ca*\n\\cp M\n\\p \\v 1 \\va 3\\va* \\vp 1b\\vp* text"),
            r#"<book code="GEN" style="id" /><chapter number="1" style="c" sid="GEN 1" altnumber="2" pubnumber="M" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="GEN 1:1" altnumber="3" pubnumber="1b" />text<verse eid="GEN 1:1" /></para>"#
                + r#"<chapter eid="GEN 1" />"#
        );
        // No chapter to own it: the projection must not guess an owner, and lint
        // carries the placement complaint.
        assert_eq!(
            content("\\p \\cp M"),
            r#"<para style="p"><char style="cp">M</char></para>"#
        );
        // An attribute cannot hold markup, so a lift whose content is not plain
        // text does not happen at all (biblica/PublishingVersesWithFormatting).
        assert_eq!(
            content("\\p \\vp \\+it 21\\+it*\\vp* text"),
            r#"<para style="p"><char style="vp"><char style="it">21</char></char> text</para>"#
        );
    }

    #[test]
    fn milestones_keep_their_spelling_and_own_no_content() {
        assert_eq!(
            content("\\p \\ts-s |sid=\"x\"\\* a \\ts\\* \\zaln-e\\*"),
            r#"<para style="p"><ms style="ts-s" sid="x" /> a <ms style="ts" /> <ms style="zaln-e" /></para>"#
        );
    }

    #[test]
    fn a_u25003_container_emits_its_points_and_not_itself() {
        assert_eq!(
            content("\\list-s\\*\n\\li one\n\\list-e\\*"),
            r#"<ms style="list-s" /><para style="li">one </para><ms style="list-e" />"#
        );
    }

    #[test]
    fn consecutive_rows_share_one_synthesized_table() {
        // Two rows → ONE wrapper; the `\p` splits the run, so the third row gets
        // a wrapper of its own. `align` is the `r` in the ROW name. The wrapper
        // carries no `style` — it has no marker behind it.
        assert_eq!(
            content("\\tr \\tc1 a\\tcr2 b\n\\tr \\tc1 c\n\\p after\n\\tr \\th1 h"),
            r#"<table><row style="tr"><cell style="tc1" align="start">a</cell><cell style="tcr2" align="end">b</cell></row><row style="tr"><cell style="tc1" align="start">c</cell></row></table>"#
                .to_owned()
                + r#"<para style="p">after</para>"#
                + r#"<table><row style="tr"><cell style="th1" align="start">h</cell></row></table>"#
        );
        // A verse open across the table decorates the WRAPPER, and its eid lands
        // in the last cell (specExamples/table).
        assert_eq!(
            content("\\id MAT\n\\c 1\n\\p \\v 1 rows:\n\\tr \\tc1 a\n\\tr \\tc1 b"),
            r#"<book code="MAT" style="id" /><chapter number="1" style="c" sid="MAT 1" />"#
                .to_owned()
                + r#"<para style="p"><verse number="1" style="v" sid="MAT 1:1" />rows:</para>"#
                + r#"<table vid="MAT 1:1"><row style="tr"><cell style="tc1" align="start">a</cell></row>"#
                + r#"<row style="tr"><cell style="tc1" align="start">b<verse eid="MAT 1:1" /></cell></row></table>"#
                + r#"<chapter eid="MAT 1" />"#
        );
    }

    #[test]
    fn a_figure_renames_src_to_file() {
        assert_eq!(
            content("\\ip before \\fig caption|src=\"a.png\" size=\"col\"\\fig* after"),
            r#"<para style="ip">before <figure style="fig" file="a.png" size="col">caption</figure> after</para>"#
        );
    }

    #[test]
    fn ref_and_periph_carry_no_style_at_all() {
        assert_eq!(
            content("\\p see \\ref Mark 1:4|MRK 1:4\\ref* now"),
            r#"<para style="p">see <ref loc="MRK 1:4">Mark 1:4</ref> now</para>"#
        );
        assert_eq!(
            content("\\periph My Title|id=\"title\"\n\\p body"),
            r#"<periph alt="My Title" id="title"><para style="p">body</para></periph>"#
        );
    }

    #[test]
    fn an_unknown_marker_takes_the_para_shape_and_an_optbreak_is_its_own() {
        assert_eq!(
            content("\\s5\n\\p x"),
            r#"<para style="s5" /><para style="p">x</para>"#
        );
        // …unless its own `\*` spells it a milestone (`\zms\*`).
        assert_eq!(
            content("\\p a\\zms\\* b"),
            r#"<para style="p">a</para><ms style="zms" /> b"#
        );
        assert_eq!(
            content("\\p a // b"),
            r#"<para style="p">a <optbreak /> b</para>"#
        );
    }

    #[test]
    fn attributes_splat_and_the_later_definition_wins() {
        assert_eq!(
            content("\\p \\w In|in\\w*"),
            r#"<para style="p"><char style="w" lemma="in">In</char></para>"#
        );
        assert_eq!(
            content("\\p \\w x|lemma=\"a\" x-s=\"1\" lemma=\"b\"\\w*"),
            r#"<para style="p"><char style="w" lemma="b" x-s="1">x</char></para>"#
        );
    }

    #[test]
    fn whitespace_is_canonicalized_the_way_the_fixtures_are() {
        // A newline inside a paragraph is ONE space; a run of spaces collapses;
        // `~` is the non-breaking space it names.
        assert_eq!(
            content("\\p a\nb\n\\p c"),
            r#"<para style="p">a b</para><para style="p">c</para>"#
        );
        assert_eq!(content("\\p a  \t b"), r#"<para style="p">a b</para>"#);
        assert_eq!(content("\\p a~b"), "<para style=\"p\">a\u{a0}b</para>");
        // Whitespace in front of a BLOCK seam is layout, not text; in front of a
        // character marker or a closer it is text.
        assert_eq!(
            content("\\p x \\p y"),
            r#"<para style="p">x</para><para style="p">y</para>"#
        );
        assert_eq!(
            content("\\p \\k ostrich \\k*bird"),
            r#"<para style="p"><char style="k">ostrich </char>bird</para>"#
        );
        // A whitespace-only run IS content here; `usj.rs`'s twin drops it.
        assert_eq!(
            content("\\p \\add a\\add* \\add b\\add*"),
            r#"<para style="p"><char style="add">a</char> <char style="add">b</char></para>"#
        );
    }

    #[test]
    fn a_closed_span_nests_in_the_open_note_text_and_an_unclosed_one_stays_a_peer() {
        // An explicitly-closed span foreign to the note's family nests in the
        // open note text; an unclosed one stays a peer.
        assert_eq!(
            content("\\p \\f + \\ft alpha \\xt ref\\xt* beta\\f*"),
            r#"<para style="p"><note caller="+" style="f"><char style="ft">alpha <char style="xt">ref</char> beta</char></note></para>"#
        );
        assert_eq!(
            content("\\p \\x - \\xo 1.1 \\xt Ps 135\\x*"),
            r#"<para style="p"><note caller="-" style="x"><char style="xo">1.1 </char><char style="xt">Ps 135</char></note></para>"#
        );
    }

    #[test]
    fn the_writer_escapes_what_xml_requires() {
        // `<`, `&` and `>` in text, and a C0 control as a numeric reference.
        assert_eq!(
            content("\\p a < b & c > d \u{1}"),
            r#"<para style="p">a &lt; b &amp; c &gt; d &#x01;</para>"#
        );
        // The same three in an ATTRIBUTE value. `"` cannot be tested here: USFM
        // defines no escapes, so a quote inside a value ends it (attributes.rs).
        assert_eq!(
            content("\\p \\w x|lemma=\"a&b<c>d\"\\w*"),
            r#"<para style="p"><char style="w" lemma="a&amp;b&lt;c&gt;d">x</char></para>"#
        );
    }
}
