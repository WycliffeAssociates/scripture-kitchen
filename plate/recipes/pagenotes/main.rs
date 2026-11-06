//! recipe-pagenotes BOOK.usfm NOTES.tsv [SETTINGS_JSON] — see recipe.rs.

#[path = "recipe.rs"]
mod recipe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usfm = std::fs::read_to_string(&args[0]).unwrap();
    let tsv = std::fs::read_to_string(&args[1]).unwrap();
    let settings = args.get(2).cloned().unwrap_or_else(|| "{}".into());
    let fonts = plate::world::fonts(&[]);
    let result = recipe::run(&fonts, &usfm, &tsv, &settings).map(|(pdf, t, r)| {
        eprintln!(
            "{} passes ({}), {} of {} notes on their verse's page; pass times {:?} ms",
            r.passes,
            if r.converged { "settled" } else { "stopped at the limit" },
            r.on_own_page,
            r.notes,
            r.pass_ms.iter().map(|m| m.round() as u64).collect::<Vec<_>>()
        );
        (pdf, t)
    });
    plate::recipe::report("pagenotes", result);
}
