//! The ANCESTRY machine: the two facts no single node knows — sidebar
//! containment and whether a paragraph stands above a verse.
//!
//! ```text
//! \esb \p in \c 1 more\esbe   →  content-outside-sidebar-rule @ \c   (owner: \esb)
//! \c 1 \v 1 no paragraph      →  missing-paragraph @ \v              (fix: insert \p)
//! \c 1 \p \v 1 fine           →  (silent; a \tc cell counts as a paragraph too)
//! \c 1 \qa ALEPH \v 1 text    →  missing-paragraph @ \v   (a heading holds no verse)
//! ```

use super::walk::is_structural_ws;
use super::{Code, Doc, Emit, Observation};
use crate::cst::Node;
use crate::tables::generated;
use crate::tables::schema::{MarkerKind, ScopeKind};
use crate::{Token, TokenKind};

const ROOT_TOKEN: u32 = u32::MAX;

/// Two depths — "am I inside a sidebar", "is there a paragraph above me" — and
/// no parallel copy of the walk's stack: each depth is undone at close from the
/// bits the open already computed, which is what lets the driver own the stack
/// outright. The one thing that cannot be re-derived is WHICH sidebar we are in,
/// so the innermost opener rides a stack of its own; it grows only on sidebars,
/// which are rare.
pub(crate) struct Ancestry {
    sidebars: u32,
    paragraphs: u32,
    sidebar_token: u32,
    /// The enclosing sidebar's opener, restored on pop, so nested sidebars name
    /// the innermost one.
    enclosing: Vec<u32>,
    /// One `\p` repairs a whole run of paragraph-less verses, so the run gets
    /// ONE finding, anchored where that `\p` would go. Per verse the rule cries
    /// wolf — a `\p`-less chapter lights up every verse in it (614 → 34 across
    /// en_ult), which buries the real signal.
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
    /// [`Frame::scratch`], so the close event pays for no table reads at all.
    #[inline]
    pub(crate) fn on_node_open(&mut self, doc: &Doc, node: &Node) -> u8 {
        let marker_idx = doc.tokens[node.token as usize].marker_idx;
        let sidebar = generated::opens_scope(marker_idx) == Some(ScopeKind::Sidebar);
        // VERSE-BEARING paragraphs only. A `\v` under `\s1`/`\ip`/`\qa` is
        // structurally heading or introduction text — the spec's own
        // OtherPara/SectionPara enums forbid a verse there — so such a
        // paragraph does not satisfy "a paragraph stands above me".
        // Cells count: a verse inside a table cell is inside a paragraph for
        // every purpose this rule has.
        let para = match generated::kind(marker_idx) {
            MarkerKind::Paragraph => !generated::forbids_verse(marker_idx),
            MarkerKind::TableCell => true,
            _ => false,
        };
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
                // paragraph, so a `\p` in the previous chapter repairs nothing
                // here — each offending chapter is its own run.
                self.run_reported = false;
            }
            // Row 0 is the walker's POP-ALL recovery, so it kills the standing
            // paragraph exactly as `\c` does: a `\p` proposed in front of it
            // repairs nothing beyond it, which makes the verses after it a run
            // of their own. en_ulb's `\s5` is 13,636 occurrences of this.
            MarkerKind::Unknown => self.run_reported = false,
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
                    // The `\p` usfmtc fabricates, PROPOSED instead, in front of
                    // the verse that starts the run. The leading newline is
                    // conditional: without it a glued `\v` (`text\v 1`) would
                    // trade this finding for a `marker-not-ws-preceded` on the
                    // `\p` we just proposed, and a fix must not hand back a new
                    // finding.
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
    use crate::lint::tests::{codes, findings, token_named};

    #[test]
    fn content_outside_sidebar_rule() {
        // The line break before the last `\p` is load-bearing: a paragraph
        // marker glued to the preceding word is a Form finding of its own.
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

        // A run never crosses `\c`: one finding per offending chapter.
        let (tokens, obs) = findings("\\c 1\n\\v 1 a \\v 2 b\n\\c 2\n\\v 1 c");
        assert_eq!(
            obs,
            vec![
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 0)),
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 2)),
            ]
        );
    }

    /// A line feed closes nothing, so the verse under a section heading nests
    /// INSIDE it — and `\s1` may not hold a verse, so the heading does not
    /// count as the paragraph above. Will's demo document, byte for byte.
    #[test]
    fn a_verse_under_a_heading_has_no_paragraph_above_it() {
        let (tokens, obs) = findings(
            "\\c 1\n\\s1 Hidden pieces are not places\n\n\\v 1 Put the caret in the slot\n",
        );
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingParagraph,
                token_named(&tokens, "v", 0)
            )]
        );

        // The heading BEHIND that finding: `\s1` does not hold the verse at all
        // — the walker's context masks displace it to the root — so the counter
        // never sees the heading, and this shape already fired. The same is
        // true of `\ip`, `\r`, `\cl`, `\sp`, `\ms`.
        for heading in ["\\ip intro", "\\r (Matt 1:1)", "\\cl Psalm", "\\sp David"] {
            let (_, obs) = findings(&format!("\\c 1\n{heading}\n\\v 1 text\n"));
            assert_eq!(codes(&obs), vec![Code::MissingParagraph], "{heading}");
        }

        // `\qa` is where the PREDICATE does the work: an acrostic heading is a
        // poetry row, so the verse NESTS inside it and the old kind-only test
        // counted it as the paragraph above. It is v-forbidden, so it is not.
        let (tokens, obs) = findings("\\c 1\n\\qa ALEPH\n\\v 1 text\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingParagraph,
                token_named(&tokens, "v", 0)
            )]
        );

        // A VERSE-BEARING paragraph still satisfies it.
        for para in ["\\p", "\\q1", "\\m", "\\li1"] {
            let (_, obs) = findings(&format!("\\c 1\n{para}\n\\v 1 text\n"));
            assert_eq!(codes(&obs), Vec::<Code>::new(), "{para}");
        }

        // The corpus shapes both headings are actually written in — a
        // verse-bearing paragraph between the heading and the verse — stay
        // silent, which is why 226 books move by nothing.
        for lead in ["\\qa ALEPH", "\\d A psalm of David"] {
            let (_, obs) = findings(&format!("\\c 1\n{lead}\n\\q1 \\v 1 text\n"));
            assert_eq!(codes(&obs), Vec::<Code>::new(), "{lead}");
        }
    }

    /// The close side is the open side's bit, so the depth comes back down
    /// exactly where it went up: a verse AFTER the heading's paragraph is
    /// silent, and one after that paragraph closes is not.
    #[test]
    fn the_heading_exclusion_is_symmetric_at_close() {
        let (_, obs) = findings("\\c 1\n\\s1 head\n\\p \\v 1 a\n\\v 2 b\n");
        assert_eq!(codes(&obs), Vec::<Code>::new());

        // `\s1` displaces the `\p`, so the verse under it is bare again — one
        // finding, at the verse that starts the new run.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\s1 head\n\\v 2 b\n\\v 3 c\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingParagraph,
                token_named(&tokens, "v", 1)
            )]
        );
    }
}
