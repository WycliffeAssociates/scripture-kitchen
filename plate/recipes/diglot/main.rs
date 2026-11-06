//! recipe-diglot LEFT.usfm RIGHT.usfm [LEFT_LABEL RIGHT_LABEL] — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (left, right) = (std::fs::read_to_string(&args[0]).unwrap(), std::fs::read_to_string(&args[1]).unwrap());
    let l = args.get(2).map_or("Left", String::as_str);
    let r = args.get(3).map_or("Right", String::as_str);
    let fonts = plate::world::fonts(&[]);
    plate::recipe::report("diglot", recipe::run(&fonts, &left, &right, (l, r), "{}"));
}
