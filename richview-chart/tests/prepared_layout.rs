//! A zoom or resize frame laid out from its data state's preparation draws what a layout on its
//! own draws (PRF-19): every fixture chart and the docs time series, through a sequence of widths,
//! zoom windows inside, across and outside the data, a turned ratio and a hidden series, with one
//! preparation per hidden set.

use richview_chart::block::ChartBlock;
use richview_chart::layout::{ChartTheme, Options, Prepared, layout, layout_prepared};
use serde_json::{Map, Value, json};

fn csv(name: &str, rows: usize) -> Vec<Value> {
    let path = format!(
        "{}/../benchmarks/fixtures/time-series/{name}.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).expect("time series");
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().expect("header").split(',').collect();
    lines
        .take(rows)
        .map(|line| {
            let row: Map<String, Value> = header
                .iter()
                .zip(line.split(','))
                .map(|(name, cell)| (name.to_string(), json!(cell)))
                .collect();
            Value::Object(row)
        })
        .collect()
}

/// The type fixtures, and line, area, scatter and bar charts with what their painters read from
/// the rows: gaps, missing values, stacks, facets with their own scales, bubbles with labels and
/// details, and calendar, numeric and category x.
fn cases() -> Vec<(String, Value, Vec<Value>)> {
    let dir = format!("{}/tests/fixtures/types", env!("CARGO_MANIFEST_DIR"));
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("fixtures")
        .map(|e| e.expect("entry").path())
        .collect();
    paths.sort();
    let mut out: Vec<(String, Value, Vec<Value>)> = paths
        .iter()
        .map(|path| {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
            let name = path.file_stem().expect("stem").to_string_lossy().into_owned();
            (name, v["spec"].clone(), v["rows"].as_array().expect("rows").clone())
        })
        .collect();
    let cpi = csv("us-cpi-components", usize::MAX);
    let long: Vec<Value> = cpi
        .iter()
        .flat_map(|r| ["food", "energy", "shelter"].map(|c| json!({"month": r["month"], "series": c, "value": r[c]})))
        .collect();
    let gappy: Vec<Value> = (0..120)
        .flat_map(|i| {
            let day = format!("2024-{:02}-{:02}", i / 28 + 1, i % 28 + 1);
            let t = f64::from(i);
            [
                json!({"d": day, "s": "A", "v": if i % 17 == 5 { Value::Null } else { json!(100.0 + t.sin() * 9.0) }}),
                json!({"d": day, "s": "B", "v": 80.0 + (t * 0.3).cos() * 7.0}),
                json!({"d": day, "s": "C", "v": if i % 23 == 0 { json!("NA") } else { json!(60.0 - t * 0.05) }}),
            ]
        })
        .filter(|r| !(r["s"] == "B" && r["d"].as_str().is_some_and(|d| d.ends_with("-13"))))
        .collect();
    let bubbles: Vec<Value> = (0..60)
        .map(|i| {
            let x = f64::from(i);
            let group = ["a", "b", "c"][i as usize % 3];
            json!({"x": x * 1.3, "y": (x * 0.7).sin() * 30.0, "s": 1.0 + (x * 0.37).cos().abs() * 40.0,
                "c": group, "n": format!("P{i}"), "k": i * 7})
        })
        .collect();
    let regions = ["North", "South", "East", "West"];
    out.extend([
        (
            "power prices".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "time", "y": "price_eur_mwh"},
                "axes": {"y": {"label": "Day-ahead price", "unit": "EUR/MWh", "digits": 5}}}),
            csv("de-power-15min", 2000),
        ),
        (
            "co2 with notes".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "month", "y": "monthly_mean", "detail": ["note"]},
                "axes": {"y": {"unit": "ppm", "digits": 5}}}),
            csv("co2-mauna-loa-monthly", usize::MAX),
        ),
        (
            "co2 area".into(),
            json!({"type": "area", "title": "T", "encoding": {"x": "month", "y": "seasonally_adjusted"}}),
            csv("co2-mauna-loa-monthly", usize::MAX),
        ),
        (
            "cpi series".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "month", "y": "value", "color": "series"}}),
            long.clone(),
        ),
        (
            "cpi stack".into(),
            json!({"type": "area", "title": "T", "stack": true, "encoding": {"x": "month", "y": "value", "color": "series"}}),
            long.clone(),
        ),
        (
            "cpi facets".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "month", "y": "value", "facet": "series"},
                "style": {"free_scales": true}}),
            long,
        ),
        (
            "gaps".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "d", "y": "v", "color": "s"}}),
            gappy.clone(),
        ),
        (
            "gaps area".into(),
            json!({"type": "area", "title": "T", "encoding": {"x": "d", "y": "v", "color": "s"}}),
            gappy,
        ),
        (
            "numeric x".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "n", "y": "v", "color": "s"},
                "annotations": [{"kind": "line", "x": 40, "label": "Forty"}, {"kind": "point", "x": 80, "y": 3, "label": "Here"}]}),
            (0..120)
                .flat_map(|i| {
                    let t = f64::from(i);
                    [json!({"n": i, "v": (t * 0.2).sin() * 5.0, "s": "up"}), json!({"n": i, "v": (t * 0.1).cos() * 4.0, "s": "down"})]
                })
                .collect(),
        ),
        (
            "category x".into(),
            json!({"type": "line", "title": "T", "encoding": {"x": "q", "y": "v"}}),
            ["Q1", "Q2", "Q3", "Q4", "Q5"].iter().enumerate().map(|(i, q)| json!({"q": q, "v": i * 3})).collect(),
        ),
        (
            "bubbles".into(),
            json!({"type": "scatter", "title": "T",
                "encoding": {"x": "x", "y": "y", "size": "s", "color": "c", "label": "n", "detail": ["k"]}}),
            bubbles,
        ),
        (
            "bars".into(),
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "color": "g"}}),
            regions
                .iter()
                .flat_map(|c| [json!({"c": c, "g": "2024", "v": c.len() * 10}), json!({"c": c, "g": "2025", "v": c.len() * 12})])
                .collect(),
        ),
        (
            "stacked bars".into(),
            json!({"type": "bar", "title": "T", "stack": true, "encoding": {"x": "c", "y": "v", "color": "g"}}),
            regions
                .iter()
                .flat_map(|c| [json!({"c": c, "g": "2024", "v": c.len() * 10}), json!({"c": c, "g": "2025", "v": -(c.len() as i64) * 3})])
                .collect(),
        ),
    ]);
    out
}

#[test]
fn every_frame_from_a_preparation_equals_its_layout_on_its_own() {
    let theme = ChartTheme::default();
    for (name, spec, rows) in cases() {
        let block = ChartBlock::from_value(&spec).unwrap_or_else(|| panic!("{name}: a chart block"));
        let mut turned = block.clone();
        turned.style.ratio = Some(0.8);
        let first = layout(
            &block,
            &rows,
            &theme,
            &Options {
                width: 640.0,
                ..Options::default()
            },
        );
        let extent = first.panels.first().and_then(|p| p.x.as_ref()).map(|x| x.extent);
        let windows: Vec<Option<[f64; 2]>> = std::iter::once(None)
            .chain(extent.into_iter().flat_map(|[lo, hi]| {
                let span = hi - lo;
                [[0.25, 0.5], [0.0, 0.1], [0.9, 1.2], [-0.5, -0.1]].map(|[a, b]| Some([lo + span * a, lo + span * b]))
            }))
            .collect();
        let hidden_sets = [
            vec![],
            first.series.first().map(|s| s.key.clone()).into_iter().collect(),
        ];
        for hidden in hidden_sets {
            let prepared = Prepared::default();
            let frames = [320.0, 1280.0]
                .into_iter()
                .flat_map(|width| windows.iter().map(move |zoom| (width, *zoom, false)))
                .chain([
                    (640.0, None, true),
                    (640.0, windows.last().copied().flatten(), true),
                    (320.0, None, false),
                ]);
            for (width, zoom, ratio) in frames {
                let block = if ratio { &turned } else { &block };
                let opts = Options {
                    width,
                    zoom,
                    hidden: hidden.clone(),
                    ..Options::default()
                };
                assert!(
                    layout_prepared(block, &rows, &theme, &opts, &prepared) == layout(block, &rows, &theme, &opts),
                    "{name}, hidden {hidden:?}, {width} px, zoom {zoom:?}, ratio {ratio}: the frame from the preparation differs"
                );
            }
        }
    }
}
