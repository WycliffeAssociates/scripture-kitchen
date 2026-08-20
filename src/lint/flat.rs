//! The FLAT machine: every rule whose evidence is a token row, its span, or
//! the one token before it — row lookup, form, payload, adjacency, attributes,
//! the version family, and the positional band.

use super::rows::version_row;
use super::walk::{is_structural_ws, span_of};
use super::{Code, Doc, Emit, NO_TOKEN, Observation, UsfmVersion};
use crate::attributes::{self, AttrEvent, AttrResolution, MalformedAttr};
use crate::tables::books;
use crate::tables::generated;
use crate::tables::schema::{
    AttrStatus, MarkerKind, Numbering, SpecContext, SpellingShape,
    StructuralWhitespaceRequirement as Ws,
};
use crate::{Token, TokenKind};

/// Which designator family may be re-numbered at this point.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Window {
    Closed,
    Chapter,
    Verse,
}

/// The FLAT machine: every rule whose evidence is a token row, its span, or the
/// one token before it — the row-lookup rules, the two Form rules, the caller
/// and numbering payload rules, adjacency, and the shape-only attribute
/// family. Fed one leaf at a time, exactly as the sketch draws it ("adjacency +
/// form + payload + attributes: single token walk").
///
/// It carries a few small pieces of lookbehind state, and they are its fields
/// rather than four separate sweeps' locals for a measured reason: a token
/// sweep over this corpus costs ~2.5 ns/token in dispatch alone, however little
/// each arm does, so a family that needs no more than the last marker earns no
/// traversal of its own.
///
/// - **The adjacency window.** `\ca`/`\cp` are legal immediately after `\c`'s
///   designator or after each other, `\va`/`\vp` likewise after `\v` — where
///   "immediately" means "with nothing but whitespace between", because the
///   scanner emits a Newline token at every line break and the spec's own
///   examples put `\cp` on its own line. These markers open no scope, so the
///   CST cannot see their misplacement; this is the only place the fact exists.
/// - **The attribute owner.** Attributes belong to the last marker, exactly as
///   [`TokenKind::AttrList`] documents, so the owner is the nearest preceding
///   opener and a Newline ends its reach — the scanner bounds lists to a line,
///   so nothing else would be honest.
/// - **The numbering-mix bitmasks**, closed out by the document simply ending.
/// - **The positional BAND** (one u8, the closeout window): where the document
///   has got to along `Scripture` → … → `ChapterContent`. Monotonic, so the
///   whole judge is a mask AND and a `trailing_zeros` — no data, no stack.
/// - **The sid ledger**, one token index per row, for the `eid`-required-if
///   rule: "was this milestone family ever opened with a `sid`".
pub(crate) struct Flat {
    /// The `\usfm` version the header declared, the one fact this machine takes
    /// from outside the walk.
    version: Option<UsfmVersion>,
    /// The four adjacency rows, resolved once instead of per token.
    ca: generated::MarkerIdx,
    cp: generated::MarkerIdx,
    va: generated::MarkerIdx,
    vp: generated::MarkerIdx,
    window: Window,
    /// The owning marker of any attribute list that arrives now: its token
    /// index, its row, whether its terminator is `\*` rather than a named
    /// closer, and whether its row defines any attributes at all. `NO_TOKEN` =
    /// no marker is in reach.
    owner: u32,
    owner_idx: generated::MarkerIdx,
    owner_is_point: bool,
    owner_has_attrs: bool,
    /// The first attribute list already seen on that owner.
    first_list: u32,
    /// The `-e` milestone point in reach owes an `eid`, and this is the earlier
    /// `sid`-carrying point that says so ([`NO_TOKEN`] = nothing owed). Settled
    /// at the point's `\*`, because "there was no list at all" is a verdict only
    /// the terminator can give.
    eid_owed: u32,
    eid_seen: bool,
    /// Per ROW: the token of the first point that carried a `sid`, or
    /// [`NO_TOKEN`]. Indexed by row exactly like `levels` below — the row IS
    /// the milestone family's key, and only four rows in the table define
    /// `sid`/`eid` at all.
    sid_at: [u32; generated::ROW_COUNT],
    /// How far along the POSITIONAL band the document has come, as a
    /// `SpecContext` discriminant (0 = `Scripture`, the state a book opens in).
    band: u8,
    /// Levels-seen per numbered family, indexed by ROW: bit 0 = the bare
    /// spelling, bit n = `\q<n>`, bit 15 = "already reported". A fixed array
    /// rather than a map because the row index IS the family key and there are
    /// only 153 rows — 306 bytes, no hashing, no allocation. `first_seen` is
    /// only ever read when the mask says the family has been seen, so it needs
    /// no sentinel initialization.
    levels: [u16; generated::ROW_COUNT],
    first_seen: [u32; generated::ROW_COUNT],
}

const REPORTED: u16 = 1 << 15;

/// The context mask's POSITIONAL half — the eight bits
/// [`SpecContext::is_positional`] names, and the only bits the band judge may
/// look at. Folded from the enum itself so a new positional variant arrives
/// here without an edit.
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
            owner: NO_TOKEN,
            owner_idx: generated::UNRESOLVED,
            owner_is_point: false,
            owner_has_attrs: false,
            first_list: NO_TOKEN,
            eid_owed: NO_TOKEN,
            eid_seen: false,
            sid_at: [NO_TOKEN; generated::ROW_COUNT],
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
        let (source, tokens) = (doc.source, doc.tokens);
        let (ca, cp, va, vp) = (self.ca, self.cp, self.va, self.vp);
        let version = self.version;
        match kind {
            // Byte-exact membership first, then the case-folded retry: "known
            // but lowercase" and "unknown" are different findings and exactly
            // one of them fires.
            TokenKind::BookCode => {
                let span = span_of(source, token);
                if books::is_book_code(span) {
                    return;
                }
                let folded = books::upper3(span).filter(|upper| books::is_book_code(upper));
                match folded {
                    // The one payload fix: the code IS a book identifier, so
                    // case-folding its three bytes is a splice and not a guess.
                    // (`book-code-unknown` gets none — which identifier the
                    // author meant is not a mechanical question.)
                    Some(upper) => out.push_fixed(
                        Observation::one(Code::BookCodeNotUppercase, idx),
                        token.start,
                        token.end(),
                        &upper,
                    ),
                    None => out.push(Observation::one(Code::BookCodeUnknown, idx)),
                }
            }
            // Openers and milestones share this arm because they are the same
            // thing to every rule below: the marker a list, a level or a
            // designator belongs to. `nested` binds the two kinds' one
            // spelling bit (`\+w`'s `+`, `\qt-e`'s `e`), and only the opener
            // half ever reads it.
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
                        // branch is the canonical spelling (its other branch,
                        // `/${Ws}\\/` with Ws zero-or-more, makes hugging
                        // grammatically legal — hence Hint severity), and the
                        // fix proposes the preferred form the rest of the
                        // corpus is written in.
                        out.push_fixed(
                            Observation::one(Code::MarkerNotWsPreceded, idx),
                            token.start,
                            token.start,
                            b"\n",
                        );
                    }
                    // "The span absorbed no delimiter" is ONE byte to check: a
                    // marker name never ends in whitespace, so a trailing
                    // space or tab can only be the folded delimiter run.
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

                    // --- the positional band ----------------------------
                    // Stay if this marker is legal where we are; else advance
                    // to the LOWEST of its positional contexts above us; else
                    // every one of them is behind us and that is the finding.
                    // Two instructions of state, and the mt#/cl/ip
                    // dual-context question answers itself.
                    //
                    // A mask with any CONTAINER bit in it abstains, and this is
                    // the lane's real boundary rather than a noise filter (it
                    // was measured as one: `\add`'s mask carries BookTitles and
                    // BookIntroduction, so without this every character marker
                    // inside a paragraph would be "behind us"). A marker a
                    // container licenses is judged by the WALKER's mask at pop
                    // time — that is displacement, the other axis — so the band
                    // speaks only for the markers whose sole license is where
                    // the book has got to: `\id`, `\h`, `\toc#`, `\mt#`, the
                    // introduction ladder, `\c`, and the paragraph rows.
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

                    // --- the Version family -----------------------------
                    // The row's own BOOL is the filter; the five-row table
                    // says since when. Reported at the OPENER only — a closer
                    // is the same occurrence, and the fix rewrites both halves
                    // from here.
                    if generated::deprecated(marker_idx) {
                        self.deprecated_marker(doc, idx, marker_idx, out);
                    }
                }

                // --- adjacency ------------------------------------------
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
                        // The self.window stays open either way: `\ca 2\ca*\cp א`
                        // is one legal run, and re-reporting every member of a
                        // misplaced run turns one slip into three findings.
                        opens
                    }
                    None => match generated::kind(marker_idx) {
                        MarkerKind::Chapter => Window::Chapter,
                        MarkerKind::Verse => Window::Verse,
                        _ => Window::Closed,
                    },
                };

                // --- numbering-mix --------------------------------------
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

                self.owner = idx;
                self.owner_idx = marker_idx;
                self.owner_is_point =
                    !opener || generated::kind(marker_idx) == MarkerKind::Milestone;
                // Resolved HERE, once per marker, rather than per Text token:
                // it is the flag that keeps the pipe scan off ordinary prose,
                // so it must not itself cost a table read per token.
                let defined = generated::defined_attributes(marker_idx);
                self.owner_has_attrs = !self.owner_is_point && !defined.is_empty();
                self.first_list = NO_TOKEN;
                // An `-e` point's list is where an `eid` would be, so the
                // question opens here and is settled at its `\*`.
                self.eid_seen = false;
                self.eid_owed = if !opener
                    && nested
                    && self.sid_at[marker_idx as usize] != NO_TOKEN
                    && defined.iter().any(|(name, _)| *name == "eid")
                {
                    self.sid_at[marker_idx as usize]
                } else {
                    NO_TOKEN
                };
            }
            // Not an enumeration of `+`/`-`/`?` — those are the conventional
            // values of one general run, and a caller of up to three bytes is
            // taken as deliberate. See the row for why the width is what it is.
            TokenKind::NoteCaller => {
                if span_of(source, token).len() > 3 {
                    out.push(Observation::one(Code::CallerShape, idx));
                }
                self.window = Window::Closed;
            }
            // Whether a closer closed anything is [`Structure`]'s question,
            // not this machine's: the answer is a property of the node that is
            // about to close, and it arrives one event later.
            TokenKind::ClosingMarker { nested } => {
                if nested
                    && token.marker_idx != generated::UNRESOLVED
                    && generated::kind(token.marker_idx) != MarkerKind::Character
                {
                    out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                }
                // A closer ends the self.owner's reach and leaves the adjacency
                // self.window alone: `\ca 2\ca*\cp א` is one legal run.
                self.owner = NO_TOKEN;
                self.owner_has_attrs = false;
                self.first_list = NO_TOKEN;
                self.eid_owed = NO_TOKEN;
            }
            // The `-e` point is COMPLETE here, and only here: an `eid` that
            // never arrived is a fact about the whole point, not about any one
            // list, so this is where the verdict is given. Anchored at the
            // list when there was one and at the milestone itself when there
            // was not — which is the shape the rule is really about
            // (`\qt-e\*` carrying nothing).
            TokenKind::MilestoneTerminator => {
                if self.eid_owed != NO_TOKEN && !self.eid_seen {
                    let anchor = if self.first_list == NO_TOKEN {
                        self.owner
                    } else {
                        self.first_list
                    };
                    out.push(Observation::pair(
                        Code::AttrRequiredIf,
                        anchor,
                        self.eid_owed,
                    ));
                }
                self.owner = NO_TOKEN;
                self.owner_has_attrs = false;
                self.first_list = NO_TOKEN;
                self.eid_owed = NO_TOKEN;
            }
            TokenKind::AttrList => {
                if self.first_list != NO_TOKEN {
                    out.push(Observation::pair(Code::AttrBothLists, idx, self.first_list));
                } else {
                    self.first_list = idx;
                }
                if self.owner == NO_TOKEN {
                    return;
                }
                // NODE-INITIAL is "in front position AND self-closed": the
                // list is the token right after its marker, and its span ends
                // with the closing pipe (plus any HS the U25001 production
                // puts inside the list). Everything else is the 3.1 trailing
                // form. The one shape this would read as node-initial and is
                // not is `\w a|b|\w*`, where a raw pipe ENDS a back-position
                // value — but that list is not in front position either, so
                // the front test already excludes it.
                let span = span_of(source, token);
                let trailing = idx != self.owner + 1
                    || !span
                        .iter()
                        .rev()
                        .find(|byte| !matches!(byte, b' ' | b'\t'))
                        .is_some_and(|byte| *byte == b'|');
                if trailing {
                    // The version half of this rule is the ROW's, not the
                    // rule's: `severity_at` is `None` below the ladder's first
                    // rung, which is exactly the old hand-coded `>= 3.2` gate
                    // with the fact moved to where the fact is authored.
                    if generated::kind(self.owner_idx) == MarkerKind::Character
                        && Code::AttrTrailingFormDeprecated
                            .row()
                            .severity_at(version)
                            .is_some()
                    {
                        out.push(Observation {
                            code: Code::AttrTrailingFormDeprecated,
                            anchor: idx,
                            second: self.owner,
                            aux: version.map_or(0, |declared| declared as u32),
                        });
                    }
                    // A trailing list stops AT its terminator, so the next
                    // token IS the closer the scanner accepted without
                    // checking whose it was. This is where that is checked.
                    let matched = match tokens.get(idx as usize + 1).map(Token::kind) {
                        Some(TokenKind::MilestoneTerminator) => self.owner_is_point,
                        Some(TokenKind::ClosingMarker { .. }) => {
                            !self.owner_is_point
                                && tokens[idx as usize + 1].marker_idx == self.owner_idx
                        }
                        _ => true,
                    };
                    if !matched {
                        out.push(Observation::pair(
                            Code::AttrTerminatorMismatch,
                            idx,
                            self.owner,
                        ));
                    }
                }
                // ROW 0 ABSTAINS, and it is both the honest line and the
                // measured one. Honest: a custom `\z` marker has no row to
                // judge its attributes against, and `unknown-marker` is the
                // one finding lint owes on a marker it cannot classify (the
                // same line `nested-spelling-misuse` draws). Measured: en_ult
                // writes 461,352 `\zaln-s` lists of five attributes each,
                // which is over half of every attribute byte in the corpus —
                // reading them to say nothing costs ~7 ns/token. The day
                // custom-marker configuration lands, `\zaln-s` gets a real row
                // and its lists are read like anyone's.
                if self.owner_idx != generated::UNRESOLVED {
                    self.read_attributes(doc, idx, token, out);
                }
            }
            // A raw pipe in the content of an attrs-capable marker. The self.owner
            // flag comes first on purpose: it is one already-loaded boolean,
            // and it keeps the byte scan off the ~85% of tokens that are
            // ordinary prose.
            //
            // MILESTONE owners abstain, and that is the same line the scanner
            // draws when it arms its back-position pipe needle for Character
            // and Figure rows alone. A milestone has no content, so a pipe
            // that stayed content there means its `\*` never came — which
            // `unterminated-milestone` already reports, exactly, and this hint
            // would only guess at.
            TokenKind::Text => {
                if self.owner_has_attrs && span_of(source, token).contains(&b'|') {
                    out.push(Observation::pair(Code::AttrPipeHint, idx, self.owner));
                }
                // GUARDED, and the guard is load-bearing: `\c 1 \ca` is the
                // only shape that cares whether a Text run is blank, so asking
                // the question unconditionally means reading every content byte
                // in the document for a fact that matters after roughly one
                // token in ten thousand.
                if self.window != Window::Closed
                    && !span_of(source, token).iter().all(|b| is_structural_ws(*b))
                {
                    self.window = Window::Closed;
                }
            }
            // A line ending ends the ATTRIBUTE machine's reach, and
            // deliberately leaves the adjacency self.window open: `\c 1` and its
            // `\cp` are conventionally written on separate lines.
            TokenKind::Newline => {
                self.owner = NO_TOKEN;
                self.owner_has_attrs = false;
                self.first_list = NO_TOKEN;
                // A point whose `\*` never came is `unterminated-milestone`,
                // exactly and already; saying it also owes an `eid` would be
                // two findings for one missing terminator.
                self.eid_owed = NO_TOKEN;
            }
            // The designator of the `\c`/`\v`/`\ca`/`\vp` that opened the
            // self.window belongs to the run; an optional break does not.
            TokenKind::Designator => {}
            TokenKind::OptBreak => self.window = Window::Closed,
        }
    }

    /// The k/v half of the Attributes family: one walk of the list's interior
    /// through [`attributes::attrs`], which is the ONLY place anything here
    /// reads inside a token.
    ///
    /// `#[inline(never)]`: it runs once per attribute LIST, never once per
    /// token, so it has no business inflating the dispatch tree above it.
    ///
    /// THE COST, measured and named because it is the closeout window's one
    /// real perf event: this is the first rule in lint to read INSIDE a list,
    /// and a word-aligned corpus is largely made of list interiors. en_ult
    /// writes 792,414 `\w` lists of `|x-occurrence="1" x-occurrences="1"` —
    /// 31 MB of bytes — and walking them costs +5.7 ns/token there (9.8 → 15.5,
    /// min-of-8) against +0.6 ns/token on the three unaligned corpora. It is
    /// not the interpreter being slow (~0.85 GB/s byte-at-a-time is what that
    /// walk is worth); it is 31 MB of work lint never did. The two honest
    /// reductions are already applied — row 0 abstains (see the call site,
    /// another 25 MB) and `x-`/`z-` names skip resolution — and what remains
    /// buys the rules below. A vectorized interior scan is the next lever if
    /// the number ever matters (planning/investigate-later.md).
    #[inline(never)]
    fn read_attributes(&mut self, doc: &Doc, idx: u32, token: &Token, out: &mut Emit) {
        let mut family = 0u32;
        for event in attributes::attrs(doc.source, token) {
            match event {
                AttrEvent::Attr(attr) => {
                    // The sid ledger, and the eid this list may be answering.
                    // Recorded off the NAME, not off resolution: it is the
                    // author's spelling that opens the family's obligation.
                    match attr.name {
                        b"sid" => self.sid_at[self.owner_idx as usize] = self.owner,
                        b"eid" => self.eid_seen = true,
                        _ => {}
                    }
                    // The corpus's whole attribute population, short-circuited:
                    // `resolve` would answer `UserNamespace` for every `x-`/`z-`
                    // name, and the pin in attributes.rs ("exact beats wildcard
                    // beats namespace, which matters for nothing today") says no
                    // defined name starts with either prefix — so this cannot
                    // change an answer, and it keeps 4.35M table scans out of
                    // en_ult's lint.
                    if attr.name.starts_with(b"x-") || attr.name.starts_with(b"z-") {
                        continue;
                    }
                    match attributes::resolve(attr.name, self.owner_idx) {
                        AttrResolution::Defined { status, .. } => {
                            family += 1;
                            // The Version family's other half, read off the
                            // resolution the row already handed back: `\xt`'s
                            // `link-href` and `\jmp`'s `link-` trio.
                            if status == AttrStatus::Deprecated {
                                out.push(Observation::pair(
                                    Code::DeprecatedAttribute,
                                    idx,
                                    self.owner,
                                ));
                            }
                        }
                        // Unreachable past the short-circuit above, and left
                        // exhaustive on purpose: the verdict belongs to
                        // `resolve`, and a fourth resolution would have to be
                        // answered here rather than defaulted.
                        AttrResolution::UserNamespace => {}
                        AttrResolution::Unknown => out.push(Observation {
                            code: Code::AttrUnknownName,
                            anchor: idx,
                            second: self.owner,
                            // The flag the row documents: an empty name is the
                            // BARE default form on a row with no
                            // `default_attribute`, which is a different
                            // authoring question with the same answer.
                            aux: u32::from(attr.name.is_empty()),
                        }),
                    }
                }
                // Always the last event, so this is one finding per list.
                AttrEvent::Malformed { why, .. } => out.push(Observation {
                    code: Code::AttrMalformed,
                    anchor: idx,
                    second: self.owner,
                    aux: malformed_slot(why),
                }),
            }
        }
        // `\ta`'s "one or more attributes, each beginning with `a-`": the row
        // carries the wildcard and no fixed names, so an empty family here is
        // the cardinality the row could not state. Asked once per LIST rather
        // than kept as a per-marker flag — `\ta` is the only row it is ever
        // true of, and a flag would put a table read on every token to learn
        // it once per list.
        let defined = generated::defined_attributes(self.owner_idx);
        let wildcard = !defined.is_empty() && defined.iter().all(|(name, _)| name.ends_with('*'));
        if wildcard && family == 0 {
            out.push(Observation::pair(Code::AttrRequiredIf, idx, self.owner));
        }
    }

    /// A deprecated marker, and the rename that repairs it where the spec's
    /// replacement is a rename.
    ///
    /// `#[inline(never)]` and behind `generated::deprecated`: five rows in 153
    /// reach it, so the cost on every other marker is one bitfield test.
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
        // and `\addpn` is correct in 2.x. The row's `severity: None` base says
        // the same thing to a consumer reading the ladder.
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

/// [`MalformedAttr`] as `aux`, in the enum's declaration order (the mapping the
/// `attr-malformed` row documents).
fn malformed_slot(why: MalformedAttr) -> u32 {
    match why {
        MalformedAttr::UnterminatedQuote => 0,
        MalformedAttr::EmptyName => 1,
        MalformedAttr::MissingValue => 2,
        MalformedAttr::BareJunk => 3,
    }
}

/// What may legally follow a marker name: structural whitespace, or one of
/// `TAGEND`'s two other alternatives (a marker, an attribute list).
fn is_delimiter_byte(byte: u8) -> bool {
    is_structural_ws(byte) || matches!(byte, b'\\' | b'|')
}

/// Does this row's `ws_after_name` REQUIRE something after the name? The
/// optional forms (milestones' `Hs`, row 0's `NotRequired`) can never be
/// missing one.
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
/// `\qt3-s` → 3. Read off the span because that is the only place it exists:
/// rows are canonical (`q`, not `q1`), so the token's own bytes are the sole
/// record of which spelling was used.
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
/// whose digit is a LEVEL? `TableColumns` rows (`\tc1`, `\tc1-2`) are excluded:
/// their digits are a column number, i.e. payload, so `\tc` beside `\tc2` is
/// not two spellings of one thing.
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

        // A real nested character pair is silent.
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

        // Known, uppercase, with a description after it: silent.
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

        // Not a code in any casing.
        let (tokens, obs) = findings("\\id ZZZ\n\\c 1\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(Code::BookCodeUnknown, book_code(&tokens))]
        );
        // `\id GENESIS` carves `GENESIS` as the code (one span up to the
        // first space), which is simply not an identifier.
        let (_, obs) = findings("\\id GENESIS\n\\c 1\n\\p \\v 1 a");
        assert_eq!(codes(&obs), vec![Code::BookCodeUnknown]);
    }

    // -----------------------------------------------------------------
    // Phase 3: adjacency
    // -----------------------------------------------------------------

    /// Compared WHOLE since 2026-08-19. These tests used to filter for the
    /// placement codes because a well-formed `\ca 2\ca*` drew a spurious
    /// `orphan-closer` — the rows demanded an explicit closer while opening no
    /// scope for it to close. Will's ruling made `ca`/`va`/`vp` scope openers
    /// (see the `ca` row in `tables::rows`), so the closers now close their own
    /// frames and the noise is gone. The adjacency rule itself is unchanged: it
    /// reads TOKENS, never the CST.
    #[test]
    fn ca_and_cp_must_follow_their_chapter() {
        // The spec's own shape: `\ca` on the `\c` line, `\cp` on the next one.
        // A Newline between them is a token, and the rule steps over it.
        let (_, obs) = findings("\\c 1 \\ca 2\\ca*\n\\cp \u{5d0}\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);

        // …and the pair the other way round is equally legal.
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

        // A whole run out of place is ONE finding, not one per member. (The
        // newline before `\cp` is only there to keep the snippet free of an
        // unrelated `marker-not-ws-preceded`, now that these tests compare the
        // finding list whole.)
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

    /// The other half of the ruling: an UNCLOSED `\ca` is now a real finding
    /// (`unclosed-char`, with the insert-`\ca*` fix) rather than silence.
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

    // -----------------------------------------------------------------
    // Phase 3: payload
    // -----------------------------------------------------------------

    #[test]
    fn caller_shape_only_reports_the_accidents() {
        // The three conventional values, and a short custom one, are silent.
        for caller in ["+", "-", "?", "*", "\",", "abc"] {
            let (_, obs) = findings(&format!("\\p a\\f {caller} \\ft n\\f*"));
            assert_eq!(obs, vec![], "caller {caller:?}");
        }

        // `\f +note` — the space after the caller was forgotten, so the whole
        // word lexed as the caller.
        let (tokens, obs) = findings("\\p a\\f +note \\ft n\\f*");
        let caller = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::NoteCaller)
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::CallerShape, caller)]);
    }

    /// `numbering-out-of-range` is NOT a code, and this is why: an over-cap
    /// level never reaches its family's row. `generated::marker_idx` validates
    /// the digits during resolution, so `\q7` IS row 0 and `unknown-marker`
    /// has already said everything there is to say about it. If this test ever
    /// fails, the rule became reachable and should be written.
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
        // …and the highest legal level resolves to the `q` row as it should.
        let (_, obs) = findings("\\c 1\n\\q4 poetry\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn numbering_mix_is_one_finding_per_family_per_book() {
        // One family, both spellings: anchored at the token that revealed it,
        // `second` at the family's first occurrence, `aux` the row's cap.
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

        // Numbered-only and bare-only are both consistent.
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

    // -----------------------------------------------------------------
    // Phase 3: form
    // -----------------------------------------------------------------

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

        // Character markers legitimately hug — the nested spelling included,
        // and aligned USFM is built out of exactly this.
        let (_, obs) = findings("\\p \\w grace\\+nd deep\\+nd*\\w*\\add x\\add*");
        assert_eq!(obs, vec![]);

        // Start of file is not a finding either (the `\id` prefix this helper
        // adds is itself the case).
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

        // A no-break space after the name is the same finding.
        let (tokens, obs) = findings("\\p a\n\\q\u{00A0}poetry\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DelimiterShape,
                token_named(&tokens, "q", 0)
            )]
        );

        // Everything `TAGEND` allows is silent: whitespace, a line ending, a
        // marker, an attribute list, end of file.
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

    // -----------------------------------------------------------------
    // Phase 3: attributes (shape only)
    // -----------------------------------------------------------------

    #[test]
    fn the_trailing_attribute_form_is_reported_only_against_a_declared_32() {
        // 3.0 declared, and no declaration at all: the trailing form is the
        // correct spelling and nothing is said.
        let (_, obs) = findings("\\id GEN\n\\usfm 3.0\n\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);

        // 3.2 declared: deprecated, and the row escalates it to an Error at 4.
        let usfm = "\\id GEN\n\\usfm 3.2\n\\p \\w grace|lemma=\"x\"\\w*\n";
        let (tokens, obs) = findings(usfm);
        let list = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .unwrap() as u32;
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrTrailingFormDeprecated,
                anchor: list,
                second: token_named(&tokens, "w", 0),
                aux: UsfmVersion::V3_2 as u32,
            }]
        );
        // The GATE and the escalation are one column now (the closeout
        // window): the row is silent below 3.2 and an Error at 4, and this
        // rule's `if` above asks it rather than restating the version.
        let row = Code::AttrTrailingFormDeprecated.row();
        assert_eq!(row.severity_at(Some(UsfmVersion::V3_0)), None);
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V4_0)),
            Some(Severity::Error)
        );

        // The node-initial form is what 3.2 wants, and says nothing.
        let (_, obs) = findings("\\id GEN\n\\usfm 3.2\n\\p \\w |lemma=\"x\"|grace\\w*\n");
        assert_eq!(obs, vec![]);

        // A MILESTONE's trailing list is its normal syntax, never deprecated —
        // this is the shape that would light up every alignment corpus.
        let (_, obs) =
            findings("\\id GEN\n\\usfm 3.2\n\\p a \\qt-s |who=\"Levi\"\\* b \\qt-e\\*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn two_attribute_lists_on_one_marker() {
        // The proposal's own "ridiculous but legal" case.
        let (tokens, obs) = findings("\\p \\w |Fred|J\u{e9}sus|Jesus\\w*\n");
        let lists: Vec<u32> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind() == TokenKind::AttrList)
            .map(|(idx, _)| idx as u32)
            .collect();
        assert_eq!(
            obs,
            vec![Observation::pair(Code::AttrBothLists, lists[1], lists[0])]
        );

        // One list per marker, twice over, is not two lists on one marker.
        // (`lemma` rather than a made-up name: `w` defines `lemma`, so the k/v
        // rules stay quiet and this test compares the finding list whole.)
        let (_, obs) = findings("\\p \\w a|lemma=\"v\"\\w* \\w b|lemma=\"v\"\\w*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn an_attribute_list_closed_by_the_wrong_marker() {
        // `\add*` ends `\w`'s list — the case scanner.rs names when it says
        // the terminator is checked only for BEING a closer.
        let (tokens, obs) = findings("\\p \\w grace|lemma=\"x\"\\add*\n");
        let list = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .unwrap() as u32;
        assert!(codes(&obs).contains(&Code::AttrTerminatorMismatch));
        assert_eq!(
            obs.iter()
                .find(|o| o.code == Code::AttrTerminatorMismatch)
                .copied(),
            Some(Observation::pair(
                Code::AttrTerminatorMismatch,
                list,
                token_named(&tokens, "w", 0)
            ))
        );

        // A milestone list closed by `\*`, and a character list closed by its
        // own `\X*`, are both matched.
        let (_, obs) = findings("\\p a \\qt-s |who=\"Levi\"\\* b \\qt-e\\*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn a_pipe_in_content_hints_only_inside_an_attrs_capable_marker() {
        // The list was refuted (no closer before the end of the line), so the
        // pipe survived as content — which is the whole reason for the hint.
        let (tokens, obs) = findings("\\p \\w gracious|lemma=\"grace\"\n\\p more\n");
        let hints: Vec<Observation> = obs
            .iter()
            .filter(|o| o.code == Code::AttrPipeHint)
            .copied()
            .collect();
        assert_eq!(hints.len(), 1, "{obs:?}");
        assert_eq!(hints[0].second, token_named(&tokens, "w", 0));

        // A pipe in ordinary prose is ordinary prose: `\p` defines no
        // attributes, so nothing is said.
        let (_, obs) = findings("\\p a | b\n");
        assert_eq!(obs, vec![]);

        // Neither does a marker that opens no attributes of its own — and this
        // snippet says something sharper since the k/v rules landed. The
        // scanner's back-position pipe needle IS armed for character rows, so
        // `| b` lexed as a trailing LIST rather than as content: there is no
        // pipe left in any Text token to hint about, and what the report says
        // instead is the truth about that list — `\add` has no default
        // attribute for a bare value to bind to (aux = 1).
        let (tokens, obs) = findings("\\p \\add a | b\\add*\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrUnknownName,
                anchor: tokens
                    .iter()
                    .position(|t| t.kind() == TokenKind::AttrList)
                    .unwrap() as u32,
                second: token_named(&tokens, "add", 0),
                aux: 1,
            }]
        );
    }

    // -----------------------------------------------------------------
    // Closeout: the k/v attribute rules
    // -----------------------------------------------------------------

    /// One helper for the whole family: the first list token, which is every
    /// one of these codes' anchor (whole-token anchoring, ruled 2026-08-20).
    fn first_list(tokens: &[Token]) -> u32 {
        tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .expect("no attribute list lexed") as u32
    }

    #[test]
    fn attr_unknown_name_reads_the_row_and_lets_the_user_namespace_through() {
        // A name `w` does not define. Anchored at the LIST, `second` at its
        // owner, aux 0 (a named attribute, not the bare form).
        let (tokens, obs) = findings("\\p \\w grace|nope=\"x\"\\w*\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrUnknownName,
                anchor: first_list(&tokens),
                second: token_named(&tokens, "w", 0),
                aux: 0,
            }]
        );

        // Everything the row DOES define is silent — exact, the `a-*` wildcard,
        // and the bare value bound through `default_attribute`.
        for usfm in [
            "\\p \\w grace|lemma=\"x\" strong=\"G1\"\\w*\n",
            "\\p \\ta text|a-alt=\"x\"\\ta*\n",
            "\\p \\w In|in\\w*\n",
        ] {
            let (_, obs) = findings(usfm);
            assert_eq!(obs, vec![], "{usfm:?}");
        }

        // The `x-`/`z-` namespace is legal wherever attributes are, on a
        // character marker and on a milestone alike — en_ult's 4.35M aligned
        // attributes are all of this shape, so a finding here is a million
        // findings there.
        let (_, obs) = findings("\\p \\w grace|x-strong=\"G1\" z-mine=\"y\"\\w*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\qt-s |x-who=\"Levi\"\\* a \\qt-e\\*\n");
        assert_eq!(obs, vec![]);

        // aux = 1 is the OTHER shape: the bare default form on a row that has
        // no default attribute at all. `\fig` is the spec's own case. (Written
        // at chapter level: `fig`'s mask carries no Para bit, so inside a `\p`
        // it displaces the paragraph and this test would also see an
        // `empty-paragraph`.)
        let (tokens, obs) = findings("\\id GEN\n\\c 1\n\\fig |a.png\\fig*\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrUnknownName,
                anchor: first_list(&tokens),
                second: token_named(&tokens, "fig", 0),
                aux: 1,
            }]
        );

        // Row 0 says nothing: `unknown-marker` has already said the one true
        // thing about `\zfoo`, and its row defines nothing, so every attribute
        // on it would otherwise be a finding.
        // (The snippet draws other findings — a custom `\z` marker pops every
        // open scope and its `\*` terminates nothing — so this asserts the one
        // thing it is about.)
        let (_, obs) = findings("\\p \\zfoo |k=\"v\"\\*\n");
        assert!(
            !codes(&obs).contains(&Code::AttrUnknownName),
            "row 0 spoke about its attributes: {obs:?}"
        );
    }

    #[test]
    fn attr_malformed_carries_the_shape_in_aux() {
        // One finding per list, whatever follows the blamed byte: `Malformed`
        // ends the interpreter's walk.
        let (tokens, obs) = findings("\\p \\w x|lemma=\"a\", strong=\"G1\"\\w*\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrMalformed,
                anchor: first_list(&tokens),
                second: token_named(&tokens, "w", 0),
                // BareJunk: a comma is not a separator (ruled — usfmtc drops
                // the tail silently, we report it).
                aux: 3,
            }]
        );

        // The four shapes, in `MalformedAttr` declaration order.
        for (usfm, aux) in [
            ("\\p \\w x|lemma=\"grace\\w*\n", 0),
            ("\\p \\w x|=\"v\"\\w*\n", 1),
            ("\\p \\w x|lemma=\\w*\n", 2),
            ("\\p \\w x|lemma=\"a\" oops\\w*\n", 3),
        ] {
            let (_, obs) = findings(usfm);
            let malformed: Vec<&Observation> = obs
                .iter()
                .filter(|o| o.code == Code::AttrMalformed)
                .collect();
            assert_eq!(malformed.len(), 1, "{usfm:?}: {obs:?}");
            assert_eq!(malformed[0].aux, aux, "{usfm:?}");
        }
    }

    #[test]
    fn attr_required_if_covers_the_eid_and_the_ta_family() {
        // A `\qt-e` point whose family was opened with `sid` and which carries
        // no `eid`: anchored at the milestone itself (there is no list),
        // `second` at the `sid`-carrying point above it.
        let (tokens, obs) = findings("\\p \\qt-s |sid=\"q1\"\\* a \\qt-e\\*\n");
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::AttrRequiredIf,
                token_named(&tokens, "qt", 1),
                token_named(&tokens, "qt", 0),
            )]
        );

        // The pair written properly is silent…
        let (_, obs) = findings("\\p \\qt-s |sid=\"q1\"\\* a \\qt-e |eid=\"q1\"\\*\n");
        assert_eq!(obs, vec![]);
        // …and so is a `-e` point in a book that never used `sid` at all: the
        // obligation comes from the author's own earlier spelling, not from the
        // row (which says Optional, and is right to).
        let (_, obs) = findings("\\p \\qt-s |who=\"Levi\"\\* a \\qt-e\\*\n");
        assert_eq!(obs, vec![]);

        // `\ta`'s family cardinality: "one or more attributes, each beginning
        // with `a-`" (char/features/ta.html). A list with none of it is the
        // finding; one member is enough to satisfy it.
        let (tokens, obs) = findings("\\p \\ta text|x-mine=\"y\"\\ta*\n");
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::AttrRequiredIf,
                first_list(&tokens),
                token_named(&tokens, "ta", 0),
            )]
        );
        let (_, obs) = findings("\\p \\ta text|a-alt=\"y\"\\ta*\n");
        assert_eq!(obs, vec![]);
    }

    // -----------------------------------------------------------------
    // Closeout: the Version family
    // -----------------------------------------------------------------

    #[test]
    fn a_deprecated_marker_is_gated_on_a_declared_version() {
        // No `\usfm` line: a book of its own era, and `\pro` is correct in it.
        // THE GATE — and the reason it is not a guess.
        let (_, obs) = findings("\\p \\pro x\\pro*\n");
        assert_eq!(obs, vec![]);

        // 3.0 declared: deprecated, with the deprecating version in aux.
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

        // The row's ladder is what a consumer maps severity through.
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

        // A numbered spelling of a deprecated paragraph row is one finding at
        // its own token, digits and all.
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
        // NO declared `\usfm`, and it fires anyway — deliberately, and unlike
        // `deprecated-marker`: `AttrStatus` carries no version, so there is no
        // rung to gate on and pretending otherwise would be inventing data.
        let (tokens, obs) = findings("\\p \\xt Gen 1:1|link-href=\"#x\"\\xt*\n");
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::DeprecatedAttribute,
                first_list(&tokens),
                token_named(&tokens, "xt", 0),
            )]
        );

        // The attribute is DEFINED, so `attr-unknown-name` stays quiet — the
        // two rules are exclusive by construction, not by an `else`.
        assert!(!codes(&obs).contains(&Code::AttrUnknownName));
    }

    // -----------------------------------------------------------------
    // Closeout: the positional band
    // -----------------------------------------------------------------

    #[test]
    fn marker_out_of_band_judges_the_monotonic_positional_axis() {
        // The book written in order: identification, headers, titles,
        // introduction, chapter content. Every one of these advances the band
        // and none of them looks back.
        let (_, obs) = findings(
            "\\id GEN\n\\usfm 3.0\n\\h Genesis\n\\toc1 Genesis\n\\mt1 Genesis\n\\ip intro\n\\c 1\n\\p \\v 1 a\n",
        );
        assert_eq!(obs, vec![]);

        // A major title AFTER the book reached chapter content: every one of
        // `mt`'s positional contexts (BookTitles, BookIntroductionEndTitles) is
        // behind us, and there is nothing above to advance to.
        let (tokens, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\mt1 late title\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerOutOfBand,
                token_named(&tokens, "mt", 0)
            )]
        );
        // The same for a header and a table-of-contents line.
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

        // `\ip` after a chapter is NOT out of band, and this is the dual-context
        // resolution the sketch asked to be tested: `ip`'s mask carries
        // BookIntroduction AND ChapterContent, so the band stays where it is
        // and the marker is legal. `\cl`'s two meanings fall out the same way.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\ip introduction\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\id GEN\n\\cl Chapter\n\\c 1\n\\cl Chapter One\n\\p a\n");
        assert_eq!(obs, vec![]);

        // Markers a CONTAINER licenses abstain — the other axis. A character
        // marker's mask carries BookTitles and BookIntroduction, so without
        // that abstention every `\add` inside a paragraph would read as
        // "behind us"; notes and table cells are the same case.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\add a\\add* \\f + \\ft n\\f*\n");
        assert_eq!(obs, vec![]);

        // Row 0 has no mask at all: en_ulb's `\s5` is 13_636 occurrences of
        // exactly this, and `unknown-marker` is the whole of what lint says.
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p a\n\\s5\n\\p b\n");
        assert_eq!(codes(&obs), vec![Code::UnknownMarker]);
    }
}
