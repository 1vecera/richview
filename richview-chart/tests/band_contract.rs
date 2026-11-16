//! Band charts: authored bounds over a calendar or numeric x axis, gaps, groups, and the shared
//! contract every type meets (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/band.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

#[test]
fn the_golden_series_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn bounds_and_centre_are_read_off_the_same_axis() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), rows.len());
    let first = obs[0];
    let number = |name: &str| {
        line(first, name)
            .unwrap()
            .trim_end_matches("°C")
            .parse::<f64>()
            .unwrap()
    };
    assert_eq!(number("Upper"), rows[0]["high"].as_f64().unwrap());
    assert_eq!(number("Lower"), rows[0]["low"].as_f64().unwrap());
    assert!(first.readout.heading.contains("2025"), "{}", first.readout.heading);
    // The dot sits on the centre line: lower than the upper bound's pixel row, above the lower.
    let plot = g.panels[0].plot;
    assert!(first.at[1] > plot.y && first.at[1] < plot.bottom());
    // A band item exists, and its edges are drawn as marks.
    let bands = g.panels[0].items.iter().filter(|i| i.role() == Role::Band).count();
    assert_eq!(bands, 1);
}

#[test]
fn a_missing_stretch_breaks_the_band() {
    let (spec, mut rows) = fixture();
    for r in rows.iter_mut().skip(8).take(3) {
        r["low"] = Value::Null;
        r["high"] = Value::Null;
    }
    // Three days missing is more than one and a half steps: two bands.
    let g = draw(&spec, &rows);
    let bands = g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Area { role: Role::Band, .. }))
        .count();
    assert_eq!(bands, 2);
    assert!(check(&g, "excluded-observations").is_some());
}

#[test]
fn reversed_bounds_fail_the_panel_and_name_the_row() {
    let (spec, mut rows) = fixture();
    rows[3]["low"] = json!(99);
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("Row 4"));
}

#[test]
fn numeric_x_axes_and_groups_work() {
    let spec = json!({"type":"band","title":"T","encoding":{"x":"n","lower":"lo","upper":"hi","color":"g"}});
    let rows: Vec<Value> = (0..12)
        .flat_map(|i| {
            [
                json!({"n": i, "lo": i as f64, "hi": i as f64 + 2.0, "g": "A"}),
                json!({"n": i, "lo": i as f64 + 1.0, "hi": i as f64 + 4.0, "g": "B"}),
            ]
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 2);
    assert_eq!(observations(&g).len(), 12);
    assert_eq!(observations(&g)[0].readout.lines.len(), 4);
}

#[test]
fn x_that_is_neither_dates_nor_numbers_says_why() {
    let spec = json!({"type":"band","title":"T","encoding":{"x":"x","lower":"lo","upper":"hi"}});
    let g = draw(&spec, &[json!({"x":"north","lo":1,"hi":2})]);
    assert!(g.failure.as_deref().unwrap().contains("dates or numbers"));
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    let flat: Vec<Value> = (0..5)
        .map(|i| json!({"day": format!("2025-03-0{}", i + 1), "low": 3, "high": 3, "mean": 3}))
        .collect();
    contract(&spec, &flat);
    let g = draw(
        &spec,
        &[
            json!({"day":"2025-03-01","low":-1e300,"high":1e300,"mean":0}),
            json!({"day":"2025-03-02","low":-1e300,"high":1e300,"mean":0}),
        ],
    );
    assert!(nonfinite(&g).is_none());
    let dense: Vec<Value> = (0..2000).map(|i| json!({"day": format!("{}", 1000 + i), "low": (i % 17) as f64, "high": (i % 17) as f64 + 5.0, "mean": (i % 17) as f64 + 2.0})).collect();
    let g = draw_at(&spec, &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn zoom_narrows_the_window() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let x = g.panels[0].x.clone().unwrap();
    assert!(x.calendar);
    let mid = (x.extent[0] + x.extent[1]) / 2.0;
    let block = richview_chart::block::ChartBlock::from_value(&spec).unwrap();
    let opts = richview_chart::layout::Options {
        width: 640.0,
        zoom: Some([x.extent[0], mid]),
        ..Default::default()
    };
    let z = richview_chart::layout::layout(&block, &rows, &richview_chart::layout::ChartTheme::default(), &opts);
    assert!(z.panels[0].x.clone().unwrap().domain[1] <= mid + 1.0);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("band", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_axis_of_bounds_a_grid_step_apart() {
    let spec = json!({"type":"band","title":"T","encoding":{"x":"n","lower":"lo","upper":"hi"}});
    let rows = vec![
        json!({"n": 1, "lo": 100.01, "hi": 100.04}),
        json!({"n": 2, "lo": 90.0, "hi": 1000.0}),
        json!({"n": 3, "lo": 0.5, "hi": 640.0}),
    ];
    runtime_matches_native(&spec, &rows);
}
