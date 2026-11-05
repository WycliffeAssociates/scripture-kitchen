use std::time::Instant;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();
    let n = 20;
    let mut t = [0f64; 6];
    let mut docx_len = 0;
    for _ in 0..n {
        let a = Instant::now();
        let tokens = usfm_onion::lex(&src);
        let cst = usfm_onion::cst::build(&tokens);
        let b = Instant::now();
        let usx = usfm_onion::usx::usx(src.as_bytes(), &tokens, &cst);
        let c = Instant::now();
        let _ = std::hint::black_box(usx.len());
        let d = Instant::now();
        let out = docx::export(&src, &docx::ExportOptions::default());
        let e = Instant::now();
        let back = docx::import(&out.docx, &docx::Settings::default()).unwrap();
        let f = Instant::now();
        std::hint::black_box(back.books.len());
        docx_len = out.docx.len();
        t[0] += (b - a).as_secs_f64();
        t[1] += (c - b).as_secs_f64();
        t[2] += (e - d).as_secs_f64();
        t[3] += (f - e).as_secs_f64();
    }
    let ms = |x: f64| x / n as f64 * 1000.0;
    println!("{path}: {} KB USFM, {} KB docx", src.len() / 1024, docx_len / 1024);
    println!("  lex + cst        {:6.1} ms", ms(t[0]));
    println!("  usx fold         {:6.1} ms", ms(t[1]));
    println!("  export, whole    {:6.1} ms  (lex, cst, usx, parse, fold, styles, zip)", ms(t[2]));
    println!("  import, whole    {:6.1} ms  (unzip, parse, print)", ms(t[3]));
}
