//! Every USFM marker as a named Word style, generated from onion's marker
//! table.
//!
//! ```text
//! row q, UpTo(4), Paragraph   ->  paragraph styles "USFM q" "USFM q1" … "USFM q4"
//! row nd, Character           ->  character style "USFM nd"
//! row f, Note                 ->  paragraph style "USFM f", the footnote body
//! \wj … \+nd Lord\+nd* …      ->  character style "USFM wj+nd", made on export
//! ```
//!
//! The NAME is the contract: import reads `w:name` and never `w:styleId`,
//! because LibreOffice rewrites ids when it saves. `src/styles.xml` is this
//! module's output for the default options, checked in and kept fresh by a
//! test; `cargo run -p docx --bin codegen` rewrites it.
//!
//! The curated set has a look and sits in Word's style gallery (`w:qFormat`).
//! Every other marker's style is plain and reached from the styles pane.

use usfm_onion::tables::generated as table;
use usfm_onion::tables::schema::{MarkerKind, Numbering, SpellingShape};

/// Every style name this crate writes starts with this.
pub const PREFIX: &str = "USFM ";
/// A run whose text is copied into the USFM verbatim: `//`, `\ts\*`.
pub const RAW: &str = "raw";
/// A note's caller when it is not `+`, as the first run of the note's body.
pub const CALLER: &str = "caller";
/// Text and verses that follow a heading with no paragraph marker of their
/// own (`\s5` then `\v 8 …`), which USFM allows and USX keeps as is.
pub const LOOSE: &str = "(no marker)";
/// Separates nested character markers in one style name: `wj+nd`.
pub const NEST: char = '+';

/// A note's own peers, which sit beside each other without closers.
pub const NOTE_PEERS: &[&str] = &[
    "fdc", "fk", "fl", "fp", "fq", "fqa", "fr", "ft", "fw", "xdc", "xk", "xnt", "xo", "xop",
    "xot", "xq", "xt", "xta",
];

/// The markers people authoring in Word see first.
pub const CURATED: &[&str] = &[
    "c", "p", "m", "q1", "q2", "s1", "mt1", "d", "b", "v", "nd", "add", "wj", "fr", "ft", "fq",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    Character,
}

/// The style id for a style name. Ids only have to be unique within one file.
pub fn id(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c,
            NEST => '_',
            _ => '-',
        })
        .filter(|&c| c != '-')
        .collect()
}

pub fn name(marker: &str) -> String {
    format!("{PREFIX}{marker}")
}

/// Every marker that gets a style, in table order, with its kind.
pub fn markers() -> Vec<(String, Kind)> {
    let mut out: Vec<(String, Kind)> = Vec::new();
    for idx in 1..table::FIRST_EXTENSION_ROW {
        let stem = table::name(idx);
        let kind = match table::kind(idx) {
            MarkerKind::Paragraph | MarkerKind::Chapter | MarkerKind::Note => Kind::Paragraph,
            MarkerKind::Header if stem == "id" => Kind::Paragraph,
            MarkerKind::Character | MarkerKind::Verse
                if table::shape(idx) != SpellingShape::MilestoneOnly =>
            {
                Kind::Character
            }
            _ => continue,
        };
        let spellings: Vec<String> = match table::numbering(idx) {
            Numbering::Unnumbered => vec![stem.to_owned()],
            Numbering::UpTo(n) => std::iter::once(stem.to_owned())
                .chain((1..=n).map(|i| format!("{stem}{i}")))
                .collect(),
            Numbering::Unbounded => vec![stem.to_owned(), format!("{stem}1")],
            Numbering::TableColumns => continue,
        };
        for s in spellings {
            if !out.iter().any(|(m, _)| *m == s) {
                out.push((s, kind));
            }
        }
    }
    out.push((LOOSE.to_owned(), Kind::Paragraph));
    out.push((RAW.to_owned(), Kind::Character));
    out.push((CALLER.to_owned(), Kind::Character));
    out
}

/// The look of one marker's style: what goes in `w:pPr` and `w:rPr`, and
/// which built-in style it is based on.
struct Look {
    based_on: &'static str,
    ppr: &'static str,
    rpr: &'static str,
}

fn look(marker: &str, kind: Kind) -> Look {
    let plain_based_on = match kind {
        Kind::Paragraph => "Normal",
        Kind::Character => "",
    };
    let (based_on, ppr, rpr): (&str, &str, &str) = match marker {
        "p" => ("Normal", r#"<w:ind w:firstLine="360"/>"#, ""),
        "q" | "q1" => ("Normal", r#"<w:spacing w:after="0"/><w:ind w:left="720" w:hanging="360"/>"#, ""),
        "q2" => ("Normal", r#"<w:spacing w:after="0"/><w:ind w:left="1080" w:hanging="360"/>"#, ""),
        "q3" => ("Normal", r#"<w:spacing w:after="0"/><w:ind w:left="1440" w:hanging="360"/>"#, ""),
        "q4" => ("Normal", r#"<w:spacing w:after="0"/><w:ind w:left="1800" w:hanging="360"/>"#, ""),
        "s" | "s1" => ("Heading2", "", ""),
        "s2" | "s3" | "s4" | "ms" | "ms1" | "ms2" | "ms3" => ("Normal", r#"<w:keepNext/>"#, "<w:b/>"),
        "mt" | "mt1" => ("Title", "", ""),
        "mt2" | "mt3" | "mt4" => ("Normal", r#"<w:jc w:val="center"/>"#, "<w:b/>"),
        "c" => ("Normal", r#"<w:keepNext/><w:spacing w:before="240"/>"#, r#"<w:b/><w:sz w:val="32"/>"#),
        "d" | "r" | "mr" | "sr" => ("Normal", "", "<w:i/>"),
        "id" | "ide" | "h" | "h1" | "h2" | "h3" | "toc" | "toc1" | "toc2" | "toc3" | "toca"
        | "toca1" | "toca2" | "toca3" | "rem" | "usfm" | "sts" => {
            ("Normal", "", r#"<w:color w:val="808080"/><w:sz w:val="18"/>"#)
        }
        "f" | "fe" | "ef" | "x" | "ex" => ("FootnoteText", "", ""),
        "v" => ("", "", r#"<w:vertAlign w:val="superscript"/>"#),
        "nd" => ("", "", "<w:smallCaps/>"),
        "add" | "fq" | "fqa" | "it" | "em" | "bk" => ("", "", "<w:i/>"),
        "fr" | "xo" | "bd" => ("", "", "<w:b/>"),
        "wj" => ("", "", r#"<w:color w:val="C00000"/>"#),
        "sc" => ("", "", "<w:smallCaps/>"),
        "sup" | "caller" => ("", "", r#"<w:vertAlign w:val="superscript"/>"#),
        "raw" => ("", "", r#"<w:color w:val="808080"/>"#),
        _ => (plain_based_on, "", ""),
    };
    let based_on = if based_on.is_empty() { plain_based_on } else { based_on };
    Look { based_on, ppr, rpr }
}

/// One `<w:style>` element for a marker, or for a nested stack (`wj+nd`),
/// whose look is its innermost marker's.
pub fn style_xml(marker: &str, kind: Kind) -> String {
    let name = name(marker);
    let innermost = marker.rsplit(NEST).next().unwrap_or(marker);
    let look = look(innermost, kind);
    let mut s = String::new();
    s.push_str(match kind {
        Kind::Paragraph => r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId=""#,
        Kind::Character => r#"<w:style w:type="character" w:customStyle="1" w:styleId=""#,
    });
    s.push_str(&id(&name));
    s.push_str(r#""><w:name w:val=""#);
    s.push_str(&crate::xml::attr(&name));
    s.push_str(r#""/>"#);
    if !look.based_on.is_empty() {
        s.push_str(r#"<w:basedOn w:val=""#);
        s.push_str(look.based_on);
        s.push_str(r#""/>"#);
    }
    if kind == Kind::Paragraph {
        s.push_str(r#"<w:next w:val=""#);
        s.push_str(&id(&name));
        s.push_str(r#""/>"#);
    }
    if CURATED.contains(&marker) {
        s.push_str("<w:qFormat/>");
    }
    if kind == Kind::Paragraph && !look.ppr.is_empty() {
        s.push_str("<w:pPr>");
        s.push_str(look.ppr);
        s.push_str("</w:pPr>");
    }
    if !look.rpr.is_empty() {
        s.push_str("<w:rPr>");
        s.push_str(look.rpr);
        s.push_str("</w:rPr>");
    }
    s.push_str("</w:style>\n");
    s
}

/// Where export puts the default run properties (font, right-to-left).
pub const RPR_DEFAULT: &str = "<w:rPrDefault><w:rPr/></w:rPrDefault>";
/// Where export puts the default paragraph properties (right-to-left).
pub const PPR_DEFAULT: &str = "<w:pPrDefault><w:pPr/></w:pPrDefault>";
/// Where export adds the styles a document needs beyond the table's.
pub const END: &str = "</w:styles>";

/// `word/styles.xml` for the default options: what `src/styles.xml` holds.
pub fn styles_xml() -> String {
    let mut s = String::new();
    s.push_str(crate::xml::DECLARATION);
    s.push_str(r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
    s.push('\n');
    s.push_str("<w:docDefaults>");
    s.push_str(RPR_DEFAULT);
    s.push_str(PPR_DEFAULT);
    s.push_str("</w:docDefaults>\n");
    s.push_str(BUILT_INS);
    for (marker, kind) in markers() {
        s.push_str(&style_xml(&marker, kind));
    }
    s.push_str(END);
    s.push('\n');
    s
}

/// The built-in styles the USFM ones are based on, and the two footnotes use.
const BUILT_INS: &str = concat!(
    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="120"/></w:pPr></w:style>"#,
    "\n",
    r#"<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/></w:style>"#,
    "\n",
    r#"<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:jc w:val="center"/><w:spacing w:after="240"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style>"#,
    "\n",
    r#"<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/></w:rPr></w:style>"#,
    "\n",
    r#"<w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style>"#,
    "\n",
    r#"<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>"#,
    "\n",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<String> = markers().iter().map(|(m, _)| id(&name(m))).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn every_curated_marker_has_a_style() {
        let all = markers();
        for m in CURATED {
            assert!(all.iter().any(|(s, _)| s == m), "{m}");
        }
    }

    #[test]
    fn spellings_expand() {
        let all = markers();
        for m in ["q", "q1", "q4", "s2", "mt1", "toc3", "f", "x", "ft", "v", "c", "id"] {
            assert!(all.iter().any(|(s, _)| s == m), "{m}");
        }
        assert!(!all.iter().any(|(s, _)| s == "q5"));
        assert!(!all.iter().any(|(s, _)| s == "tc1"));
    }
}
