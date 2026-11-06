//! recipe-notes BOOK.usfm NOTES.tsv [SETTINGS_JSON] — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usfm = std::fs::read_to_string(&args[0]).unwrap();
    let tsv = std::fs::read_to_string(&args[1]).unwrap();
    let settings = args.get(2).cloned().unwrap_or_else(|| r#"{"size":10.5}"#.into());
    let fonts = plate::world::fonts(&[]);
    plate::recipe::report("notes", recipe::run(&fonts, &usfm, &tsv, &settings));
}
