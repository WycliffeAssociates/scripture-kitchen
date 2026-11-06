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

struct App {
    fonts: Arc<FontStore>,
    /// File name → USFM text.
    books: BTreeMap<String, String>,
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
    let app = Arc::new(App { fonts, books });
    let router = Router::new()
        .route("/", get(index))
        .route("/api/books", get(list_books))
        .route("/api/fonts", get(list_fonts))
        .route("/api/render", post(render))
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
    let result = tokio::task::spawn_blocking(move || {
        let r = plate::world::render(&fonts, &usfm, &settings);
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
