//! `plate-serve --corpus DIR [--host 127.0.0.1] [--port 7878]`
//!
//! A local print studio: the page at `/` posts settings and a book list to
//! `/api/render` and shows the PDF that comes back. The template and the page
//! are read from disk on every request, so editing either shows on the next
//! render. `/api/markup` and `/api/template` serve what a browser-side Typst
//! compile would need.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use typst_kit::fonts::FontStore;

// The recipes, compiled straight from their folders (they stay out of the
// library: examples, not product).
#[path = "../../recipes/diff/recipe.rs"]
mod diff;
#[path = "../../recipes/diglot/recipe.rs"]
mod diglot;
#[path = "../../recipes/links/recipe.rs"]
mod links;
#[path = "../../recipes/notes/recipe.rs"]
mod notes;
#[path = "../../recipes/study/recipe.rs"]
mod study;

struct App {
    fonts: Arc<FontStore>,
    /// File name → USFM text.
    books: BTreeMap<String, String>,
    /// Book code → text, for the recipes' second texts.
    ulb: BTreeMap<String, String>,
    ult: BTreeMap<String, String>,
}

/// Every .usfm under `dir`, keyed by its `\id` code.
fn by_code(dir: &std::path::Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return map };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("usfm"))
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            map.insert(plate::markup(&text).code, text);
        }
    }
    map
}

#[derive(Serialize)]
struct BookInfo {
    file: String,
    code: String,
    name: String,
}

#[derive(Deserialize)]
struct RenderRequest {
    books: Vec<String>,
    settings: serde_json::Value,
    #[serde(default)]
    recipe: Option<String>,
}

#[derive(Deserialize)]
struct BookQuery {
    book: String,
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (mut corpus, mut port, mut host) = (None::<PathBuf>, 7878u16, "127.0.0.1".to_string());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--corpus" => corpus = Some(args.next().unwrap().into()),
            "--port" => port = args.next().unwrap().parse().unwrap(),
            "--host" => host = args.next().unwrap(),
            other => panic!("unknown argument {other}"),
        }
    }
    let corpus = corpus.expect("--corpus DIR");
    let mut books = BTreeMap::new();
    for entry in std::fs::read_dir(&corpus).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("usfm") || ext.eq_ignore_ascii_case("sfm") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            books.insert(name, std::fs::read_to_string(&path).unwrap());
        }
    }
    let fonts = plate::world::fonts(&[PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts"))]);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../testData");
    let ulb = by_code(&root.join("exampleCorpora/en_ulb"));
    let ult = by_code(&root.join("stressCorpora/en_ult"));
    let app = Arc::new(App { fonts, books, ulb, ult });
    let router = Router::new()
        .route("/", get(index))
        .route("/api/books", get(list_books))
        .route("/api/fonts", get(list_fonts))
        .route("/api/render", post(render).get(render_get))
        .route("/api/markup", get(markup))
        .route("/api/template", get(template))
        .route("/api/usfm", get(usfm))
        .nest_service(
            "/wasm",
            tower_http::services::ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/wasm/pkg")),
        )
        .with_state(app);
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await.unwrap();
    eprintln!("plate studio on http://{host}:{port}");
    axum::serve(listener, router).await.unwrap();
}

async fn index() -> Html<String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/web/index.html");
    Html(std::fs::read_to_string(path).unwrap_or_else(|e| e.to_string()))
}

async fn list_books(State(app): State<Arc<App>>) -> Json<Vec<BookInfo>> {
    Json(
        app.books
            .iter()
            .map(|(file, usfm)| {
                let b = plate::markup(usfm);
                BookInfo { file: file.clone(), code: b.code, name: b.name }
            })
            .collect(),
    )
}

async fn list_fonts(State(app): State<Arc<App>>) -> Json<Vec<String>> {
    let mut families: Vec<String> = app.fonts.book().families().map(|(f, _)| f.to_string()).collect();
    families.sort();
    families.dedup();
    Json(families)
}

#[derive(Deserialize)]
struct RenderQuery {
    /// Comma-separated file names.
    books: String,
    /// The settings JSON.
    settings: String,
    #[serde(default)]
    recipe: Option<String>,
}

/// The GET spelling of `/api/render`, for networks whose web filter stops a
/// POST to an uncategorized host (Sophos does).
async fn render_get(State(app): State<Arc<App>>, Query(q): Query<RenderQuery>) -> Response {
    let settings = match serde_json::from_str(&q.settings) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let books = q.books.split(',').filter(|b| !b.is_empty()).map(str::to_string).collect();
    render(State(app), Json(RenderRequest { books, settings, recipe: q.recipe })).await
}

async fn render(State(app): State<Arc<App>>, Json(req): Json<RenderRequest>) -> Response {
    let usfm: Vec<String> = req
        .books
        .iter()
        .filter_map(|b| app.books.get(b).cloned())
        .collect();
    if usfm.is_empty() {
        return (StatusCode::BAD_REQUEST, "no books").into_response();
    }
    let settings = req.settings.to_string();
    let fonts = app.fonts.clone();
    let recipe = req.recipe.unwrap_or_default();
    let code = plate::markup(&usfm[0]).code;
    let app2 = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        let need = |map: &BTreeMap<String, String>, what: &str| {
            map.get(&code).cloned().ok_or_else(|| format!("no {what} text for {code} in testData"))
        };
        let r = match recipe.as_str() {
            "links" => links::run(&fonts, &usfm, &settings),
            "notes" | "study" | "study-all" => {
                let tsv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("data/tn_{code}.tsv"));
                match std::fs::read_to_string(&tsv_path) {
                    Ok(tsv) => {
                        let text = app2.ult.get(&code).cloned().unwrap_or_else(|| usfm[0].clone());
                        match recipe.as_str() {
                            "notes" => notes::run(&fonts, &text, &tsv, &settings),
                            "study" => study::run(&fonts, &text, &tsv, "sample", &settings),
                            _ => study::run(&fonts, &text, &tsv, "all", &settings),
                        }
                    }
                    Err(_) => Err(format!("no notes for {code}: fetch {} (only Mark is here)", tsv_path.display())),
                }
            }
            "diglot" => need(&app2.ulb, "ULB").and_then(|right| {
                diglot::run(&fonts, &usfm[0], &right, ("Berean Standard Bible", "Unlocked Literal Bible"), &settings)
            }),
            "diff-side" | "diff-redline" => need(&app2.ult, "ULT").and_then(|old| {
                let new = need(&app2.ulb, "ULB")?;
                diff::run(&fonts, &old, &new, recipe.trim_start_matches("diff-"), ("ULT", "ULB"), &settings)
            }),
            _ => plate::world::render(&fonts, &usfm, &settings),
        };
        // Keep the incremental cache for recent compiles only.
        comemo::evict(10);
        r
    })
    .await
    .unwrap();
    match result {
        Ok((pdf, timings)) => Response::builder()
            .header(header::CONTENT_TYPE, "application/pdf")
            .header("x-plate-timings", serde_json::to_string(&timings).unwrap())
            .body(Body::from(pdf))
            .unwrap(),
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
    }
}

async fn markup(State(app): State<Arc<App>>, Query(q): Query<BookQuery>) -> Response {
    match app.books.get(&q.book) {
        Some(usfm) => plate::markup(usfm).markup.into_response(),
        None => (StatusCode::NOT_FOUND, "no such book").into_response(),
    }
}

async fn usfm(State(app): State<Arc<App>>, Query(q): Query<BookQuery>) -> Response {
    match app.books.get(&q.book) {
        Some(usfm) => usfm.clone().into_response(),
        None => (StatusCode::NOT_FOUND, "no such book").into_response(),
    }
}

async fn template() -> Response {
    std::fs::read_to_string(plate::world::template_path())
        .unwrap_or_else(|e| e.to_string())
        .into_response()
}
