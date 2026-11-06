//! An in-memory Typst world: the template, one source per book, the settings
//! JSON, and a font store built once and shared between compiles.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
pub use typst_kit::fonts::FontStore;
use web_time::Instant;
use typst_layout::PagedDocument;

use crate::Book;

/// The template, as it sits beside this crate. Read on every compile, so an
/// edit shows in the next render without a rebuild.
pub fn template_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("typst/usfm.typ")
}

/// The template as built into the binary, for hosts with no file system.
pub const TEMPLATE: &str = include_str!("../typst/usfm.typ");

/// Typst's embedded fonts only: Libertinus Serif, New Computer Modern, DejaVu
/// Sans Mono. All the browser build has.
pub fn embedded_fonts() -> Arc<FontStore> {
    let mut store = FontStore::new();
    store.extend(typst_kit::fonts::embedded());
    Arc::new(store)
}

/// The families a store can set text in, sorted.
pub fn families(fonts: &FontStore) -> Vec<String> {
    let mut f: Vec<String> = fonts.book().families().map(|(name, _)| name.to_string()).collect();
    f.sort();
    f.dedup();
    f
}

/// Typst's embedded fonts (Libertinus Serif, New Computer Modern, DejaVu Sans
/// Mono), then every font under `dirs`, then the system's.
#[cfg(feature = "native")]
pub fn fonts(dirs: &[PathBuf]) -> Arc<FontStore> {
    let mut store = FontStore::new();
    store.extend(typst_kit::fonts::embedded());
    for dir in dirs {
        store.extend(typst_kit::fonts::scan(dir));
    }
    store.extend(typst_kit::fonts::system());
    Arc::new(store)
}

struct PlateWorld {
    library: LazyHash<Library>,
    fonts: Arc<FontStore>,
    main: FileId,
    sources: HashMap<FileId, Source>,
    files: HashMap<FileId, Bytes>,
}

fn id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("valid path")).intern()
}

impl World for PlateWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.sources
            .get(&id)
            .cloned()
            .ok_or_else(|| FileError::NotFound(PathBuf::from("?")))
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(bytes) = self.files.get(&id) {
            return Ok(bytes.clone());
        }
        self.sources
            .get(&id)
            .map(|s| Bytes::from_string(s.text().to_string()))
            .ok_or_else(|| FileError::NotFound(PathBuf::from("?")))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 1, 1)
    }
}

/// Where the time went, in milliseconds.
#[derive(Debug, Clone, Serialize)]
pub struct Timings {
    /// USFM → markup, all books (onion lex + CST + fold).
    pub markup_ms: f64,
    /// Typst evaluation and layout.
    pub layout_ms: f64,
    /// PDF export.
    pub pdf_ms: f64,
    pub pages: usize,
    pub markup_bytes: usize,
    pub warnings: usize,
}

/// Folds each USFM book to markup, then compiles with the template on disk.
/// `settings` is the JSON the template reads; `{}` takes every default.
#[cfg(feature = "native")]
pub fn render(
    fonts: &Arc<FontStore>,
    usfm: &[String],
    settings: &str,
) -> Result<(Vec<u8>, Timings), String> {
    let template = std::fs::read_to_string(template_path()).map_err(|e| e.to_string())?;
    render_with(fonts, usfm, settings, &template)
}

/// [`render`] with the template's text given.
pub fn render_with(
    fonts: &Arc<FontStore>,
    usfm: &[String],
    settings: &str,
    template: &str,
) -> Result<(Vec<u8>, Timings), String> {
    let t0 = Instant::now();
    let books: Vec<Book> = usfm.iter().map(|u| crate::markup(u)).collect();
    let markup_ms = ms(t0);
    compile(fonts, &books, settings, template, markup_ms)
}

pub fn compile(
    fonts: &Arc<FontStore>,
    books: &[Book],
    settings: &str,
    template: &str,
    markup_ms: f64,
) -> Result<(Vec<u8>, Timings), String> {
    let (includes, sources) = book_sources(books, "/usfm.typ");
    let doc = Doc {
        main: format!("#import \"/usfm.typ\" as u\n#show: u.setup\n{includes}"),
        sources,
        settings: settings.to_string(),
        markup_ms,
    };
    compile_doc(fonts, &doc, template)
}

/// A whole document for [`compile_doc`]: `main` is `/main.typ`, `sources` are
/// further files by absolute path, and the base template is always
/// `/usfm.typ`. A recipe adds its overlay as another source and imports that.
pub struct Doc {
    pub main: String,
    pub sources: Vec<(String, String)>,
    pub settings: String,
    pub markup_ms: f64,
}

/// One `/books/N.typ` per book, each importing `module` as `u`, and the
/// `#include` lines that pull them into a main file in order.
pub fn book_sources(books: &[Book], module: &str) -> (String, Vec<(String, String)>) {
    let mut includes = String::new();
    let mut sources = Vec::new();
    for (i, book) in books.iter().enumerate() {
        let path = format!("/books/{i}.typ");
        includes.push_str(&format!("#include \"{path}\"\n"));
        sources.push((path, format!("#import \"{module}\" as u\n{}", book.markup)));
    }
    (includes, sources)
}

pub fn compile_doc(
    fonts: &Arc<FontStore>,
    doc: &Doc,
    template: &str,
) -> Result<(Vec<u8>, Timings), String> {
    let mut sources = HashMap::new();
    for (path, text) in &doc.sources {
        let fid = id(path);
        sources.insert(fid, Source::new(fid, text.clone()));
    }
    let main_id = id("/main.typ");
    sources.insert(main_id, Source::new(main_id, doc.main.clone()));
    let template_id = id("/usfm.typ");
    sources.insert(template_id, Source::new(template_id, template.to_string()));
    let mut files = HashMap::new();
    files.insert(id("/settings.json"), Bytes::from_string(doc.settings.clone()));

    let world = PlateWorld {
        library: LazyHash::new(Library::default()),
        fonts: fonts.clone(),
        main: main_id,
        sources,
        files,
    };

    let t1 = Instant::now();
    let warned = typst::compile::<PagedDocument>(&world);
    let layout_ms = ms(t1);
    let warnings = warned.warnings.len();
    #[cfg(feature = "native")]
    for w in warned.warnings.iter().take(5) {
        eprintln!("warning: {}", describe(&world, w));
    }
    let document = warned.output.map_err(|errors| {
        errors
            .iter()
            .take(8)
            .map(|e| describe(&world, e))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let t2 = Instant::now();
    let pdf = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        errors
            .iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let pdf_ms = ms(t2);

    Ok((
        pdf,
        Timings {
            markup_ms: doc.markup_ms,
            layout_ms,
            pdf_ms,
            pages: document.pages().len(),
            markup_bytes: doc.sources.iter().map(|(_, t)| t.len()).sum(),
            warnings,
        },
    ))
}

fn describe(world: &PlateWorld, diag: &SourceDiagnostic) -> String {
    let mut s = diag.message.to_string();
    if let Some(fid) = diag.span.id()
        && let Some(source) = world.sources.get(&fid)
        && let Some(range) = typst::WorldExt::range(world, diag.span)
    {
        let lo = source.text()[..range.start]
            .char_indices()
            .rev()
            .nth(60)
            .map_or(0, |(i, _)| i);
        let hi = (range.end + 60).min(source.text().len());
        let hi = (hi..=source.text().len())
            .find(|&i| source.text().is_char_boundary(i))
            .unwrap_or(source.text().len());
        s.push_str(&format!(" … {:?}", &source.text()[lo..hi]));
    }
    for hint in &diag.hints {
        s.push_str(&format!("\n  hint: {hint:?}"));
    }
    s
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
