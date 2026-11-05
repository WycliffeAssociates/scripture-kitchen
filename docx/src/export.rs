//! USFM to .docx: onion's USX folded into WordprocessingML.
//!
//! ```text
//! \c 1                 <w:p pStyle="USFM c">1</w:p>
//! \p                   <w:p pStyle="USFM p">
//! \v 1 The \nd Lord\nd*  <w:r rStyle="USFM v">1</w:r><w:r>The </w:r>
//!                        <w:r rStyle="USFM nd">Lord</w:r>…
//! \f + \fr 1.1 \ft x\f*  <w:footnoteReference w:id="1"/>  body in footnotes.xml,
//!                        paragraph style "USFM f", runs "USFM fr" and "USFM ft"
//! \ts\*  //            <w:r rStyle="USFM raw">\ts\*</w:r>   copied back verbatim
//! ```
//!
//! USX and not USJ, because USX keeps a space that sits alone between two
//! spans (`\nd Lord\nd* \add God\add*`), and a reader of the document sees it.
//!
//! What has nowhere to live in Word is reported as [`Unsupported`], never
//! hidden: attributes (`\w x|lemma="y"\w*`), milestones with attributes,
//! figures, `\ca`/`\cp`/`\va`/`\vp`. Their text is kept where they have any;
//! a table's cells and a sidebar's paragraphs come out as unstyled paragraphs.

use std::collections::BTreeSet;

use crate::dom::{self, Element, Node};
use crate::styles::{self, Kind};
use crate::xml;
use crate::{ExportOptions, Exported, Unsupported};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STYLES: &str = include_str!("styles.xml");
/// The USX attributes this export writes somewhere.
const CARRIED: &[&str] = &["style", "number", "sid", "eid", "vid", "code", "caller"];

pub fn export(usfm: &str, options: &ExportOptions) -> Exported {
    let w = fold(usfm);
    let book = w.book.clone();
    let source = crate::Source::of(usfm.as_bytes());
    let parts = Parts {
        document: w.document(),
        footnotes: w.footnotes(),
        styles: w.styles(options),
        custom: Some(crate::props::write(book.as_deref(), &source)),
        main: DOCUMENT,
    };
    Exported { book, docx: parts.zip(), unsupported: w.unsupported }
}

/// What a new document from the template holds: one of each paragraph a book
/// starts with, to type over.
const STARTER: &str = "\\id XXX Book name\n\\h Book name\n\\toc1 The Book of …\n\\toc2 Book name\n\\toc3 Abbr\n\\mt1 Book name\n\\c 1\n\\s1 A heading\n\\p\n\\v 1 The first verse.\n\\v 2 The second.\n\\q1\n\\v 3 A line of poetry,\n\\q2 and the next one indented.\n";

/// The template (.dotx): every style, a starter book, no source.
pub fn template() -> Vec<u8> {
    let w = fold(STARTER);
    Parts {
        document: w.document(),
        footnotes: w.footnotes(),
        styles: w.styles(&ExportOptions::default()),
        custom: None,
        main: TEMPLATE,
    }
    .zip()
}

fn fold(usfm: &str) -> Writer {
    let tokens = usfm_onion::lex(usfm);
    let cst = usfm_onion::cst::build(&tokens);
    let usx = usfm_onion::usx::usx(usfm.as_bytes(), &tokens, &cst);
    let root = dom::parse("usx", usx.as_bytes()).expect("onion's USX is XML");
    let mut w = Writer::new();
    w.blocks(&root.children);
    w.close_loose();
    w
}

fn style_of(e: &Element) -> &str {
    e.attr("style").unwrap_or("")
}

struct Writer {
    body: String,
    notes: String,
    in_note: bool,
    next_note: u32,
    /// A paragraph opened for inline content that sat outside any paragraph.
    loose: bool,
    /// The style of the run just written, so two touching spans of one
    /// style are not read back as one.
    last: Option<String>,
    base: BTreeSet<String>,
    extra: Vec<(String, Kind)>,
    book: Option<String>,
    at: String,
    unsupported: Vec<Unsupported>,
}

impl Writer {
    fn new() -> Self {
        Self {
            body: String::new(),
            notes: String::new(),
            in_note: false,
            next_note: 1,
            loose: false,
            last: None,
            base: styles::markers().into_iter().map(|(m, _)| m).collect(),
            extra: Vec::new(),
            book: None,
            at: String::new(),
            unsupported: Vec::new(),
        }
    }

    fn out(&mut self) -> &mut String {
        if self.in_note { &mut self.notes } else { &mut self.body }
    }

    fn report(&mut self, what: String) {
        let at = if self.at.is_empty() { self.book.clone().unwrap_or_default() } else { self.at.clone() };
        self.unsupported.push(Unsupported { at, what });
    }

    /// The style id for a marker, adding a style when the table has none.
    fn style(&mut self, marker: &str, kind: Kind) -> String {
        if !self.base.contains(marker) && !self.extra.iter().any(|(m, _)| m == marker) {
            self.extra.push((marker.to_owned(), kind));
        }
        styles::id(&styles::name(marker))
    }

    fn blocks(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Element(e) => self.block(e),
                Node::Text(t) if t.trim().is_empty() => {}
                Node::Text(_) => self.loose_inline(node),
            }
        }
    }

    fn block(&mut self, e: &Element) {
        match e.name.as_str() {
            "book" => {
                self.close_loose();
                let code = e.attr("code").unwrap_or("").to_owned();
                let rest = e.text();
                let text = if rest.trim().is_empty() { code.clone() } else { format!("{code} {}", rest.trim()) };
                self.book = Some(code.clone());
                self.at = code;
                self.paragraph_of_text("id", &text);
            }
            // A verse's or chapter's end: nothing to write.
            "chapter" | "verse" if e.attr("eid").is_some() => {}
            "chapter" => {
                self.close_loose();
                self.at = e.attr("sid").unwrap_or("").to_owned();
                self.lifted(e, "\\c");
                self.paragraph_of_text("c", e.attr("number").unwrap_or(""));
            }
            "para" => {
                self.close_loose();
                let marker = style_of(e);
                if marker.is_empty() {
                    self.report("a paragraph with no marker".to_owned());
                    self.open_paragraph(None);
                } else {
                    let id = self.style(marker, Kind::Paragraph);
                    self.open_paragraph(Some(&id));
                }
                self.inline(&e.children, &mut Vec::new());
                self.end_paragraph();
            }
            t @ ("table" | "row" | "cell" | "sidebar" | "periph") => {
                self.close_loose();
                match t {
                    "sidebar" => self.report("a sidebar (\\esb); its paragraphs are kept".to_owned()),
                    "periph" => self.report("a peripheral (\\periph); its paragraphs are kept".to_owned()),
                    "table" => self.report("a table; its cells are kept as paragraphs".to_owned()),
                    _ => {}
                }
                if t == "cell" {
                    self.open_paragraph(None);
                    self.inline(&e.children, &mut Vec::new());
                    self.end_paragraph();
                } else {
                    self.blocks(&e.children);
                }
            }
            _ => self.loose_inline(&Node::Element(e.clone())),
        }
    }

    /// Inline content outside any paragraph, in a paragraph that writes no
    /// marker back.
    fn loose_inline(&mut self, node: &Node) {
        if !self.loose {
            let id = self.style(styles::LOOSE, Kind::Paragraph);
            self.open_paragraph(Some(&id));
            self.loose = true;
        }
        self.inline(std::slice::from_ref(node), &mut Vec::new());
    }

    fn close_loose(&mut self) {
        if self.loose {
            self.end_paragraph();
            self.loose = false;
        }
    }

    fn open_paragraph(&mut self, style: Option<&str>) {
        self.last = None;
        let out = self.out();
        out.push_str("<w:p>");
        if let Some(id) = style {
            out.push_str(r#"<w:pPr><w:pStyle w:val=""#);
            out.push_str(id);
            out.push_str(r#""/></w:pPr>"#);
        }
    }

    fn end_paragraph(&mut self) {
        self.out().push_str("</w:p>\n");
    }

    fn paragraph_of_text(&mut self, marker: &str, text: &str) {
        let id = self.style(marker, Kind::Paragraph);
        self.open_paragraph(Some(&id));
        self.run(text, None);
        self.end_paragraph();
    }

    /// `\ca`/`\cp`/`\va`/`\vp`, which USX lifts onto the chapter or verse.
    fn lifted(&mut self, e: &Element, on: &str) {
        for (key, marker) in [("altnumber", "a"), ("pubnumber", "p")] {
            if e.attr(key).is_some() {
                self.report(format!("{on}{marker} on {on}"));
            }
        }
    }

    fn attributes(&mut self, e: &Element) {
        let marker = style_of(e).to_owned();
        let keys: Vec<String> =
            e.attrs.iter().map(|(k, _)| k).filter(|k| !CARRIED.contains(&k.as_str())).cloned().collect();
        for k in keys {
            self.report(format!("attribute {k} on \\{marker}"));
        }
    }

    /// A span is starting: if the run before it has the same style, a space
    /// keeps the two apart.
    fn separate(&mut self, style: &str) {
        if self.last.as_deref() == Some(style) {
            self.run(" ", None);
        }
    }

    fn inline(&mut self, nodes: &[Node], stack: &mut Vec<String>) {
        for node in nodes {
            let e = match node {
                Node::Text(text) => {
                    let style = (!stack.is_empty()).then(|| stack.join("+"));
                    self.run(text, style.as_deref());
                    continue;
                }
                Node::Element(e) => e,
            };
            match e.name.as_str() {
                "verse" if e.attr("eid").is_some() => {}
                "verse" => {
                    self.at = e.attr("sid").unwrap_or("").to_owned();
                    self.lifted(e, "\\v");
                    match e.attr("number").unwrap_or("") {
                        // `\v` with no number: a run cannot be empty, so the marker
                        // itself is kept, verbatim.
                        "" => self.run("\\v ", Some(styles::RAW)),
                        number => {
                            self.separate("v");
                            self.run(number, Some("v"));
                        }
                    }
                }
                // An empty span (`\ft \fqa x`): a run cannot be empty, so the
                // marker itself is kept, verbatim, at the top level of its
                // paragraph or note.
                "char" if e.children.is_empty() && stack.is_empty() => {
                    self.attributes(e);
                    let m = style_of(e);
                    let raw = if self.in_note && styles::NOTE_PEERS.contains(&m) {
                        format!("\\{m} ")
                    } else {
                        format!("\\{m} \\{m}*")
                    };
                    self.run(&raw, Some(styles::RAW));
                }
                "char" if e.children.is_empty() => {
                    self.report(format!("an empty \\{} inside another span, left out", style_of(e)));
                }
                "char" => {
                    self.attributes(e);
                    stack.push(style_of(e).to_owned());
                    self.separate(&stack.join("+"));
                    self.inline(&e.children, stack);
                    stack.pop();
                }
                "note" => self.note(e),
                "ms" => {
                    let marker = style_of(e);
                    if e.attrs.iter().all(|(k, _)| k == "style") {
                        self.run(&format!("\\{marker}\\*"), Some(styles::RAW));
                    } else {
                        self.report(format!("milestone \\{marker} with attributes, left out"));
                    }
                }
                "optbreak" => self.run("//", Some(styles::RAW)),
                "figure" => self.report("a figure (\\fig), left out".to_owned()),
                other => {
                    self.report(format!("{other} \\{}; its text is kept", style_of(e)));
                    self.inline(&e.children, stack);
                }
            }
        }
    }

    fn note(&mut self, e: &Element) {
        if self.in_note {
            self.report("a note inside a note, left out".to_owned());
            return;
        }
        let marker = style_of(e).to_owned();
        let caller = e.attr("caller").unwrap_or("+").to_owned();
        if e.attr("category").is_some() {
            self.report(format!("\\cat on \\{marker}"));
        }
        let id = self.next_note;
        self.next_note += 1;
        self.out().push_str(&format!(
            r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{id}"/></w:r>"#
        ));
        let style = self.style(&marker, Kind::Paragraph);
        let outer = self.last.take();
        self.in_note = true;
        self.out().push_str(&format!(
            r#"<w:footnote w:id="{id}"><w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>"#
        ));
        if caller != "+" {
            self.run(&caller, Some(styles::CALLER));
        }
        self.inline(&e.children, &mut Vec::new());
        self.out().push_str("</w:p></w:footnote>\n");
        self.in_note = false;
        self.last = outer;
    }

    fn run(&mut self, text: &str, style: Option<&str>) {
        if text.is_empty() {
            return;
        }
        if !text.chars().all(xml::allowed) {
            self.report("a control character XML cannot hold, left out".to_owned());
        }
        let id = style.map(|m| self.style(m, Kind::Character));
        self.last = style.map(str::to_owned);
        let out = self.out();
        out.push_str("<w:r>");
        if let Some(id) = id {
            out.push_str(r#"<w:rPr><w:rStyle w:val=""#);
            out.push_str(&id);
            out.push_str(r#""/></w:rPr>"#);
        }
        out.push_str(r#"<w:t xml:space="preserve">"#);
        out.push_str(&xml::text(text));
        out.push_str("</w:t></w:r>");
    }

    fn document(&self) -> String {
        let mut s = String::from(xml::DECLARATION);
        s.push_str(&format!(r#"<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>"#));
        s.push('\n');
        s.push_str(&self.body);
        s.push_str(r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#);
        s.push_str("</w:body></w:document>\n");
        s
    }

    fn footnotes(&self) -> String {
        let mut s = String::from(xml::DECLARATION);
        s.push_str(&format!(r#"<w:footnotes xmlns:w="{W}" xmlns:r="{R}">"#));
        s.push('\n');
        s.push_str(r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>"#);
        s.push('\n');
        s.push_str(r#"<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"#);
        s.push('\n');
        s.push_str(&self.notes);
        s.push_str("</w:footnotes>\n");
        s
    }

    fn styles(&self, options: &ExportOptions) -> String {
        let mut rpr = String::new();
        if let Some(font) = &options.font {
            let f = xml::attr(font);
            rpr.push_str(&format!(
                r#"<w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:eastAsia="{f}" w:cs="{f}"/>"#
            ));
        }
        if options.rtl {
            rpr.push_str("<w:rtl/>");
        }
        let mut s = STYLES.replacen(
            styles::RPR_DEFAULT,
            &format!("<w:rPrDefault><w:rPr>{rpr}</w:rPr></w:rPrDefault>"),
            1,
        );
        if options.rtl {
            s = s.replacen(
                styles::PPR_DEFAULT,
                "<w:pPrDefault><w:pPr><w:bidi/></w:pPr></w:pPrDefault>",
                1,
            );
        }
        let extra: String = self.extra.iter().map(|(m, k)| styles::style_xml(m, *k)).collect();
        s.replacen(styles::END, &format!("{extra}{}", styles::END), 1)
    }
}

struct Parts {
    document: String,
    footnotes: String,
    styles: String,
    custom: Option<String>,
    /// The main part's content type: a document or a template.
    main: &'static str,
}

impl Parts {
    fn zip(&self) -> Vec<u8> {
        let custom_type = if self.custom.is_some() {
            r#"<Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>"#
        } else {
            ""
        };
        let content_types = format!("{}{custom_type}</Types>\n", CONTENT_TYPES.replace("MAIN", self.main));
        let custom_rel = if self.custom.is_some() {
            r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/>"#
        } else {
            ""
        };
        let rels = format!("{RELS}{custom_rel}</Relationships>\n");
        let mut parts: Vec<(&str, &[u8])> = vec![
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("word/_rels/document.xml.rels", DOCUMENT_RELS.as_bytes()),
            ("word/document.xml", self.document.as_bytes()),
            ("word/styles.xml", self.styles.as_bytes()),
            ("word/footnotes.xml", self.footnotes.as_bytes()),
            ("word/settings.xml", SETTINGS.as_bytes()),
        ];
        if let Some(custom) = &self.custom {
            parts.push(("docProps/custom.xml", custom.as_bytes()));
        }
        crate::zip::write(&parts)
    }
}

const DOCUMENT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const TEMPLATE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml";

/// Closed by [`Parts::zip`], after the custom properties' override if any.
const CONTENT_TYPES: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Override PartName="/word/document.xml" ContentType="MAIN"/>"#,
    r#"<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#,
    r#"<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>"#,
    r#"<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>"#,
);

/// Closed by [`Parts::zip`], after the custom properties' relationship if any.
const RELS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
);

const DOCUMENT_RELS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#,
    r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>"#,
    "</Relationships>\n",
);

const SETTINGS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>"#,
    "</w:settings>\n",
);
