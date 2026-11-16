//! Local profiling helper for an existing source file. No timing claim belongs in core itself.
use std::time::Instant;
fn main() {
    let path = std::env::args().nth(1).expect("profile SOURCE.rmd");
    let source = std::fs::read_to_string(&path).expect("source");
    let count = 10;
    let mut parse_ms = vec![];
    let mut render_ms = vec![];
    for _ in 0..count {
        let started = Instant::now();
        let parsed = richview_core::document::parse(&source).unwrap();
        parse_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(parsed);
        let started = Instant::now();
        let rendered = richview_core::document::render(&source, &Default::default()).unwrap();
        render_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(rendered);
    }
    parse_ms.sort_by(f64::total_cmp);
    render_ms.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"source":path,"iterations":count,"parse_median_ms":parse_ms[count/2],"render_median_ms":render_ms[count/2]})
    );
}
