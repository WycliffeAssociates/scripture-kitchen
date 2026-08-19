//! The ANCESTRY machine: the two facts no single node knows — sidebar
//! containment and whether a paragraph stands above a verse.

use super::walk::is_structural_ws;
use super::{Code, Doc, Emit, Observation};
use crate::cst::Node;
use crate::tables::generated;
use crate::tables::schema::{MarkerKind, ScopeKind};
use crate::{Token, TokenKind};

const ROOT_TOKEN: u32 = u32::MAX;

/// The ANCESTRY machine: the two facts no single node knows — "am I inside a
/// sidebar" and "is there a paragraph above me".
///
/// It keeps no parallel copy of the walk's stack. Both depths are re-derived at
/// close from the node in hand (the same two table reads the open did), which
/// is what lets the driver own the stack outright — and is the property a
/// Builder-driven pipeline needs, since the Builder's frames are its own. The
/// one thing that cannot be re-derived is WHICH sidebar we are in, so the
/// innermost opener rides a stack of its own; it grows only on sidebars, which
/// are rare.
pub(crate) struct Ancestry {
    sidebars: u32,
    paragraphs: u32,
    sidebar_token: u32,
    /// The enclosing sidebar's opener, restored on pop, so nested sidebars name
    /// the innermost one.
    enclosing: Vec<u32>,
    /// One `\p` repairs a whole run of paragraph-less verses, so the run gets
    /// ONE finding, anchored where that `\p` would go. Reported per verse this
    /// rule cries wolf: `\c 2` with no `\p` lights up every verse in the
    /// chapter (614 → 34 across en_ult), which buries the real signal.
    run_reported: bool,
}

/// [`Frame::scratch`] bits: what this node contributes to the two depths.
const IS_SIDEBAR: u8 = 1;
const IS_PARAGRAPH: u8 = 2;

impl Ancestry {
    pub(crate) fn new() -> Self {
        Self {
            sidebars: 0,
            paragraphs: 0,
            sidebar_token: ROOT_TOKEN,
            enclosing: Vec::new(),
            run_reported: false,
        }
    }

    /// Reads the node's row ONCE and hands the answer back to the driver as
    /// [`Frame::scratch`]; the close event gets it for free rather than paying
    /// for the same two table reads again.
    #[inline]
    pub(crate) fn on_node_open(&mut self, doc: &Doc, node: &Node) -> u8 {
        let marker_idx = doc.tokens[node.token as usize].marker_idx;
        let sidebar = generated::opens_scope(marker_idx) == Some(ScopeKind::Sidebar);
        // Cells count: a verse inside a table cell is inside a paragraph for
        // every purpose this rule has.
        let para = matches!(
            generated::kind(marker_idx),
            MarkerKind::Paragraph | MarkerKind::TableCell
        );
        self.sidebars += u32::from(sidebar);
        self.paragraphs += u32::from(para);
        if para {
            self.run_reported = false;
        }
        if sidebar {
            self.enclosing.push(self.sidebar_token);
            self.sidebar_token = node.token;
        }
        u8::from(sidebar) | (u8::from(para) << 1)
    }

    #[inline]
    pub(crate) fn on_node_close(&mut self, scratch: u8) {
        self.sidebars -= u32::from(scratch & IS_SIDEBAR != 0);
        self.paragraphs -= u32::from(scratch & IS_PARAGRAPH != 0);
        if scratch & IS_SIDEBAR != 0 {
            self.sidebar_token = self.enclosing.pop().unwrap_or(ROOT_TOKEN);
        }
    }

    #[inline]
    pub(crate) fn on_leaf(
        &mut self,
        doc: &Doc,
        idx: u32,
        token: &Token,
        kind: TokenKind,
        out: &mut Emit,
    ) {
        if !matches!(kind, TokenKind::Marker { .. }) {
            return;
        }
        match generated::kind(token.marker_idx) {
            MarkerKind::Chapter => {
                if self.sidebars > 0 {
                    out.push(Observation::pair(
                        Code::ContentOutsideSidebarRule,
                        idx,
                        self.sidebar_token,
                    ));
                }
                // A run cannot cross `\c`: the chapter displaces any open
                // paragraph, so a `\p` inserted in the previous chapter
                // repairs nothing here — each offending chapter is its own
                // run and gets its own finding.
                self.run_reported = false;
            }
            MarkerKind::Verse => {
                if self.sidebars > 0 {
                    out.push(Observation::pair(
                        Code::ContentOutsideSidebarRule,
                        idx,
                        self.sidebar_token,
                    ));
                }
                if self.paragraphs == 0 && !self.run_reported {
                    self.run_reported = true;
                    // The `\p` usfmtc fabricates, PROPOSED instead: inserted in
                    // front of the verse that starts the run, which is where the
                    // one repairing paragraph belongs. The leading newline is
                    // conditional because without it a glued `\v` (`text\v 1`)
                    // would trade this finding for a `marker-not-ws-preceded` on
                    // the `\p` we just proposed — a fix must not hand back a new
                    // finding, and the oracle would say so.
                    let at = token.start;
                    let text: &[u8] = if at == 0 || is_structural_ws(doc.source[at as usize - 1]) {
                        b"\\p\n"
                    } else {
                        b"\n\\p\n"
                    };
                    out.push_fixed(Observation::one(Code::MissingParagraph, idx), at, at, text);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::tests::{findings, token_named};

    #[test]
    fn content_outside_sidebar_rule() {
        // (The line break before the last `\p` is load-bearing since phase 3:
        // a paragraph marker glued to the preceding word is a Form finding of
        // its own, and this test is not about that.)
        let (tokens, obs) = findings("\\p out\\esb \\p in \\c 1 more\\esbe\n\\p after");
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::ContentOutsideSidebarRule,
                token_named(&tokens, "c", 0),
                token_named(&tokens, "esb", 0),
            )]
        );
    }

    #[test]
    fn missing_paragraph() {
        let (tokens, obs) = findings("\\c 1\n\\v 1 no paragraph here");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingParagraph,
                token_named(&tokens, "v", 0)
            )]
        );

        // Inside a paragraph, and inside a table cell, both silent.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 fine\n\\tr \\tc1 \\v 2 also fine");
        assert_eq!(obs, vec![]);

        // A run never crosses `\c` — one `\p` cannot repair both chapters,
        // so consecutive offending chapters each get their own finding.
        let (tokens, obs) = findings("\\c 1\n\\v 1 a \\v 2 b\n\\c 2\n\\v 1 c");
        assert_eq!(
            obs,
            vec![
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 0)),
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 2)),
            ]
        );
    }
}
