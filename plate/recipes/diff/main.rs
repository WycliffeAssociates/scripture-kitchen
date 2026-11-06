//! recipe-diff OLD.usfm NEW.usfm [side|redline] [OLD_LABEL NEW_LABEL] — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (old, new) = (std::fs::read_to_string(&args[0]).unwrap(), std::fs::read_to_string(&args[1]).unwrap());
    let layout = args.get(2).map_or("side", String::as_str);
    let l = args.get(3).map_or("Old", String::as_str);
    let r = args.get(4).map_or("New", String::as_str);
    let fonts = plate::world::fonts(&[]);
    plate::recipe::report(&format!("diff-{layout}"), recipe::run(&fonts, &old, &new, layout, (l, r), "{}"));
}
