//! The reference doors: a place spelled for machines (`parseReference`,
//! `formatReference`), for people (`Citations`, `showReference`), and the
//! place at a cursor (`Galley.addressAt`). The Rust is the `reference` crate
//! and onion's `Toc::address_at`; this is only their JS shapes.
//!
//! An address crosses as Sefer's `Address`: resolve's `Address` plus the
//! `book` it is in (`BookAddress`). Offsets in text cross in UTF-16, as a JS
//! string counts them.
//!
//! No locale's book names are linked here. A `Citations` reader holds the
//! book table's English names and the codes, plus the names the host hands
//! it: the project's own, and canon's TypeScript names for whichever
//! languages the page has loaded.

use js_sys::{Array, Object};
use reference::{Address, Assumption, BookKey, Catalogue, Grammar, Reference};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use onion_wasm::options::{bag, get};

/// A reference as JSON: resolve's address shape with its `book`.
pub(super) fn book_address(book: BookKey, address: &Address) -> Value {
    let mut out = onion_wasm::resolution::address_json(address);
    out["book"] = json!(book.to_string());
    out
}

fn reference_json(reference: &Reference) -> Value {
    book_address(reference.book, &reference.address)
}

/// A book code, case folded up: three letters or digits, at least one a
/// letter. Not only canon's: a `\\z` book a `Citations` reader was given
/// formats too.
fn book_code(value: &JsValue, door: &str) -> Result<BookKey, JsError> {
    value
        .as_string()
        .and_then(|code| <[u8; 3]>::try_from(code.to_ascii_uppercase().as_bytes()).ok())
        .filter(|code| {
            code.iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && code.iter().any(u8::is_ascii_uppercase)
        })
        .map(BookKey::new)
        .ok_or_else(|| {
            JsError::new(&format!(
                "{door}: a book is a three-letter code, like \"MAT\""
            ))
        })
}

/// A `BookAddress` from JS.
fn reference_from(value: &JsValue, door: &str) -> Result<Reference, JsError> {
    let book = get(Some(value), "book")
        .ok_or_else(|| JsError::new(&format!("{door}: an address needs its book")))?;
    let book = book_code(&book, door)?;
    let address = onion_wasm::resolution::address(value)
        .map_err(|e| JsError::new(&e.replacen("resolve", door, 1)))?;
    Ok(Reference { book, address })
}

fn utf16_at(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

/// The machine spelling (the Scripture Referencing Standard's basic
/// reference), read strictly: `{ ok: true, addresses }`, or `{ ok: false,
/// error, at }` with a stable error name and a UTF-16 offset. `book` reads a
/// list whose first reference leaves its book out (`"5:3-12"`).
#[wasm_bindgen(js_name = parseReference)]
pub fn parse_reference(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "{ book?: string }")] opts: Option<JsValue>,
) -> Result<String, JsError> {
    let door = "parseReference";
    let opts = bag(opts.as_ref(), door, &["book"]).map_err(|e| JsError::new(&e))?;
    let read = match get(opts.as_ref(), "book") {
        Some(book) => reference::parse_in(book_code(&book, door)?, text),
        None => reference::parse(text),
    };
    Ok(match read {
        Ok(refs) => json!({
            "ok": true,
            "addresses": refs.iter().map(reference_json).collect::<Vec<_>>(),
        }),
        Err(error) => json!({
            "ok": false,
            "error": error.kind.as_str(),
            "at": utf16_at(text, error.at),
        }),
    }
    .to_string())
}

/// The machine spelling of a list: `"MAT 1:20-2:3; MRK 1:1"`.
#[wasm_bindgen(js_name = formatReference)]
pub fn format_reference(
    #[wasm_bindgen(unchecked_param_type = "BookAddress[]")] refs: JsValue,
) -> Result<String, JsError> {
    let door = "formatReference";
    if !Array::is_array(&refs) {
        return Err(JsError::new(&format!(
            "{door}: expected an array of addresses"
        )));
    }
    let refs: Vec<Reference> = Array::from(&refs)
        .iter()
        .map(|one| reference_from(&one, door))
        .collect::<Result<_, _>>()?;
    Ok(reference::format(&refs))
}

/// What a person reads: `"Mateo 5:3-7"`. The book's name and the word for an
/// introduction are the caller's, in the project's language.
#[wasm_bindgen(js_name = showReference)]
pub fn show_reference(
    #[wasm_bindgen(unchecked_param_type = "BookAddress")] address: JsValue,
    book_name: &str,
    intro: &str,
) -> Result<String, JsError> {
    let reference = reference_from(&address, "showReference")?;
    Ok(reference::show(&reference, book_name, intro))
}

/// A citation reader: what a person types in a go-to box or a note, read
/// against the book names it was made with. Make one per project (and
/// language), then `read` on every keystroke.
#[wasm_bindgen]
pub struct Citations {
    catalogue: Catalogue,
}

#[wasm_bindgen]
impl Citations {
    /// `names`: per book code, what the project calls it in any language it
    /// names books in, and registered abbreviations (`{ MAT: ["Mateo", "Mt"] }`).
    /// `intro`: words for the introduction (`["introducción"]`). `books`: codes
    /// outside canon's table that should still be named (a `\z` book).
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_optional_param_type = "CitationNames")] opts: Option<JsValue>,
    ) -> Result<Citations, JsError> {
        let door = "Citations";
        let opts =
            bag(opts.as_ref(), door, &["names", "intro", "books"]).map_err(|e| JsError::new(&e))?;
        let mut catalogue = Catalogue::english();
        if let Some(books) = get(opts.as_ref(), "books") {
            for code in strings(&books, door, "books")? {
                let bytes: [u8; 3] =
                    code.to_ascii_uppercase()
                        .as_bytes()
                        .try_into()
                        .map_err(|_| {
                            JsError::new(&format!("{door}: {code:?} is not a three-letter code"))
                        })?;
                catalogue.book(BookKey::new(bytes));
            }
        }
        if let Some(names) = get(opts.as_ref(), "names") {
            if !names.is_object() || Array::is_array(&names) {
                return Err(JsError::new(&format!(
                    "{door}: names is an object of book codes to name lists"
                )));
            }
            for entry in Object::entries(names.unchecked_ref::<Object>()).iter() {
                let entry = Array::from(&entry);
                let code = entry.get(0).as_string().unwrap_or_default();
                let bytes: [u8; 3] = code
                    .to_ascii_uppercase()
                    .as_bytes()
                    .try_into()
                    .map_err(|_| JsError::new(&format!("{door}: {code:?} is not a book code")))?;
                for name in strings(&entry.get(1), door, "names")? {
                    catalogue.name(BookKey::new(bytes), &name);
                }
            }
        }
        if let Some(intro) = get(opts.as_ref(), "intro") {
            for word in strings(&intro, door, "intro")? {
                catalogue.intro_word(&word);
            }
        }
        Ok(Citations { catalogue })
    }

    /// `{ ok: true, text, addresses, assumed }` or `{ ok: false, text,
    /// problem, word? }`, as Sefer's `Citation`. `grammar` is `"navigation"`
    /// (the default: prefixes, `3,1` as `3:1`) or `"prose"`; `held` is the
    /// project's books, which win a prefix tie.
    pub fn read(
        &self,
        text: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "CiteOptions")] opts: Option<JsValue>,
    ) -> Result<String, JsError> {
        let door = "Citations.read";
        let opts = bag(opts.as_ref(), door, &["grammar", "held"]).map_err(|e| JsError::new(&e))?;
        let grammar = match get(opts.as_ref(), "grammar").and_then(|g| g.as_string()) {
            None => Grammar::Navigation,
            Some(g) if g == "navigation" => Grammar::Navigation,
            Some(g) if g == "prose" => Grammar::Prose,
            Some(other) => {
                return Err(JsError::new(&format!(
                    "{door}: grammar is \"navigation\" or \"prose\", not {other:?}"
                )));
            }
        };
        let held: Vec<BookKey> = match get(opts.as_ref(), "held") {
            None => Vec::new(),
            Some(held) => strings(&held, door, "held")?
                .iter()
                .filter_map(|code| {
                    <[u8; 3]>::try_from(code.to_ascii_uppercase().as_bytes())
                        .ok()
                        .map(BookKey::new)
                })
                .collect(),
        };
        let trimmed = text.trim();
        Ok(
            match reference::cite(text, &self.catalogue, grammar, &held) {
                Ok(cited) => json!({
                    "ok": true,
                    "text": trimmed,
                    "addresses": cited.refs.iter().map(reference_json).collect::<Vec<_>>(),
                    "assumed": cited.assumed.iter().map(|a| match a {
                        Assumption::Prefix { word, book } =>
                            json!({ "kind": "prefix", "word": word, "book": book.to_string() }),
                        Assumption::CommaIsColon => json!({ "kind": "comma" }),
                    }).collect::<Vec<_>>(),
                }),
                Err(error) => {
                    let mut out =
                        json!({ "ok": false, "text": trimmed, "problem": error.problem.as_str() });
                    if let Some(word) = error.word {
                        out["word"] = json!(word);
                    }
                    out
                }
            }
            .to_string(),
        )
    }

    /// Every book the words before the numbers could mean, in canon order:
    /// a filter under a go-to box. `"john 3"` lists JHN, 1JN, 2JN and 3JN.
    #[wasm_bindgen(js_name = booksMatching)]
    pub fn books_matching(&self, text: &str) -> Vec<String> {
        self.catalogue
            .books_matching(reference::citation_words(text))
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

/// An array of strings off an option.
fn strings(value: &JsValue, door: &str, key: &str) -> Result<Vec<String>, JsError> {
    let fail = || JsError::new(&format!("{door}: {key} must be an array of strings"));
    if !Array::is_array(value) {
        return Err(fail());
    }
    Array::from(value)
        .iter()
        .map(|item| item.as_string().ok_or_else(fail))
        .collect()
}
