//! The FLAT machine: every rule whose evidence is a token row, its span, or the
//! one token before it — row lookup, form, payload, adjacency, version, the
//! positional band. The ATTRIBUTE arm keeps its own state cluster, in
//! [`super::attr_rules`].

use super::attr_rules::AttrRules;
use super::rows::version_row;
use super::walk::{is_structural_ws, span_of};
use super::{Code, Doc, Emit, NO_TOKEN, Observation, UsfmVersion};
use crate::scanner::payload_label;
use crate::tables::books;
use crate::tables::generated;
use crate::tables::schema::{
    MarkerKind, Numbering, SpecContext, SpellingShape, StructuralWhitespaceRequirement as Ws,
};
use crate::{Token, TokenKind};

/// Which designator family an annotation may still follow here.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Window {
    Closed,
    Chapter,
    Verse,
}

/// Fed one leaf at a time. Its lookbehind lives in fields rather than in four
/// separate sweeps' locals for a measured reason: a token sweep over this corpus
/// costs ~2.5 ns/token in dispatch alone however little each arm does, so a
/// family that needs no more than the last marker earns no traversal of its own.
///
/// `\ca`/`\cp` are legal immediately after `\c`'s designator or after each other
/// and `\va`/`\vp` after `\v`, where "immediately" allows whitespace between —
/// the spec's own examples put `\cp` on its own line. They open no scope, so the
/// CST cannot see their misplacement; the `window` is where the fact lives.
pub(crate) struct Flat {
    version: Option<UsfmVersion>,
    /// The four adjacency rows, resolved once instead of per token.
    ca: generated::MarkerIdx,
    cp: generated::MarkerIdx,
    va: generated::MarkerIdx,
    vp: generated::MarkerIdx,
    window: Window,
    attrs: AttrRules,
    /// How far along the POSITIONAL band the document has come, as a
    /// `SpecContext` discriminant (0 = `Scripture`, the state a book opens in).
    /// Monotonic, so the whole judge is a mask AND and a `trailing_zeros`.
    band: u8,
    /// Levels-seen per numbered family, indexed by ROW: bit 0 = the bare
    /// spelling, bit n = `\q<n>`, bit 15 = "already reported". An array rather
    /// than a map because the row index IS the family key — 306 bytes, no
    /// hashing. `first_seen` is read only where the mask says the family was
    /// seen, so it needs no sentinel.
    levels: [u16; generated::ROW_COUNT],
    first_seen: [u32; generated::ROW_COUNT],
}

const REPORTED: u16 = 1 << 15;

/// The context mask's POSITIONAL half, the only bits the band judge may look at.
/// Folded from the enum so a new positional variant arrives without an edit.
const POSITIONAL: u32 = {
    let mut mask = 0;
    let mut at = 0;
    while at < SpecContext::ALL.len() {
        let ctx = SpecContext::ALL[at];
        if ctx.is_positional() {
            mask |= generated::context_bit(ctx);
        }
        at += 1;
    }
    mask
};

impl Flat {
    pub(crate) fn new(version: Option<UsfmVersion>) -> Self {
        let idx_of = |name: &[u8]| generated::marker_idx(name, SpellingShape::PlainOnly);
        Self {
            version,
            ca: idx_of(b"ca"),
            cp: idx_of(b"cp"),
            va: idx_of(b"va"),
            vp: idx_of(b"vp"),
            window: Window::Closed,
            attrs: AttrRules::new(),
            band: 0,
            levels: [0u16; generated::ROW_COUNT],
            first_seen: [0u32; generated::ROW_COUNT],
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
        let source = doc.source;
        let (ca, cp, va, vp) = (self.ca, self.cp, self.va, self.vp);
        match kind {
            // Byte-exact membership first, then the case-folded retry: exactly
            // one of the two findings fires.
            TokenKind::BookCode => {
                // The label alone: the folded delimiter is not part of the code,
                // and the case-fold splice below must not overwrite it.
                let span = payload_label(span_of(source, token));
                if books::is_book_code(span) {
                    return;
                }
                let folded = books::upper3(span).filter(|upper| books::is_book_code(upper));
                match folded {
                    // The one payload fix: the code IS a book identifier, so
                    // case-folding its three bytes is a splice and not a guess.
                    Some(upper) => out.push_fixed(
                        Observation::one(Code::BookCodeNotUppercase, idx),
                        token.start,
                        token.start + span.len() as u32,
                        &upper,
                    ),
                    None => out.push(Observation::one(Code::BookCodeUnknown, idx)),
                }
            }
            // Openers and milestones share this arm: to every rule below they
            // are one thing, the marker a list, a level or a designator belongs
            // to. `nested` binds the spelling bit (`\+w`'s `+`, `\qt-e`'s `e`).
            TokenKind::Marker { nested } | TokenKind::Milestone { end: nested } => {
                let marker_idx = token.marker_idx;
                let opener = matches!(kind, TokenKind::Marker { .. });
                if opener {
                    if marker_idx == generated::UNRESOLVED {
                        out.push(Observation::one(Code::UnknownMarker, idx));
                    } else if nested && generated::kind(marker_idx) != MarkerKind::Character {
                        out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                    }
                    if generated::kind(marker_idx) == MarkerKind::Paragraph
                        && token.start > 0
                        && !is_structural_ws(source[token.start as usize - 1])
                    {
                        // A NEWLINE, not a space: the PARA railroad's newline
                        // branch is canonical, while its `/${Ws}\\/` branch (Ws
                        // zero-or-more) makes hugging legal — hence Hint.
                        out.push_fixed(
                            Observation::one(Code::MarkerNotWsPreceded, idx),
                            token.start,
                            token.start,
                            b"\n",
                        );
                    }
                    // "The span absorbed no delimiter" is ONE byte to check: a
                    // marker name never ends in whitespace, so a trailing space
                    // or tab can only be the folded delimiter run.
                    if wants_delimiter(marker_idx)
                        && !span_of(source, token)
                            .last()
                            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                        && source
                            .get(token.end() as usize)
                            .is_some_and(|byte| !is_delimiter_byte(*byte))
                    {
                        out.push(Observation::one(Code::DelimiterShape, idx));
                    }

                    // Stay if this marker is legal where we are; else advance to
                    // the LOWEST of its positional contexts above us; else every
                    // one is behind us and that is the finding. mt#/cl/ip's dual
                    // contexts answer themselves.
                    //
                    // A mask carrying any CONTAINER bit abstains — that is the
                    // lane's boundary, not a noise filter. What a container
                    // licenses is judged by the WALKER's mask at pop time (the
                    // displacement axis), so the band speaks only for markers
                    // whose sole license is where the book has got to.
                    let mask = generated::context_mask(marker_idx);
                    let positional = mask & POSITIONAL;
                    if positional != 0
                        && mask & !POSITIONAL == 0
                        && positional & (1 << self.band) == 0
                    {
                        let above = positional & !((1 << (self.band + 1)) - 1);
                        if above == 0 {
                            out.push(Observation::one(Code::MarkerOutOfBand, idx));
                        } else {
                            self.band = above.trailing_zeros() as u8;
                        }
                    }

                    // The row's own BOOL is the filter; the five-row table says
                    // since when. Reported at the OPENER only — a closer is the
                    // same occurrence, and the fix rewrites both halves here.
                    if generated::deprecated(marker_idx) {
                        self.deprecated_marker(doc, idx, marker_idx, out);
                    }
                }

                let (code, opens) = if marker_idx == ca || marker_idx == cp {
                    (Some(Code::CaCpPlacement), Window::Chapter)
                } else if marker_idx == va || marker_idx == vp {
                    (Some(Code::VaVpPlacement), Window::Verse)
                } else {
                    (None, Window::Closed)
                };
                self.window = match code {
                    Some(code) => {
                        if self.window != opens {
                            out.push(Observation::one(code, idx));
                        }
                        // Open either way: `\ca 2\ca*\cp א` is one legal run,
                        // and re-reporting each member of a misplaced run turns
                        // one slip into three findings.
                        opens
                    }
                    None => match generated::kind(marker_idx) {
                        MarkerKind::Chapter => Window::Chapter,
                        MarkerKind::Verse => Window::Verse,
                        _ => Window::Closed,
                    },
                };

                if has_levels(marker_idx) {
                    let slot = marker_idx as usize;
                    if self.levels[slot] == 0 {
                        self.first_seen[slot] = idx;
                    }
                    self.levels[slot] |= 1 << spelled_level(span_of(source, token)).min(14);
                    let family = self.levels[slot];
                    // Bare AND numbered, said once per family per book.
                    if family & REPORTED == 0 && family & 1 != 0 && family & !(REPORTED | 1) != 0 {
                        self.levels[slot] |= REPORTED;
                        out.push(Observation {
                            code: Code::NumberingMix,
                            anchor: idx,
                            second: self.first_seen[slot],
                            aux: match generated::numbering(marker_idx) {
                                Numbering::UpTo(cap) => u32::from(cap),
                                // `liv` alone: numbered with no cap stated.
                                _ => 0,
                            },
                        });
                    }
                }

                self.attrs.on_marker(idx, marker_idx, opener, nested);
            }
            // Not an enumeration of `+`/`-`/`?`: those are conventions on one
            // general run, and up to three bytes is taken as deliberate.
            TokenKind::NoteCaller => {
                if payload_label(span_of(source, token)).len() > 3 {
                    out.push(Observation::one(Code::CallerShape, idx));
                }
                self.window = Window::Closed;
            }
            // Whether a closer closed anything is [`Structure`]'s question: the
            // answer is a property of the node about to close, one event later.
            TokenKind::ClosingMarker { nested } => {
                if nested
                    && token.marker_idx != generated::UNRESOLVED
                    && generated::kind(token.marker_idx) != MarkerKind::Character
                {
                    out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                }
                // A closer ends the attribute owner's reach and leaves the
                // adjacency `window` alone: `\ca 2\ca*\cp א` is one legal run.
                self.attrs.close_reach();
            }
            TokenKind::MilestoneTerminator => self.attrs.on_milestone_terminator(out),
            TokenKind::AttrList => self.attrs.on_attr_list(doc, idx, token, self.version, out),
            TokenKind::Text => {
                self.attrs.on_text(doc, idx, token, out);
                // The guard is load-bearing: `\c 1 \ca` is the only shape that
                // cares whether a Text run is blank, so asking unconditionally
                // reads every content byte for a one-in-ten-thousand fact.
                if self.window != Window::Closed
                    && !span_of(source, token).iter().all(|b| is_structural_ws(*b))
                {
                    self.window = Window::Closed;
                }
            }
            // Ends the ATTRIBUTE machine's reach and deliberately leaves the
            // adjacency window open: `\c 1` and its `\cp` are conventionally
            // written on separate lines.
            TokenKind::Newline => self.attrs.close_reach(),
            // The designator of whatever opened the window belongs to the run;
            // an optional break does not.
            TokenKind::Designator => {}
            TokenKind::OptBreak => self.window = Window::Closed,
        }
    }

    /// A deprecated marker, and the rename that repairs it where the spec's
    /// replacement is a rename. `#[inline(never)]` behind
    /// `generated::deprecated`: five rows in 153 reach it, so the cost on every
    /// other marker is one bitfield test.
    #[inline(never)]
    fn deprecated_marker(
        &self,
        doc: &Doc,
        idx: u32,
        marker_idx: generated::MarkerIdx,
        out: &mut Emit,
    ) {
        let Some(row) = version_row(generated::name(marker_idx)) else {
            return;
        };
        // THE GATE: a book that declares no version is a book of its own era,
        // and `\addpn` is correct in 2.x.
        if !self
            .version
            .is_some_and(|declared| declared >= row.deprecated_in)
        {
            return;
        }
        let observation = Observation {
            code: Code::DeprecatedMarker,
            anchor: idx,
            second: NO_TOKEN,
            aux: row.deprecated_in as u32,
        };
        match row.replacement {
            Some(replacement) => super::fix::rename(doc, out, observation, replacement),
            None => out.push(observation),
        }
    }
}

/// What may legally follow a marker name: structural whitespace, or one of
/// `TAGEND`'s two other alternatives (a marker, an attribute list).
fn is_delimiter_byte(byte: u8) -> bool {
    is_structural_ws(byte) || matches!(byte, b'\\' | b'|')
}

/// Does this row's `ws_after_name` REQUIRE something after the name? The
/// optional forms (milestones' `Hs`, row 0's `NotRequired`) never can.
fn wants_delimiter(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::ws_after_name(marker_idx),
        Ws::AtLeastOneHorizontalWhitespace
            | Ws::AtLeastOneWhitespace
            | Ws::SingleNewline
            | Ws::AtLeastOneNewline
            | Ws::TagEndDelimiter
    )
}

/// The level digit an occurrence was SPELLED with — `\q2` → 2, `\q` → 0,
/// `\qt3-s` → 3. Read off the span because rows are canonical (`q`, not `q1`),
/// so the token's own bytes are the sole record of the spelling used.
fn spelled_level(span: &[u8]) -> u8 {
    let from = usize::from(span.get(1) == Some(&b'+')) + 1;
    let mut level = 0u8;
    for byte in &span[from.min(span.len())..] {
        match byte {
            b'0'..=b'9' => level = level.saturating_mul(10).saturating_add(byte - b'0'),
            b'a'..=b'z' if level == 0 => {}
            _ => break,
        }
    }
    level
}

/// Is this row numbered in the sense `numbering-mix` cares about — a family
/// whose digit is a LEVEL? `TableColumns` (`\tc1`, `\tc1-2`) is excluded: those
/// digits are payload, so `\tc` beside `\tc2` is not two spellings of one thing.
fn has_levels(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::numbering(marker_idx),
        Numbering::UpTo(_) | Numbering::Unbounded
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::Severity;
    use crate::lint::tests::{codes, findings, token_named};

    #[test]
    fn unknown_marker() {
        let (tokens, obs) = findings("\\p a \\zfoo b");
        let unknown = tokens
            .iter()
            .position(|t| {
                t.marker_idx == generated::UNRESOLVED
                    && matches!(t.kind(), TokenKind::Marker { .. })
            })
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::UnknownMarker, unknown)]);
    }

    #[test]
    fn nested_spelling_misuse() {
        // `\+f` resolves to the `f` row (shape Any), which is a NOTE — the
        // nested spelling belongs to character markers alone.
        let (tokens, obs) = findings("\\p a \\+f + \\ft n\\+f*");
        assert_eq!(
            codes(&obs),
            vec![Code::NestedSpellingMisuse, Code::NestedSpellingMisuse]
        );
        assert_eq!(obs[0].anchor, token_named(&tokens, "f", 0));

        let (_, obs) = findings("\\p \\add a \\+nd b\\+nd* c\\add*");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn book_codes_are_checked_against_the_books_table() {
        let book_code = |tokens: &[Token]| {
            tokens
                .iter()
                .position(|t| t.kind() == TokenKind::BookCode)
                .unwrap() as u32
        };

        let (_, obs) = findings("\\id 1JN Some description\n\\c 1\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);
        // Peripherals count as book identifiers too.
        let (_, obs) = findings("\\id XXA\n\\p a");
        assert_eq!(obs, vec![]);

        // Known but mis-cased: exactly ONE finding, and not `unknown`.
        let (tokens, obs) = findings("\\id gen\n\\c 1\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::BookCodeNotUppercase,
                book_code(&tokens)
            )]
        );
        let (_, obs) = findings("\\id Gen\n\\c 1\n\\p \\v 1 a");
        assert_eq!(codes(&obs), vec![Code::BookCodeNotUppercase]);

        let (tokens, obs) = findings("\\id ZZZ\n\\c 1\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(Code::BookCodeUnknown, book_code(&tokens))]
        );
        // `\id GENESIS` carves `GENESIS` as the code — one span up to the first
        // space — which is simply not an identifier.
        let (_, obs) = findings("\\id GENESIS\n\\c 1\n\\p \\v 1 a");
        assert_eq!(codes(&obs), vec![Code::BookCodeUnknown]);
    }

    /// The finding list is compared WHOLE: `ca`/`va`/`vp` open scopes, so a
    /// well-formed `\ca 2\ca*` draws no `orphan-closer` to filter out.
    #[test]
    fn ca_and_cp_must_follow_their_chapter() {
        let (_, obs) = findings("\\c 1 \\ca 2\\ca*\n\\cp \u{5d0}\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\c 1\n\\cp \u{5d0}\n\\ca 2\\ca*\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);

        // Real content between them closes the window.
        let (tokens, obs) = findings("\\c 1\n\\p text\n\\ca 2\\ca*\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::CaCpPlacement,
                token_named(&tokens, "ca", 0)
            )]
        );

        // A whole run out of place is ONE finding, not one per member.
        let (tokens, obs) = findings("\\p text\n\\ca 2\\ca*\n\\cp \u{5d0}\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::CaCpPlacement,
                token_named(&tokens, "ca", 0)
            )]
        );
    }

    #[test]
    fn va_and_vp_must_follow_their_verse() {
        let (_, obs) = findings("\\c 1\n\\p \\v 1 \\va 2\\va* \\vp 1-2\\vp* text");
        assert_eq!(obs, vec![]);

        // The designator and a line break both keep the window open.
        let (_, obs) = findings("\\c 1\n\\p \\v 1\n\\va 2\\va*\n");
        assert_eq!(obs, vec![]);

        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 text \\va 2\\va*");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VaVpPlacement,
                token_named(&tokens, "va", 0)
            )]
        );

        // A `\va` after a CHAPTER is still misplaced — the two windows are
        // separate machines, not one "designator" window.
        let (tokens, obs) = findings("\\c 1 \\va 2\\va*\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VaVpPlacement,
                token_named(&tokens, "va", 0)
            )]
        );
    }

    /// An UNCLOSED `\ca` is a real finding — `unclosed-char`, with the
    /// insert-`\ca*` fix — and not silence.
    #[test]
    fn an_unclosed_chapter_annotation_is_an_unclosed_char() {
        // Displaced by the next chapter: the row is RequiredExplicit, so the
        // walker stamps Recovery and lint reads it off the node.
        let (tokens, obs) = findings("\\c 1\n\\ca 2\n\\c 2\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnclosedChar,
                token_named(&tokens, "ca", 0)
            )]
        );
    }

    #[test]
    fn caller_shape_only_reports_the_accidents() {
        for caller in ["+", "-", "?", "*", "\",", "abc"] {
            let (_, obs) = findings(&format!("\\p a\\f {caller} \\ft n\\f*"));
            assert_eq!(obs, vec![], "caller {caller:?}");
        }

        // `\f +note`: the space was forgotten, so the word lexed as caller.
        let (tokens, obs) = findings("\\p a\\f +note \\ft n\\f*");
        let caller = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::NoteCaller)
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::CallerShape, caller)]);
    }

    /// `numbering-out-of-range` is NOT a code because an over-cap level never
    /// reaches its family's row: `marker_idx` validates the digits, so `\q7` IS
    /// row 0. Should this fail, the rule became reachable and wants writing.
    #[test]
    fn an_over_cap_level_is_an_unknown_marker_not_a_range_finding() {
        let (tokens, obs) = findings("\\c 1\n\\q7 poetry\n");
        let over_cap = tokens
            .iter()
            .position(|t| {
                matches!(t.kind(), TokenKind::Marker { .. })
                    && t.marker_idx == generated::UNRESOLVED
            })
            .unwrap() as u32;
        assert_eq!(codes(&obs), vec![Code::UnknownMarker]);
        assert_eq!(obs[0].anchor, over_cap);
        let (_, obs) = findings("\\c 1\n\\q4 poetry\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn numbering_mix_is_one_finding_per_family_per_book() {
        let (tokens, obs) = findings("\\c 1\n\\q a\n\\q1 b\n\\q2 c\n\\q d\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::NumberingMix,
                anchor: token_named(&tokens, "q", 1),
                second: token_named(&tokens, "q", 0),
                aux: 4,
            }]
        );

        let (_, obs) = findings("\\c 1\n\\q1 a\n\\q2 b\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\c 1\n\\q a\n\\q b\n");
        assert_eq!(obs, vec![]);

        // Two families mixing is two findings — never one per occurrence.
        let (_, obs) = findings("\\c 1\n\\q a\n\\q1 b\n\\q c\n\\s d\n\\s1 e\n\\s f\n");
        assert_eq!(codes(&obs), vec![Code::NumberingMix, Code::NumberingMix]);

        // `\tc1` is a COLUMN, not a level: `\tc` beside it is not a mix.
        let (_, obs) = findings("\\c 1\n\\tr \\tc a \\tc2 b\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn marker_not_ws_preceded_is_paragraphs_only() {
        let (tokens, obs) = findings("\\p text\\s1 heading\n\\p more");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerNotWsPreceded,
                token_named(&tokens, "s", 0)
            )]
        );

        // Character markers legitimately hug — aligned USFM is built of it.
        let (_, obs) = findings("\\p \\w grace\\+nd deep\\+nd*\\w*\\add x\\add*");
        assert_eq!(obs, vec![]);

        let (_, obs) = findings("\\id GEN\n\\p a");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn delimiter_shape_reports_a_non_whitespace_separator() {
        // The live corpus finding: en_ulb REV writes `\m(for fine linen…`.
        let (tokens, obs) = findings("\\p a\n\\m(for fine linen)\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DelimiterShape,
                token_named(&tokens, "m", 0)
            )]
        );

        let (tokens, obs) = findings("\\p a\n\\q\u{00A0}poetry\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DelimiterShape,
                token_named(&tokens, "q", 0)
            )]
        );

        // Everything `TAGEND` allows, end of file included.
        for usfm in [
            "\\p text",
            "\\p\ttext",
            "\\p\n\\p text",
            "\\p\\v 1 text",
            "\\p|cat=\"x\"| text",
            "\\c 1\n\\p a\n\\b\n\\p b",
        ] {
            let (_, obs) = findings(usfm);
            assert!(
                !codes(&obs).contains(&Code::DelimiterShape),
                "{usfm:?} reported a delimiter"
            );
        }
    }

    #[test]
    fn a_deprecated_marker_is_gated_on_a_declared_version() {
        // THE GATE: no `\usfm` line means a book of its own era, and `\pro` is
        // correct in it.
        let (_, obs) = findings("\\p \\pro x\\pro*\n");
        assert_eq!(obs, vec![]);

        let usfm = "\\id GEN\n\\usfm 3.0\n\\p \\pro x\\pro*\n";
        let (tokens, obs) = findings(usfm);
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::DeprecatedMarker,
                anchor: token_named(&tokens, "pro", 0),
                second: NO_TOKEN,
                aux: UsfmVersion::V3_0 as u32,
            }]
        );
        // Reported at the OPENER only: the closer is the same occurrence.
        assert_eq!(obs.len(), 1);

        let row = Code::DeprecatedMarker.row();
        assert_eq!(row.severity_at(None), None);
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V3_0)),
            Some(Severity::Warning)
        );
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V4_0)),
            Some(Severity::Error)
        );

        // A numbered spelling of a deprecated row: one finding, digits and all.
        let (tokens, obs) = findings("\\id GEN\n\\usfm 3.2\n\\c 1\n\\ph2 hanging\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::DeprecatedMarker,
                anchor: token_named(&tokens, "ph", 0),
                second: NO_TOKEN,
                aux: UsfmVersion::V3_0 as u32,
            }]
        );
    }

    /// The family's other half: an attribute the row marks
    /// [`AttrStatus::Deprecated`] — `\xt`'s `link-href`, `\jmp`'s `link-` trio.
    #[test]
    fn a_deprecated_attribute_is_read_off_the_row_with_no_version_gate() {
        // NO declared `\usfm` and it fires anyway, unlike `deprecated-marker`:
        // `AttrStatus` carries no version, so there is no rung to gate on and
        // pretending otherwise would be inventing data.
        let (tokens, obs) = findings("\\p \\xt Gen 1:1|link-href=\"#x\"\\xt*\n");
        let list = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .unwrap() as u32;
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::DeprecatedAttribute,
                list,
                token_named(&tokens, "xt", 0),
            )]
        );

        // The attribute is DEFINED, so `attr-unknown-name` stays quiet — the
        // two rules are exclusive by construction, not by an `else`.
        assert!(!codes(&obs).contains(&Code::AttrUnknownName));
    }

    #[test]
    fn marker_out_of_band_judges_the_monotonic_positional_axis() {
        // The book written in order: each marker advances, none looks back.
        let (_, obs) = findings(
            "\\id GEN\n\\usfm 3.0\n\\h Genesis\n\\toc1 Genesis\n\\mt1 Genesis\n\\ip intro\n\\c 1\n\\p \\v 1 a\n",
        );
        assert_eq!(obs, vec![]);

        // Both of `mt`'s positional contexts are behind us, nothing above.
        let (tokens, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\mt1 late title\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerOutOfBand,
                token_named(&tokens, "mt", 0)
            )]
        );
        let (tokens, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\h Genesis\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerOutOfBand,
                token_named(&tokens, "h", 0)
            )]
        );
        let (tokens, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\toc1 Genesis\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerOutOfBand,
                token_named(&tokens, "toc", 0)
            )]
        );

        // `\ip`'s mask carries BookIntroduction AND ChapterContent, so the band
        // stays put. `\cl`'s two meanings fall out the same way.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\ip introduction\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\id GEN\n\\cl Chapter\n\\c 1\n\\cl Chapter One\n\\p a\n");
        assert_eq!(obs, vec![]);

        // Markers a CONTAINER licenses abstain — the other axis.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\add a\\add* \\f + \\ft n\\f*\n");
        assert_eq!(obs, vec![]);

        // Row 0 has no mask at all: en_ulb's `\s5` is 13_636 occurrences of it,
        // and `unknown-marker` is the whole of what lint says.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\s5\n\\p b\n");
        assert_eq!(codes(&obs), vec![Code::UnknownMarker]);
    }
}
