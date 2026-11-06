//! recipe-study BOOK.usfm NOTES.tsv [sample|all] [SETTINGS_JSON] — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usfm = std::fs::read_to_string(&args[0]).unwrap();
    let tsv = std::fs::read_to_string(&args[1]).unwrap();
    let density = args.get(2).map_or("sample", String::as_str);
    let settings = args.get(3).cloned().unwrap_or_else(|| "{}".into());
    let fonts = plate::world::fonts(&[]);
    plate::recipe::report(&format!("study-{density}"), recipe::run(&fonts, &usfm, &tsv, density, &settings));
}
