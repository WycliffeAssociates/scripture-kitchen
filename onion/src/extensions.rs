//! The registry: user `\z` markers, resolved to the template row their
//! `\category` says they behave as.
//!
//! ```text
//! set_extensions(&[CustomMarker { name: "zaln", category: Milestone, .. }])
//!
//! lex("\\zaln-s |x-strong=\"H0430\"\\*word\\zaln-e\\*")
//!     -> Milestone{end:false} marker_idx = zms   ← pairs, takes attributes
//!        …                                          exactly as `\qt-s` does
//!
//! lex("\\zaln word")           // plain spelling of a milestone name
//!     -> Marker marker_idx = 0                  ← the spelling disagrees
//!
//! set_extensions_with(&[CustomMarker { name: "s5", category: Standalone, .. }],
//!                     &ExtensionOptions { relax_z_prefix: true })
//!
//! lex("\\p \\v 1 a \\s5 \\v 3 b")
//!     -> \s5 is a bare point                     ← the \p stays open past it
//! ```
//!
//! **Legacy names.** `relax_z_prefix` admits a name without the `z`, for a
//! host that has to read markup it cannot change — en_ulb's `\s5` chunk
//! marker. The name must be one the spec table resolves to nothing in every
//! spelling, so `s5` passes (`\s` stops at level 4) and `s1` or `p` is a
//! report: a legacy name can add a marker, never redefine one. A
//! `markers.ext` file stays `z`-only.
//!
//! A user marker is a spec marker the table has not met, and the spec says
//! which one in one field. So this module maps a NAME to a template row and
//! stops: every behaviour — the caller payload, the note scope, the closing
//! rule, the lint contexts, the mask treatment, the export shape — is the
//! row's, and nothing downstream asks whether a marker is an extension to
//! decide what it does. The one thing a template cannot carry is the name the
//! document spelled, which is what [`generated::is_extension`] exists for.
//!
//! The registry never reads a file. `mise::extensions::parse_markers_ext` is
//! one translator into the list it takes; a `custom.sty` reader later is
//! another. Neither touches the lexer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use rustc_hash::FxHashMap;

pub use mise::extensions::{
    CustomMarker, ExtensionCategory, Malformed, check_legacy_name, check_name,
};

use crate::tables::generated::{self, MarkerIdx, UNRESOLVED};
use crate::tables::schema::{Numbering, SpellingShape};

/// A resolved set of user markers: name bytes to template row, plus the list
/// as declared, so a host can show what it installed.
#[derive(Debug, Default)]
pub struct Extensions {
    by_name: FxHashMap<Box<[u8]>, MarkerIdx>,
    declared: Vec<CustomMarker>,
    /// Some registered name lacks the `z`, so a spec miss asks the registry.
    legacy: bool,
}

/// How a list is judged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtensionOptions {
    /// Admit names without the `z` that the spec does not define.
    pub relax_z_prefix: bool,
}

impl Extensions {
    /// Resolves a list, keeping what it can and reporting the rest.
    ///
    /// Reports are VALUES: one bad entry costs one entry, never the list. A
    /// report's `line` is 0 — these faults are the list's, not a file's, and
    /// the file reader fills the real line when it is the one reporting.
    pub fn new(list: &[CustomMarker]) -> (Self, Vec<Malformed>) {
        Self::new_with(list, &ExtensionOptions::default())
    }

    /// The same under `opts`.
    pub fn new_with(list: &[CustomMarker], opts: &ExtensionOptions) -> (Self, Vec<Malformed>) {
        let mut out = Self::default();
        let mut reports = Vec::new();
        for marker in list {
            let legacy = opts.relax_z_prefix && !marker.name.starts_with('z');
            let reason = if legacy {
                check_legacy_name(&marker.name).or_else(|| spec_defines(&marker.name))
            } else {
                check_name(&marker.name)
            };
            let reason = if let Some(reason) = reason {
                Some(reason)
            } else if out.by_name.contains_key(marker.name.as_bytes()) {
                Some("duplicate name; the first definition stands")
            } else {
                None
            };
            if let Some(reason) = reason {
                reports.push(Malformed {
                    line: 0,
                    name: Some(marker.name.clone()),
                    reason,
                });
                continue;
            }
            // `attribute` and `internal` describe USX internals and have no
            // USFM behaviour: accepted so a valid file never errors, and
            // registered nowhere, so the name stays at row 0 as today.
            let template = generated::template_for(marker.category);
            if template != UNRESOLVED {
                out.by_name.insert(marker.name.as_bytes().into(), template);
                out.legacy |= legacy;
            }
            out.declared.push(marker.clone());
        }
        (out, reports)
    }

    /// The list as declared, in order, including the two categories that
    /// register nothing.
    pub fn declared(&self) -> &[CustomMarker] {
        &self.declared
    }

    /// No name in this registry maps to a row — so the scanner's `z` branch
    /// can answer [`UNRESOLVED`] without a lookup.
    ///
    /// NOT "declares nothing": `attribute` and `internal` entries are accepted
    /// and sit in [`declared`](Self::declared) while registering no name, so a
    /// registry of those two alone resolves nothing and declares two.
    pub fn resolves_nothing(&self) -> bool {
        self.by_name.is_empty()
    }

    /// A `z` lexeme's row, or [`UNRESOLVED`].
    ///
    /// `lexeme` is the name as spelled — no leading `\`, no trailing `*`, the
    /// `-s`/`-e` still on it — and `shape` is what the scanner already
    /// classified, exactly as [`generated::marker_idx`] takes them.
    ///
    /// **The spelling has to agree with the category.** The token KIND is
    /// decided by the spelling before the table is consulted, so a `char` name
    /// spelled `\zfoo-s` would hand the walker a milestone token sitting on a
    /// character row. The template's own `shape` column carries the rule and
    /// this is one [`SpellingShape::overlaps`]; a disagreement is row 0, which
    /// is what the spelling gets today anyway.
    pub fn resolve(&self, lexeme: &[u8], shape: SpellingShape) -> MarkerIdx {
        if self.resolves_nothing() {
            return UNRESOLVED;
        }
        let stripped = match lexeme {
            [head @ .., b'-', b's' | b'e'] => head,
            whole => whole,
        };
        if let Some(&idx) = self.by_name.get(stripped) {
            return self.agreed(idx, shape);
        }
        // `cell` is the one numbered category, so the digits are stripped only
        // after the whole name missed — a name that legally ENDS in a digit
        // (`\z9`) matched above and never reaches here.
        let stem = stripped.len()
            - stripped
                .iter()
                .rev()
                .take_while(|b| b.is_ascii_digit())
                .count();
        if stem == stripped.len() {
            return UNRESOLVED;
        }
        match self.by_name.get(&stripped[..stem]) {
            Some(&idx) if generated::numbering(idx) == Numbering::TableColumns => {
                self.agreed(idx, shape)
            }
            _ => UNRESOLVED,
        }
    }

    fn agreed(&self, idx: MarkerIdx, shape: SpellingShape) -> MarkerIdx {
        if generated::shape(idx).overlaps(shape) {
            idx
        } else {
            UNRESOLVED
        }
    }
}

/// Why a legacy name cannot be registered: the spec table already resolves
/// it in some spelling.
fn spec_defines(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    [SpellingShape::PlainOnly, SpellingShape::MilestoneOnly]
        .into_iter()
        .any(|shape| generated::marker_idx(bytes, shape) != UNRESOLVED)
        .then_some("names a spec marker; a legacy name can only add one")
}

/// THE resolution door: a `z` name through the registry, everything else
/// through the spec table — and a spec miss through the registry too when it
/// holds a legacy name. The registry never sees a name the spec resolves.
///
/// [`generated::marker_idx`] is a projection of the authored rows and knows
/// nothing about a runtime registry — which is the invariant that keeps the
/// wire and the generated table out of this feature entirely.
#[inline]
pub fn marker_idx(lexeme: &[u8], shape: SpellingShape, ext: &Extensions) -> MarkerIdx {
    if lexeme.first() == Some(&b'z') {
        return ext.resolve(lexeme, shape);
    }
    let idx = generated::marker_idx(lexeme, shape);
    if idx == UNRESOLVED && ext.legacy {
        return ext.resolve(lexeme, shape);
    }
    idx
}

// ------------------------------------------------------- the process registry

static REGISTRY: RwLock<Option<Arc<Extensions>>> = RwLock::new(None);
static EMPTY: OnceLock<Arc<Extensions>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn empty() -> Arc<Extensions> {
    Arc::clone(EMPTY.get_or_init(|| Arc::new(Extensions::default())))
}

/// The installed registry. Read ONCE per [`crate::lex`] call into a local, so
/// no lexeme ever takes the lock.
///
/// Poison is recovered from rather than propagated: what the lock guards is
/// one whole `Arc`, replaced in a single assignment, so a panic elsewhere
/// cannot leave it half-written and there is nothing for poison to warn about.
pub fn current() -> Arc<Extensions> {
    let guard = REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
    guard.as_ref().map_or_else(empty, Arc::clone)
}

/// Installs a list process-wide, replacing whatever was there, and returns
/// what it could not keep. Passing `&[]` clears.
///
/// Bumps [`generation`], which is how a cache keyed on content alone learns
/// that the same bytes now parse differently.
pub fn set_extensions(list: &[CustomMarker]) -> Vec<Malformed> {
    set_extensions_with(list, &ExtensionOptions::default())
}

/// The same under `opts`.
pub fn set_extensions_with(list: &[CustomMarker], opts: &ExtensionOptions) -> Vec<Malformed> {
    let (extensions, reports) = Extensions::new_with(list, opts);
    *REGISTRY.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(extensions));
    GENERATION.fetch_add(1, Ordering::Release);
    reports
}

/// Bumped by every [`set_extensions`]. A product derived under one generation
/// is stale under another — the same bytes resolve to different rows.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::lex_with;
    use crate::tables::schema::MarkerKind;

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

    /// The first marker token's row, lexed against `ext`.
    fn row(source: &str, ext: &Extensions) -> MarkerIdx {
        lex_with(source, ext)
            .iter()
            .find(|token| {
                matches!(
                    token.kind(),
                    crate::TokenKind::Marker { .. }
                        | crate::TokenKind::Milestone { .. }
                        | crate::TokenKind::ClosingMarker { .. }
                )
            })
            .expect("a marker token")
            .marker_idx
    }

    /// Every behaviour-bearing category resolves a user name to its template,
    /// and the row carries the spec marker's kind.
    #[test]
    fn every_category_resolves_to_its_template() {
        let cases: &[(ExtensionCategory, &str, MarkerKind)] = &[
            (ExtensionCategory::Header, "\\zmy x", MarkerKind::Paragraph),
            (ExtensionCategory::Title, "\\zmy x", MarkerKind::Paragraph),
            (
                ExtensionCategory::Introduction,
                "\\zmy x",
                MarkerKind::Paragraph,
            ),
            (
                ExtensionCategory::SectionPara,
                "\\zmy x",
                MarkerKind::Paragraph,
            ),
            (
                ExtensionCategory::VersePara,
                "\\zmy x",
                MarkerKind::Paragraph,
            ),
            (ExtensionCategory::List, "\\zmy x", MarkerKind::Paragraph),
            (
                ExtensionCategory::OtherPara,
                "\\zmy x",
                MarkerKind::Paragraph,
            ),
            (
                ExtensionCategory::Footnote,
                "\\zmy + x\\zmy*",
                MarkerKind::Note,
            ),
            (
                ExtensionCategory::CrossReference,
                "\\zmy + x\\zmy*",
                MarkerKind::Note,
            ),
            (
                ExtensionCategory::Char,
                "\\zmy x\\zmy*",
                MarkerKind::Character,
            ),
            (
                ExtensionCategory::IntroChar,
                "\\zmy x\\zmy*",
                MarkerKind::Character,
            ),
            (
                ExtensionCategory::ListChar,
                "\\zmy x\\zmy*",
                MarkerKind::Character,
            ),
            (
                ExtensionCategory::FootnoteChar,
                "\\zmy x",
                MarkerKind::Character,
            ),
            (
                ExtensionCategory::CrossReferenceChar,
                "\\zmy x",
                MarkerKind::Character,
            ),
            (
                ExtensionCategory::Milestone,
                "\\zmy-s \\*",
                MarkerKind::Milestone,
            ),
            (
                ExtensionCategory::Standalone,
                "\\zmy x",
                MarkerKind::Milestone,
            ),
            (ExtensionCategory::Cell, "\\zmy1 x", MarkerKind::TableCell),
        ];
        for (category, source, kind) in cases {
            let ext = registry(&[("zmy", *category)]);
            let idx = row(source, &ext);
            assert_eq!(
                idx,
                generated::template_for(*category),
                "{category}: {source:?}"
            );
            assert!(generated::is_extension(idx), "{category}: not a template");
            assert_eq!(generated::kind(idx), *kind, "{category}");
        }
    }

    /// The two USX-internal words parse and register nothing.
    #[test]
    fn attribute_and_internal_stay_at_row_zero() {
        for category in [ExtensionCategory::Attribute, ExtensionCategory::Internal] {
            let ext = registry(&[("zmy", category)]);
            assert!(ext.resolves_nothing(), "{category} registered something");
            assert_eq!(row("\\zmy x", &ext), UNRESOLVED);
            // …and the declaration is still reported back to the host.
            assert_eq!(ext.declared().len(), 1);
        }
    }

    /// The spelling has to agree with the category, because the spelling has
    /// already decided the token kind.
    #[test]
    fn a_spelling_that_disagrees_with_the_category_is_row_zero() {
        let milestone = registry(&[("zaln", ExtensionCategory::Milestone)]);
        assert_eq!(
            row("\\zaln-s \\*", &milestone),
            generated::template_for(ExtensionCategory::Milestone)
        );
        assert_eq!(row("\\zaln word", &milestone), UNRESOLVED);

        let character = registry(&[("zfoo", ExtensionCategory::Char)]);
        assert_eq!(
            row("\\zfoo x\\zfoo*", &character),
            generated::template_for(ExtensionCategory::Char)
        );
        assert_eq!(row("\\zfoo-s x", &character), UNRESOLVED);

        // `standalone` is bare: the milestone spelling is someone else's.
        let standalone = registry(&[("zms", ExtensionCategory::Standalone)]);
        let template = generated::template_for(ExtensionCategory::Standalone);
        assert_eq!(row("\\zms x", &standalone), template);
        assert_eq!(row("\\zms-s \\*", &standalone), UNRESOLVED);
    }

    /// `cell` is the one numbered category: its digits are a column index,
    /// everyone else's are part of the name or nothing at all.
    #[test]
    fn only_cell_takes_digits() {
        let cell = registry(&[("ztc", ExtensionCategory::Cell)]);
        let template = generated::template_for(ExtensionCategory::Cell);
        assert_eq!(row("\\ztc x", &cell), template);
        assert_eq!(row("\\ztc1 x", &cell), template);
        assert_eq!(row("\\ztc1-2 x", &cell), template);
        assert_eq!(lex_with("\\ztc2 x", &cell)[0].level, 2, "the column index");

        let para = registry(&[("zp", ExtensionCategory::VersePara)]);
        assert_eq!(
            row("\\zp x", &para),
            generated::template_for(ExtensionCategory::VersePara)
        );
        assert_eq!(row("\\zp1 x", &para), UNRESOLVED, "a level it never had");

        // A registered name that legally ENDS in a digit matches whole.
        let digit = registry(&[("z9", ExtensionCategory::VersePara)]);
        assert_eq!(
            row("\\z9 x", &digit),
            generated::template_for(ExtensionCategory::VersePara)
        );
    }

    /// An unregistered `z` marker is row 0, exactly as before this feature.
    #[test]
    fn an_unregistered_extension_is_unchanged() {
        let ext = registry(&[("zmy", ExtensionCategory::Char)]);
        assert_eq!(row("\\zother x", &ext), UNRESOLVED);
        assert_eq!(row("\\zaln-s \\*", &ext), UNRESOLVED);
        // And a template's own name is not a marker anyone can spell.
        assert_eq!(row("\\zpara x", &ext), UNRESOLVED);
        assert_eq!(row("\\zchar x\\zchar*", &ext), UNRESOLVED);
    }

    /// Bad entries cost themselves and nothing else.
    #[test]
    fn a_bad_entry_is_reported_and_the_rest_install() {
        let list = |pairs: &[(&str, ExtensionCategory)]| -> Vec<CustomMarker> {
            pairs
                .iter()
                .map(|(name, category)| CustomMarker {
                    name: (*name).to_owned(),
                    category: *category,
                    description: String::new(),
                    attributes: Vec::new(),
                })
                .collect()
        };
        let (ext, reports) = Extensions::new(&list(&[
            ("foo", ExtensionCategory::Char),
            ("zok", ExtensionCategory::Char),
            ("z-x", ExtensionCategory::Char),
            ("zok", ExtensionCategory::Footnote),
            ("", ExtensionCategory::Char),
        ]));
        assert_eq!(
            reports
                .iter()
                .map(|report| report.reason)
                .collect::<Vec<_>>(),
            [
                "name does not start with z",
                "name is not ASCII alphanumeric",
                "duplicate name; the first definition stands",
                "\\marker with no name",
            ]
        );
        assert_eq!(
            row("\\zok x\\zok*", &ext),
            generated::template_for(ExtensionCategory::Char),
            "the first definition stands, as a char"
        );
    }

    /// A legacy name registers only when asked for, and only where the spec
    /// has nothing: it adds a marker, never redefines one.
    #[test]
    fn a_legacy_name_needs_the_relaxed_prefix_and_a_spec_miss() {
        let list = |name: &str| {
            vec![CustomMarker {
                name: name.to_owned(),
                category: ExtensionCategory::Standalone,
                description: String::new(),
                attributes: Vec::new(),
            }]
        };
        let relaxed = ExtensionOptions {
            relax_z_prefix: true,
        };
        let template = generated::template_for(ExtensionCategory::Standalone);

        let (strict, reports) = Extensions::new(&list("s5"));
        assert_eq!(reports[0].reason, "name does not start with z");
        assert_eq!(row("\\s5 x", &strict), UNRESOLVED);

        let (ext, reports) = Extensions::new_with(&list("s5"), &relaxed);
        assert!(reports.is_empty(), "{reports:?}");
        assert_eq!(row("\\s5 x", &ext), template);
        assert_eq!(row("\\s5-s \\*", &ext), UNRESOLVED, "bare only");
        assert_eq!(
            row("\\s1 x", &ext),
            generated::marker_idx(b"s1", SpellingShape::PlainOnly),
            "spec names still resolve through the spec"
        );

        for spec in ["s1", "p", "qt", "ts"] {
            let (ext, reports) = Extensions::new_with(&list(spec), &relaxed);
            assert_eq!(
                reports[0].reason, "names a spec marker; a legacy name can only add one",
                "{spec}"
            );
            assert!(ext.resolves_nothing(), "{spec}");
        }
        let (_, reports) = Extensions::new_with(&list("s-5"), &relaxed);
        assert_eq!(reports[0].reason, "name is not ASCII alphanumeric");
        // `z` names are judged exactly as without the option.
        let (ext, reports) = Extensions::new_with(&list("zbare"), &relaxed);
        assert!(reports.is_empty());
        assert_eq!(row("\\zbare x", &ext), template);
    }

    /// Installing bumps the generation, which is what a content-keyed cache
    /// watches.
    /// Clears the process registry however a test ends, so a panic cannot
    /// leave a marker installed for everything else sharing the process.
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            set_extensions(&[]);
        }
    }

    #[test]
    fn installing_bumps_the_generation() {
        let _restore = Restore;
        let before = generation();
        let reports = set_extensions(&[CustomMarker {
            name: "zgen".to_owned(),
            category: ExtensionCategory::VersePara,
            description: String::new(),
            attributes: Vec::new(),
        }]);
        assert!(reports.is_empty());
        assert!(generation() > before);
        assert_eq!(
            crate::lex("\\zgen x")[0].marker_idx,
            generated::template_for(ExtensionCategory::VersePara),
            "the global door sees it"
        );
        let cleared = generation();
        assert!(set_extensions(&[]).is_empty());
        assert!(generation() > cleared);
        assert_eq!(crate::lex("\\zgen x")[0].marker_idx, UNRESOLVED);
    }
}
