//! The chart kernel benchmark (PRF-19): the three frozen time series of the docs examples, laid
//! out by the geometry engine at 390 and 1,280 px over 100 zoom windows, and nearest-observation
//! hover. Each window is laid out from the chart's preparation, as the runtime lays out a zoom
//! frame, and once more on its own (`oneShot`), as a new data state is. Run with
//! `cargo run --release -p richview-chart --example benchmark`; it prints JSON. Reading the CSV
//! belongs to core and is outside the timings, as are the painters, Wasm start and the DOM.
use richview_chart::block::ChartBlock;
use richview_chart::layout::{ChartTheme, Options, Prepared, layout, layout_prepared};
use richview_chart::{hit, vector};
use serde_json::{Map, Value, json};
use std::{fs, path::Path, time::Instant};

/// Each example's chart block as its `report.rmd` writes it (docs/public/examples).
fn blocks() -> [(&'static str, Value); 3] {
    [
        (
            "co2-mauna-loa-monthly",
            json!({"type": "line", "title": "The seasonal cycle sits on a long-term rise in CO₂",
                "encoding": {"x": "month", "y": "monthly_mean", "detail": ["note"]},
                "axes": {"y": {"label": "CO₂ mole fraction", "unit": "ppm", "digits": 5}}}),
        ),
        (
            "us-cpi-components",
            json!({"type": "line", "title": "US energy prices rise and fall sharply",
                "encoding": {"x": "month", "y": "energy"},
                "axes": {"y": {"label": "Energy price index", "unit": "1982–84 = 100", "digits": 6}}}),
        ),
        (
            "de-power-15min",
            json!({"type": "line", "title": "German day-ahead electricity prices cross below zero",
                "encoding": {"x": "time", "y": "price_eur_mwh"},
                "axes": {"y": {"label": "Day-ahead price", "unit": "EUR/MWh", "digits": 5}}}),
        ),
    ]
}

/// The CSV's rows as objects of text cells, as core hands them over; `#` lines are source notes.
fn rows(csv: &str) -> Vec<Value> {
    let mut records = csv.lines().filter(|line| !line.starts_with('#')).map(fields);
    let header = records.next().unwrap_or_default();
    records
        .map(|cells| {
            let row: Map<String, Value> = header
                .iter()
                .zip(cells)
                .map(|(name, cell)| (name.clone(), Value::String(cell)))
                .collect();
            Value::Object(row)
        })
        .collect()
}

/// One CSV line's fields; a quoted field may hold commas and doubled quotes.
fn fields(line: &str) -> Vec<String> {
    let (mut out, mut field, mut quoted) = (vec![], String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out
}

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).round() as usize]
}

fn peak_rss_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|s| s.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/fixtures/time-series");
    let theme = ChartTheme::default();
    let mut results = vec![];
    for (name, spec) in blocks() {
        let csv = fs::read_to_string(root.join(format!("{name}.csv"))).unwrap();
        let rows = rows(&csv);
        let block = ChartBlock::from_value(&spec).expect("a chart block");
        for width in [390.0, 1280.0] {
            let base = Options {
                width,
                ..Options::default()
            };
            let prepared = Prepared::default();
            let started = Instant::now();
            let whole = layout_prepared(&block, &rows, &theme, &base, &prepared);
            let first_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(whole.failure.is_none(), "{name}: {:?}", whole.failure);
            let [first, last] = whole.panels[0].x.as_ref().expect("an x scale").extent;
            let span = last - first;
            let (mut times, mut one_shot_times, mut hover_times) = (vec![], vec![], vec![]);
            let (mut max_observations, mut max_svg_bytes) = (0, 0);
            for frame in 0..110 {
                let zoom = if frame == 0 {
                    1.0
                } else {
                    0.08 + (frame % 19) as f64 / 19.0 * 0.92
                };
                let from = first + span * (1.0 - zoom) * (frame % 7) as f64 / 7.0;
                let options = Options {
                    zoom: (frame > 0).then_some([from, from + span * zoom]),
                    ..base.clone()
                };
                let started = Instant::now();
                let g = layout_prepared(&block, &rows, &theme, &options, &prepared);
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                max_observations = max_observations.max(g.panels.iter().map(|p| p.observations.len()).sum());
                max_svg_bytes = max_svg_bytes.max(vector::svg(&g, &theme).len());
                let plot = g.panels[0].plot;
                let started = Instant::now();
                std::hint::black_box(hit::at(&g, [plot.x + plot.w * 0.5, plot.y + plot.h * 0.5]));
                hover_times.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let fresh = layout(&block, &rows, &theme, &options);
                let one_shot = started.elapsed().as_secs_f64() * 1000.0;
                assert!(
                    fresh == g,
                    "{name}: a frame from the preparation equals its layout on its own"
                );
                if frame >= 10 {
                    times.push(elapsed);
                    one_shot_times.push(one_shot);
                }
            }
            results.push(
                json!({"fixture": name, "width": width, "inputCSVBytes": csv.len(), "rows": rows.len(),
                "firstLayoutMs": first_ms, "layoutMedianMs": percentile(&mut times, 0.5), "layoutP95Ms": percentile(&mut times, 0.95),
                "oneShotMedianMs": percentile(&mut one_shot_times, 0.5), "oneShotP95Ms": percentile(&mut one_shot_times, 0.95),
                "hoverMedianMs": percentile(&mut hover_times, 0.5), "maxObservations": max_observations,
                "maxSVGBytes": max_svg_bytes, "samples": 100}),
            );
        }
    }
    println!(
        "{}",
        json!({"scope": "geometry layout of the docs examples' chart blocks over 100 zoom windows from one preparation per width, each window also on its own (oneShot), and nearest-observation hover; excludes CSV reading, the painters, Wasm start and the DOM",
            "peakProcessRSSKiB": peak_rss_kib(), "results": results})
    );
}
