//! recipe-links BOOK.usfm… — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let usfm: Vec<String> = std::env::args().skip(1).map(|p| std::fs::read_to_string(p).unwrap()).collect();
    let fonts = plate::world::fonts(&[]);
    plate::recipe::report("links", recipe::run(&fonts, &usfm, "{}"));
}
