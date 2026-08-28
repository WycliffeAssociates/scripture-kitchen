use super::walk::span_of;
use super::{Code, Doc, Emit, NO_TOKEN, Observation, UsfmVersion};
use crate::attributes::{self, AttrEvent, AttrResolution, MalformedAttr};
use crate::tables::generated;
use crate::tables::schema::{AttrStatus, MarkerKind};
use crate::{Token, TokenKind};

/// - **The owner** is the nearest preceding opener, as [`TokenKind::AttrList`]
///   documents, and a Newline ends its reach — the scanner bounds lists to a
///   line, so nothing wider would be honest.
/// - **The sid ledger**, one token index per row: "was this milestone family
///   ever opened with a `sid`" (the `eid`-required-if rule).
pub(crate) struct AttrRules {
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
    /// [`NO_TOKEN`]. Indexed by row exactly like [`Flat`](super::flat::Flat)'s
    /// `levels` — the row IS the milestone family's key, and only four rows in
    /// the table define `sid`/`eid` at all.
    sid_at: [u32; generated::ROW_COUNT],
    /// The `-e` point in reach is eid-defining but its family has NO in-chunk
    /// sid — reduce's question, not this chunk's: if the point's `\*` finds
    /// no `eid`, it is RECORDED as a candidate and judged against earlier
    /// chunks' sids (carried.rs `sid_eid`).
    eid_candidate: bool,
    /// The recorded candidates: (row, the anchor the emission would use).
    candidates: Vec<(u8, u32)>,
}

impl AttrRules {
    pub(super) fn new() -> Self {
        Self {
            owner: NO_TOKEN,
            owner_idx: generated::UNRESOLVED,
            owner_is_point: false,
            owner_has_attrs: false,
            first_list: NO_TOKEN,
            eid_owed: NO_TOKEN,
            eid_seen: false,
            sid_at: [NO_TOKEN; generated::ROW_COUNT],
            eid_candidate: false,
            candidates: Vec::new(),
        }
    }

    /// A marker or milestone point: it becomes the owner any list arriving now
    /// belongs to, and the question its `\*` will have to answer.
    #[inline]
    pub(super) fn on_marker(
        &mut self,
        idx: u32,
        marker_idx: generated::MarkerIdx,
        opener: bool,
        nested: bool,
    ) {
        self.owner = idx;
        self.owner_idx = marker_idx;
        self.owner_is_point = !opener || generated::kind(marker_idx) == MarkerKind::Milestone;
        // Resolved once per marker, not per Text token: this flag
        // keeps the pipe scan off prose, so it must not itself cost a
        // table read per token.
        let defined = generated::defined_attributes(marker_idx);
        self.owner_has_attrs = !self.owner_is_point && !defined.is_empty();
        self.first_list = NO_TOKEN;
        // An `-e` point's list is where an `eid` would be, so the
        // question opens here and is settled at its `\*`.
        self.eid_seen = false;
        let owes_eid = !opener && nested && defined.iter().any(|(name, _)| *name == "eid");
        self.eid_owed = if owes_eid && self.sid_at[marker_idx as usize] != NO_TOKEN {
            self.sid_at[marker_idx as usize]
        } else {
            NO_TOKEN
        };
        // No in-chunk sid: whether an EARLIER chunk opened the family is
        // reduce's question — remember the point as a candidate instead.
        self.eid_candidate = owes_eid && self.sid_at[marker_idx as usize] == NO_TOKEN;
    }

    /// A closer, a `\*` or a line ending: the owner is out of reach.
    ///
    /// A closer ends the owner's reach and leaves the adjacency window alone
    /// (`\ca 2\ca*\cp א` is one legal run) — which is why that window is
    /// [`Flat`](super::flat::Flat)'s field and this cluster is not.
    #[inline]
    pub(super) fn close_reach(&mut self) {
        self.owner = NO_TOKEN;
        self.owner_has_attrs = false;
        self.first_list = NO_TOKEN;
        // A point whose `\*` never came is already `unterminated-milestone`;
        // adding "and it owes an `eid`" is two findings for one mistake.
        self.eid_owed = NO_TOKEN;
        self.eid_candidate = false;
    }

    /// The `-e` point is COMPLETE here, and only here: a missing `eid` is a fact
    /// about the whole point, not about any one list. Anchored at the list when
    /// there was one, at the milestone itself when there was not (`\qt-e\*`
    /// carrying nothing, which is the shape the rule is about).
    #[inline]
    pub(super) fn on_milestone_terminator(&mut self, out: &mut Emit) {
        if !self.eid_seen && (self.eid_owed != NO_TOKEN || self.eid_candidate) {
            let anchor = if self.first_list == NO_TOKEN {
                self.owner
            } else {
                self.first_list
            };
            if self.eid_owed != NO_TOKEN {
                out.push(Observation::pair(
                    Code::AttrRequiredIf,
                    anchor,
                    self.eid_owed,
                ));
            } else {
                self.candidates.push((self.owner_idx, anchor));
            }
        }
        self.close_reach();
    }

    /// End of the chunk: sweep the fold observations into the summary — the
    /// LAST sid per row (matching `sid_at`'s most-recent-wins overwrite) and
    /// the `-e` points whose obligation only earlier chunks can settle.
    pub(super) fn record(&mut self, carried: &mut super::carried::Carried) {
        for (row, &sid) in self.sid_at.iter().enumerate() {
            if sid != NO_TOKEN {
                carried.sid_last.push((row as u8, sid));
            }
        }
        carried.eid_candidates = core::mem::take(&mut self.candidates);
    }

    /// A raw pipe in the content of an attrs-capable marker. The owner flag is
    /// tested first: one already-loaded boolean keeps the byte scan off the
    /// ~85% of tokens that are ordinary prose.
    ///
    /// MILESTONE owners abstain — the same line the scanner draws when it arms
    /// its back-position pipe needle for Character and Figure rows alone. A
    /// milestone has no content, so a pipe left there means its `\*` never came,
    /// which `unterminated-milestone` already reports exactly.
    #[inline]
    pub(super) fn on_text(&mut self, doc: &Doc, idx: u32, token: &Token, out: &mut Emit) {
        if self.owner_has_attrs && span_of(doc.source, token).contains(&b'|') {
            out.push(Observation::pair(Code::AttrPipeHint, idx, self.owner));
        }
    }

    /// The list itself: the shape rules (which form it is written in, whether
    /// it is the owner's second, whose closer ended it) and then, for a row
    /// that defines attributes at all, its interior.
    pub(super) fn on_attr_list(
        &mut self,
        doc: &Doc,
        idx: u32,
        token: &Token,
        version: Option<UsfmVersion>,
        out: &mut Emit,
    ) {
        let (source, tokens) = (doc.source, doc.tokens);
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
        // puts inside it). Everything else is the 3.1 trailing form.
        // `\w a|b|\w*` ends a back-position value with a raw pipe, but
        // it fails the front test, so it is not mistaken for one.
        let span = span_of(source, token);
        let trailing = idx != self.owner + 1
            || !span
                .iter()
                .rev()
                .find(|byte| !matches!(byte, b' ' | b'\t'))
                .is_some_and(|byte| *byte == b'|');
        if trailing {
            // The version half of this rule belongs to the ROW:
            // `severity_at` is `None` below the ladder's first rung,
            // so the gate is asked, never restated here.
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
                    !self.owner_is_point && tokens[idx as usize + 1].marker_idx == self.owner_idx
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
        // ROW 0 ABSTAINS, honestly and cheaply. Honest: a custom `\z`
        // marker has no row to judge its attributes against, and
        // `unknown-marker` is the one finding lint owes on a marker it
        // cannot classify. Cheap: en_ult writes 461,352 `\zaln-s` lists
        // of five attributes, over half of every attribute byte in the
        // corpus, and reading them to say nothing costs ~7 ns/token.
        // Configured `\z` markers get real rows and get read.
        if self.owner_idx != generated::UNRESOLVED {
            self.read_attributes(doc, idx, token, out);
        }
    }

    /// The k/v half of the Attributes family: one walk of the list's interior
    /// through [`attributes::attrs`], which is the ONLY place anything here
    /// reads inside a token.
    ///
    /// `#[inline(never)]`: it runs once per attribute LIST, never once per
    /// token, so it has no business inflating the dispatch tree above it.
    ///
    /// THE COST: a word-aligned corpus is largely made of list interiors. en_ult
    /// writes 792,414 `\w` lists of `|x-occurrence="1" x-occurrences="1"` — 31 MB
    /// of bytes — and walking them costs +5.7 ns/token there (9.8 → 15.5,
    /// min-of-8) against +0.6 ns/token on the three unaligned corpora. That is
    /// not the interpreter being slow (~0.85 GB/s byte-at-a-time is what the walk
    /// is worth); it is 31 MB of bytes. Both honest reductions are applied — row
    /// 0 abstains (see the call site, another 25 MB) and `x-`/`z-` names skip
    /// resolution — and what remains buys the rules below.
    #[inline(never)]
    fn read_attributes(&mut self, doc: &Doc, idx: u32, token: &Token, out: &mut Emit) {
        let mut family = 0u32;
        for event in attributes::attrs(doc.source, token) {
            match event {
                AttrEvent::Attr(attr) => {
                    // Recorded off the NAME, not off resolution: it is the
                    // author's spelling that opens the family's obligation.
                    match attr.name {
                        b"sid" => self.sid_at[self.owner_idx as usize] = self.owner,
                        b"eid" => self.eid_seen = true,
                        _ => {}
                    }
                    // The corpus's whole attribute population, short-circuited:
                    // `resolve` answers `UserNamespace` for every `x-`/`z-` name
                    // and no defined name carries either prefix, so this cannot
                    // change an answer — and it keeps 4.35M table scans out of
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
                        // Unreachable past the short-circuit above; left
                        // exhaustive so a fourth resolution must be answered
                        // here rather than defaulted.
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
        // carries the wildcard and no fixed names, so an empty family is the
        // cardinality the row could not state. Asked once per LIST, not kept as
        // a per-marker flag — `\ta` is the only row it is ever true of.
        let defined = generated::defined_attributes(self.owner_idx);
        let wildcard = !defined.is_empty() && defined.iter().all(|(name, _)| name.ends_with('*'));
        if wildcard && family == 0 {
            out.push(Observation::pair(Code::AttrRequiredIf, idx, self.owner));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::Severity;
    use crate::lint::tests::{codes, findings, token_named};

    /// The first list token — every one of these codes anchors on the whole
    /// list token.
    fn first_list(tokens: &[Token]) -> u32 {
        tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .expect("no attribute list lexed") as u32
    }

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
        // Gate and escalation are one column: the row is silent below 3.2 and
        // an Error at 4, and the rule asks it rather than restating a version.
        let row = Code::AttrTrailingFormDeprecated.row();
        assert_eq!(row.severity_at(Some(UsfmVersion::V3_0)), None);
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V4_0)),
            Some(Severity::Error)
        );

        // The node-initial form is what 3.2 wants, and says nothing.
        let (_, obs) = findings("\\id GEN\n\\usfm 3.2\n\\p \\w |lemma=\"x\"|grace\\w*\n");
        assert_eq!(obs, vec![]);

        // A MILESTONE's trailing list is its normal syntax, never deprecated.
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
        // (`lemma` is a name `w` defines, so the k/v rules stay quiet and the
        // finding list can be compared whole.)
        let (_, obs) = findings("\\p \\w a|lemma=\"v\"\\w* \\w b|lemma=\"v\"\\w*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn an_attribute_list_closed_by_the_wrong_marker() {
        // `\add*` ends `\w`'s list: the scanner accepted it for BEING a closer.
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
        // The list was refuted (no closer before the line ended), so the pipe
        // survived as content — the whole reason for the hint.
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

        // Neither does a marker that opens no attributes of its own. The
        // scanner's back-position pipe needle IS armed for character rows, so
        // `| b` lexes as a trailing LIST rather than as content: no pipe is
        // left in any Text token to hint about, and the report says the truth
        // about that list — `\add` has no default attribute for a bare value
        // to bind to (aux = 1).
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

    #[test]
    fn attr_unknown_name_reads_the_row_and_lets_the_user_namespace_through() {
        // A name `w` does not define: anchored at the LIST, `second` at its
        // owner, aux 0 (named, not the bare form).
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

        // The `x-`/`z-` namespace is legal wherever attributes are, character
        // marker and milestone alike — en_ult's 4.35M aligned attributes are
        // all of this shape, so a finding here is a million findings there.
        let (_, obs) = findings("\\p \\w grace|x-strong=\"G1\" z-mine=\"y\"\\w*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\qt-s |x-who=\"Levi\"\\* a \\qt-e\\*\n");
        assert_eq!(obs, vec![]);

        // aux = 1 is the OTHER shape: the bare default form on a row with no
        // default attribute, `\fig` being the spec's own case. Written at
        // chapter level because `fig`'s mask carries no Para bit, so inside a
        // `\p` it displaces the paragraph and adds an `empty-paragraph`.
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

        // Row 0 says nothing about its attributes: `unknown-marker` has said
        // the one true thing about `\zfoo`. The snippet draws other findings —
        // a custom `\z` pops every scope and its `\*` terminates nothing — so
        // this asserts only the code it is about.
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
                // BareJunk: a comma is not a separator — usfmtc drops the
                // tail silently, we report it.
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
        // A `\qt-e` whose family was opened with `sid` and carries no `eid`:
        // anchored at the milestone (no list), `second` at the `sid` point.
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
        // …and so is a `-e` point in a book that never used `sid`: the
        // obligation comes from the author's earlier spelling, not from the row
        // (which says Optional, and is right to).
        let (_, obs) = findings("\\p \\qt-s |who=\"Levi\"\\* a \\qt-e\\*\n");
        assert_eq!(obs, vec![]);

        // `\ta`'s family cardinality (char/features/ta.html): a list with no
        // `a-` attribute is the finding, one member satisfies it.
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
}
