//! A registered `\z` marker behaves as its category — end to end.
//!
//! ```text
//! zfoot = footnote
//!
//! \p \v 1 Jesus\zfoot + \zfchar why\zfoot* wept.
//!         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a Note node, caller and all
//!         verseText("Jesus wept.")        the note subtree drops, as `\f`'s does
//!         usj style "zfoot"               the SPELLING, never the template's name
//! ```
//!
//! Instrument: SHAPES. One document per claim, registered through `lex_with`
//! so nothing here touches the process registry and the file can run in any
//! order beside anything else.
//!
//! The table tests prove a template COPIES its source row; these prove the
//! engine reads the row rather than the name — the walker, lint, the mask, the
//! exports and the diff, none of which knows what an extension is.

use mise::extensions::{CustomMarker, ExtensionCategory};

use usfm_onion::cst::{self, Cst};
use usfm_onion::extensions::{ExtensionOptions, Extensions};
use usfm_onion::lint::Code;
use usfm_onion::tables::generated;
use usfm_onion::tables::schema::{MarkerKind, SpecContext};
use usfm_onion::{Filter, Token, TokenKind, lex, lex_with, mask};

fn registry(pairs: &[(&str, ExtensionCategory)]) -> Extensions {
    let list: Vec<CustomMarker> = pairs
        .iter()
        .map(|(name, category)| CustomMarker {
            name: (*name).to_owned(),
            category: *category,
            description: String::new(),
            attributes: Vec::new(),
        })
        .collect();
    let (extensions, reports) = Extensions::new(&list);
    assert!(reports.is_empty(), "{reports:?}");
    extensions
}

struct Doc {
    tokens: Vec<Token>,
    cst: Cst,
}

fn parse(source: &str, ext: &Extensions) -> Doc {
    let tokens = lex_with(source, ext);
    let cst = cst::build(&tokens);
    Doc { tokens, cst }
}

impl Doc {
    /// The node opened by the first token spelling `name`.
    fn node_of(&self, source: &str, name: &str) -> u32 {
        let want = format!("\\{name}");
        let token =
            self.tokens
                .iter()
                .position(|token| {
                    matches!(
                        token.kind(),
                        TokenKind::Marker { .. } | TokenKind::Milestone { .. }
                    ) && source[token.start as usize..token.end() as usize].trim_end() == want
                })
                .unwrap_or_else(|| panic!("no `{want}` opener in {source:?}")) as u32;
        self.cst
            .nodes
            .iter()
            .position(|node| node.token == token)
            .unwrap_or_else(|| panic!("`{want}` opened no node")) as u32
    }

    fn extent(&self, node: u32) -> std::ops::Range<u32> {
        self.cst.extent(node, &self.tokens)
    }
}

// ---------------------------------------------------------------- the walker

/// A registered footnote opens a Note scope INSIDE the paragraph, closes at
/// its own `\zmyf*`, and leaves the paragraph open — `\f`'s behaviour, from
/// `\f`'s row.
#[test]
fn a_footnote_extension_nests_and_closes_like_a_footnote() {
    let ext = registry(&[("zmyf", ExtensionCategory::Footnote)]);
    let source = "\\id GEN\n\\c 1\n\\p \\v 1 Jesus\\zmyf + \\ft why\\zmyf* wept.\n";
    let doc = parse(source, &ext);

    let note = doc.node_of(source, "zmyf");
    let opener = doc.cst.nodes[note as usize].token;
    assert_eq!(
        generated::kind(doc.tokens[opener as usize].marker_idx),
        MarkerKind::Note
    );
    // The caller is a payload token, as `\f`'s `+` is.
    assert_eq!(
        doc.tokens[opener as usize + 1].kind(),
        TokenKind::NoteCaller,
        "the caller rides the marker"
    );
    // The extent stops at the note's own closer, not at the line's end.
    let extent = doc.extent(note);
    assert_eq!(
        &source[extent.start as usize..extent.end as usize],
        "\\zmyf + \\ft why\\zmyf*"
    );

    // The paragraph is still open past the note.
    let para = doc.node_of(source, "p");
    let para_extent = doc.extent(para);
    assert!(
        para_extent.end as usize >= source.trim_end().len(),
        "the note must not close the paragraph: {para_extent:?}"
    );
    assert!(para_extent.start < extent.start && extent.end <= para_extent.end);
}

/// A `char` extension pairs with its own closer, and a `footnotechar` one sits
/// as a SIBLING of its peers inside a note — `\ft`'s closing rule, which ends
/// at the note rather than at an explicit closer.
#[test]
fn character_extensions_pair_and_note_peers_stay_siblings() {
    let ext = registry(&[
        ("zmyc", ExtensionCategory::Char),
        ("zmyft", ExtensionCategory::FootnoteChar),
    ]);
    let source =
        "\\id GEN\n\\c 1\n\\p \\v 1 a \\zmyc word\\zmyc* and\\f + \\zmyft one \\zmyft two\\f*\n";
    let doc = parse(source, &ext);

    let character = doc.node_of(source, "zmyc");
    let extent = doc.extent(character);
    assert_eq!(
        &source[extent.start as usize..extent.end as usize],
        "\\zmyc word\\zmyc*"
    );

    // The two `zmyft` peers are siblings, not nested: `\ft`'s own shape.
    let firsts: Vec<u32> = doc
        .cst
        .nodes
        .iter()
        .enumerate()
        .filter(|(id, node)| {
            *id != 0
                && generated::template_for(ExtensionCategory::FootnoteChar)
                    == doc.tokens[node.token as usize].marker_idx
        })
        .map(|(id, _)| id as u32)
        .collect();
    assert_eq!(firsts.len(), 2, "two peers");
    let (a, b) = (doc.extent(firsts[0]), doc.extent(firsts[1]));
    assert!(
        a.end <= b.start,
        "peers are siblings, not nested: {a:?} {b:?}"
    );
}

/// `standalone` is bare — a leaf that opens nothing and leaves its paragraph
/// open — and `milestone` pairs `-s`/`-e`, taking attributes as `\qt-s` does.
#[test]
fn standalone_is_bare_and_milestone_pairs() {
    let ext = registry(&[
        ("zms", ExtensionCategory::Standalone),
        ("zaln", ExtensionCategory::Milestone),
    ]);
    let source =
        "\\id GEN\n\\c 1\n\\p \\v 1 a \\zms b \\zaln-s |x-strong=\"H1\"\\*word\\zaln-e\\* c\n";
    let doc = parse(source, &ext);

    let template = generated::template_for(ExtensionCategory::Standalone);
    let bare = doc
        .tokens
        .iter()
        .position(|token| token.marker_idx == template)
        .expect("the standalone resolved") as u32;
    assert!(
        doc.cst.nodes.iter().all(|node| node.token != bare),
        "a standalone opens no node"
    );
    let para = doc.node_of(source, "p");
    assert!(
        doc.extent(para).end as usize >= source.trim_end().len(),
        "the paragraph runs past it"
    );

    let milestone = doc.node_of(source, "zaln-s");
    let extent = doc.extent(milestone);
    assert_eq!(
        &source[extent.start as usize..extent.end as usize],
        "\\zaln-s |x-strong=\"H1\"\\*",
        "the attribute list rides the milestone"
    );
    // Both halves of the pair resolve to the one template row.
    let template = generated::template_for(ExtensionCategory::Milestone);
    let sides = doc
        .tokens
        .iter()
        .filter(|token| token.marker_idx == template)
        .count();
    assert_eq!(sides, 2, "`-s` and `-e` share the row");
}

/// en_ulb's `\s5`, registered as a legacy standalone: the paragraph it sits
/// in stays one paragraph, nothing lints, and every export keeps `s5`.
#[test]
fn a_legacy_chunk_marker_passes_through() {
    let list = [CustomMarker {
        name: "s5".to_owned(),
        category: ExtensionCategory::Standalone,
        description: String::new(),
        attributes: Vec::new(),
    }];
    let (ext, reports) = Extensions::new_with(
        &list,
        &ExtensionOptions {
            relax_z_prefix: true,
        },
    );
    assert!(reports.is_empty(), "{reports:?}");
    let source =
        "\\id GEN\n\\usfm 3.0\n\\c 1\n\\s5\n\\p\n\\v 1 a\n\\s5\n\\v 2 b\n\\q1 c\n\\s5\n\\v 3 d\n";

    let para = parse(source, &ext).node_of(source, "p");
    let doc = parse(source, &ext);
    let extent = doc.extent(para);
    assert_eq!(
        &source[extent.start as usize..extent.end as usize],
        "\\p\n\\v 1 a\n\\s5\n\\v 2 b\n",
        "one paragraph, closed by the \\q1 and not by the \\s5"
    );
    let poetry = doc.extent(doc.node_of(source, "q1"));
    assert!(
        poetry.end as usize >= source.trim_end().len(),
        "and inside poetry too"
    );

    assert_eq!(
        codes(source, &ext),
        [],
        "no unknown-marker, no missing-paragraph"
    );
    let strict = codes(source, &registry(&[]));
    assert!(strict.contains(&Code::UnknownMarker), "{strict:?}");
    assert!(strict.contains(&Code::MissingParagraph), "{strict:?}");

    let usj = usfm_onion::usj::usj(source.as_bytes(), &doc.tokens, &doc.cst);
    let usx = usfm_onion::usx::usx(source.as_bytes(), &doc.tokens, &doc.cst);
    let html = usfm_onion::html::html(source.as_bytes(), &doc.tokens, &doc.cst);
    for out in [&usj, &usx, &html] {
        assert!(out.contains("s5"), "an export lost `s5`:\n{out}");
        assert!(
            !out.contains("zmsbare"),
            "a template name reached an export"
        );
    }
}

// ------------------------------------------------------------------- lint

fn codes(source: &str, ext: &Extensions) -> Vec<Code> {
    let doc = parse(source, ext);
    usfm_onion::lint::lint(source.as_bytes(), &doc.tokens, &doc.cst)
        .observations
        .iter()
        .map(|o| o.code)
        .collect()
}

/// `unknown-marker` is the whole of what a registration silences — and it
/// still fires for a name nobody registered and for a spelling the category
/// refuses.
#[test]
fn unknown_marker_is_silent_for_a_registered_name_only() {
    let ext = registry(&[("zmyp", ExtensionCategory::VersePara)]);
    let registered = "\\id GEN\n\\usfm 3.0\n\\c 1\n\\zmyp \\v 1 one\n";
    assert!(
        !codes(registered, &ext).contains(&Code::UnknownMarker),
        "a registered marker is not unknown"
    );

    let other = "\\id GEN\n\\usfm 3.0\n\\c 1\n\\zother \\v 1 one\n";
    assert!(
        codes(other, &ext).contains(&Code::UnknownMarker),
        "an unregistered one still is"
    );

    // Registered as a paragraph, spelled as a milestone: row 0.
    //
    // `unknown-marker` speaks for OPENERS only (`lint/flat.rs`), so an unknown
    // MILESTONE has never raised it — `\zaln-s` does not today either. The
    // claim here is the resolution, which is this feature's; the milestone gap
    // is older and untouched.
    let wrong = "\\id GEN\n\\usfm 3.0\n\\c 1\n\\zmyp-s \\*\\v 1 one\n";
    let doc = parse(wrong, &ext);
    assert!(
        doc.tokens
            .iter()
            .filter(|token| matches!(token.kind(), TokenKind::Milestone { .. }))
            .all(|token| token.marker_idx == generated::UNRESOLVED),
        "a spelling the category refuses resolves to row 0"
    );

    // And with nothing registered, the baseline is unchanged.
    let none = registry(&[]);
    assert!(codes(registered, &none).contains(&Code::UnknownMarker));
}

/// Context rules apply per TEMPLATE: a `sectionpara` extension is as illegal
/// in the book headers as `\s` is.
#[test]
fn context_rules_apply_per_template() {
    let ext = registry(&[("zmys", ExtensionCategory::SectionPara)]);
    let template = generated::template_for(ExtensionCategory::SectionPara);
    assert!(
        !generated::allowed_in(template, SpecContext::BookHeaders),
        "the `s` row is not legal in the headers, so neither is its template"
    );
    assert!(generated::allowed_in(template, SpecContext::ChapterContent));
    // And the registered marker carries that row.
    let source = "\\id GEN\n\\usfm 3.0\n\\zmys A heading\n\\c 1\n\\p \\v 1 one\n";
    let doc = parse(source, &ext);
    assert!(
        doc.tokens.iter().any(|token| token.marker_idx == template),
        "the marker resolved to the section template"
    );
}

// ------------------------------------------------------------- the name rule

/// Every export writes the SPELLING, never the template's name.
#[test]
fn the_exports_carry_the_spelled_name() {
    let ext = registry(&[
        ("zmyp", ExtensionCategory::VersePara),
        ("zmyc", ExtensionCategory::Char),
        ("zmyf", ExtensionCategory::Footnote),
    ]);
    let source = "\\id GEN\n\\c 1\n\\zmyp \\v 1 a \\zmyc word\\zmyc*\\zmyf + \\ft n\\zmyf*\n";
    let doc = parse(source, &ext);

    let usj = usfm_onion::usj::usj(source.as_bytes(), &doc.tokens, &doc.cst);
    let usx = usfm_onion::usx::usx(source.as_bytes(), &doc.tokens, &doc.cst);
    let html = usfm_onion::html::html(source.as_bytes(), &doc.tokens, &doc.cst);
    for name in ["zmyp", "zmyc", "zmyf"] {
        assert!(usj.contains(name), "USJ lost `{name}`:\n{usj}");
        assert!(usx.contains(name), "USX lost `{name}`:\n{usx}");
        assert!(html.contains(name), "HTML lost `{name}`:\n{html}");
    }
    // …and no export ever writes a template's own name.
    for out in [&usj, &usx, &html] {
        for template in ["zpara", "zchar", "zfoot"] {
            assert!(!out.contains(template), "a template name reached an export");
        }
    }
}

/// A cell extension aligns START, and the spec's own cells are unmoved.
///
/// The spec's `cell` category carries no alignment, so an extension has none
/// to spell: reading one out of a `z` name would invent a convention the spec
/// does not have, and silently align a `\zaligner` cell. The digit trim the
/// spec rows need — `\tcr1` ends in its column index, not in the `r` — is the
/// whole of what moved.
#[test]
fn a_cell_extension_aligns_start_and_the_spec_cells_are_unmoved() {
    let ext = registry(&[
        ("ztc", ExtensionCategory::Cell),
        ("ztcr", ExtensionCategory::Cell),
    ]);
    let source = "\\id GEN\n\\c 1\n\\p\n\\tr \\ztc1 a \\ztcr2 b\n";
    let doc = parse(source, &ext);
    let usj = usfm_onion::usj::usj(source.as_bytes(), &doc.tokens, &doc.cst);
    assert!(
        !usj.contains("\"align\":\"end\""),
        "no extension cell invents an alignment:\n{usj}"
    );
    assert_eq!(usj.matches("\"align\":\"start\"").count(), 2, "{usj}");

    let spec = "\\id GEN\n\\c 1\n\\p\n\\tr \\tc1 a \\tcr2 b\n";
    let doc = parse(spec, &ext);
    let usj = usfm_onion::usj::usj(spec.as_bytes(), &doc.tokens, &doc.cst);
    assert!(
        usj.contains("\"align\":\"end\""),
        "`\\tcr2` still aligns end:\n{usj}"
    );
    assert!(
        usj.contains("\"align\":\"start\""),
        "`\\tc1` still aligns start:\n{usj}"
    );
}

// -------------------------------------------------- mask, format and the diff

/// A `char` extension's markers drop from verse text exactly as `\add`'s do,
/// and a `footnote` extension's subtree drops whole.
#[test]
fn the_mask_reads_the_row() {
    let ext = registry(&[
        ("zmyc", ExtensionCategory::Char),
        ("zmyf", ExtensionCategory::Footnote),
    ]);
    let source = "\\id GEN\n\\c 1\n\\p \\v 1 Jesus \\zmyc wept\\zmyc*\\zmyf + \\ft why\\zmyf*.\n";
    let doc = parse(source, &ext);
    let verse = mask(
        source.as_bytes(),
        &doc.tokens,
        &doc.cst,
        &Filter::verse_text(),
    );
    assert_eq!(
        verse.text(source.as_bytes()).trim(),
        "Jesus wept.",
        "the character markers unwrap and the note subtree is removed"
    );

    // Unregistered, the same bytes read differently — the note's prose rides
    // in, because row 0 is not a Note.
    let none = registry(&[]);
    let doc = parse(source, &none);
    let verse = mask(
        source.as_bytes(),
        &doc.tokens,
        &doc.cst,
        &Filter::verse_text(),
    );
    assert!(
        verse.text(source.as_bytes()).contains("why"),
        "row 0 keeps the note prose, which is the behaviour a registration fixes"
    );
}

/// A `versepara` extension cuts a decision unit the way `\p` does — which is
/// to say it does not, since units are cut at `\c`/`\v`, and the marker rides
/// inside the unit it belongs to rather than becoming an unknown-marker
/// boundary.
#[test]
fn a_paragraph_extension_reads_as_a_paragraph_everywhere() {
    let ext = registry(&[("zmyp", ExtensionCategory::VersePara)]);
    let template = generated::template_for(ExtensionCategory::VersePara);
    assert_eq!(generated::kind(template), MarkerKind::Paragraph);

    let source = "\\id GEN\n\\c 1\n\\zmyp \\v 1 one\n\\zmyp \\v 2 two\n";
    let doc = parse(source, &ext);
    // Two paragraph nodes, each opened by the extension.
    let paras = doc
        .cst
        .nodes
        .iter()
        .skip(1) // the root opens on no token
        .filter(|node| doc.tokens[node.token as usize].marker_idx == template)
        .count();
    assert_eq!(paras, 2, "each `\\zmyp` opened a paragraph");

    // Formatting treats it as a paragraph: the structure cut keeps it, where
    // an unregistered `z` marker is an Unknown and drops.
    let structure = mask(
        source.as_bytes(),
        &doc.tokens,
        &doc.cst,
        &Filter::structure(),
    );
    assert!(
        structure.text(source.as_bytes()).contains("\\zmyp"),
        "a paragraph survives the structure cut: {:?}",
        structure.text(source.as_bytes())
    );
    let none = registry(&[]);
    let doc = parse(source, &none);
    let structure = mask(
        source.as_bytes(),
        &doc.tokens,
        &doc.cst,
        &Filter::structure(),
    );
    assert!(
        !structure.text(source.as_bytes()).contains("\\zmyp"),
        "an unknown marker does not"
    );
}

/// Nothing above disturbs a document with no extensions in it: the whole
/// feature is inert until something is registered.
#[test]
fn a_document_without_extensions_is_byte_identical() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../testData/exampleCorpora/en_ulb/08-RUT.usfm"
    ))
    .expect("the corpus book");
    let ext = registry(&[("zmyp", ExtensionCategory::VersePara)]);
    assert_eq!(
        lex_with(&source, &ext),
        lex(&source),
        "a registry nothing in the book names changes no token"
    );
}
