//! Heatmaps: one cell per (x, y) pair on a regular grid, colour that rises monotonically with the
//! value, a neutral cell for a missing value, one observation per cell, and the contract every
//! type shares (FEA-CH-75). Cell texts are in `heatmap_cell_values.rs` and `heatmap_contrast.rs`,
//! coordinates in `heatmap_coordinates.rs`.

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Rgba, Targeting};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"heatmap","title":"Deploys by weekday and hour (UTC)","encoding":{"x":"hour","y":"weekday","color":"deploys"}})
}

fn rows() -> Vec<Value> {
    let mut rows = Vec::new();
    for (d, day) in ["Mon", "Tue", "Wed"].iter().enumerate() {
        for (h, hour) in ["09", "12", "15"].iter().enumerate() {
            rows.push(json!({"weekday":day,"hour":hour,"deploys": 2 + 2 * d + h}));
        }
    }
    rows
}

/// A rough lightness: enough to tell a ramp that darkens from one that does not.
fn lightness(c: &Rgba) -> f64 {
    0.2126 * c.0[0] as f64 + 0.7152 * c.0[1] as f64 + 0.0722 * c.0[2] as f64
}

/// The fill of each painted data cell, in drawing order.
fn fills(g: &Geometry) -> Vec<Rgba> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { fill, .. } => Some(fill.color),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn cells_tile_a_regular_grid_and_each_reads_its_own_value() {
    let g = draw(&spec(), &rows());
    assert_eq!(g.panels[0].targeting, Targeting::Contain);
    let obs = observations(&g);
    assert_eq!(obs.len(), 9);
    assert_eq!(obs[0].readout.heading, "09 · Mon");
    assert_eq!(line(obs[0], "Deploys"), Some("2"));
    assert_eq!(line(obs[8], "Deploys"), Some("8"));
    let areas: Vec<_> = obs
        .iter()
        .map(|o| o.area.expect("a cell is hit by containment"))
        .collect();
    // One cell width and one cell height, rows and columns aligned.
    for a in &areas {
        assert!(
            (a.w - areas[0].w).abs() < 0.01 && (a.h - areas[0].h).abs() < 0.01,
            "{a:?}"
        );
    }
    for row in areas.chunks(3) {
        assert!(
            row.iter().all(|a| (a.y - row[0].y).abs() < 0.01),
            "a weekday is one row"
        );
        assert!(
            row[0].x < row[1].x && row[1].x < row[2].x,
            "hours run left to right in the author's order"
        );
    }
    assert!(
        areas[0].y < areas[3].y && areas[3].y < areas[6].y,
        "weekdays run top to bottom in the author's order"
    );
    // Neighbours touch without overlapping.
    assert!((areas[0].right() - areas[1].x).abs() < 0.01 && (areas[0].bottom() - areas[3].y).abs() < 0.01);
}

#[test]
fn colour_follows_the_value_in_one_direction_and_equal_values_share_a_colour() {
    let rows = vec![
        json!({"weekday":"A","hour":"1","deploys":1}),
        json!({"weekday":"A","hour":"2","deploys":3}),
        json!({"weekday":"A","hour":"3","deploys":3}),
        json!({"weekday":"A","hour":"4","deploys":6}),
        json!({"weekday":"A","hour":"5","deploys":10}),
    ];
    let g = draw(&spec(), &rows);
    let l: Vec<f64> = fills(&g).iter().map(lightness).collect();
    assert_eq!(l.len(), 5);
    assert!((l[1] - l[2]).abs() < 1e-6, "equal values, equal colour: {l:?}");
    let darkens = l[4] < l[0];
    for pair in [(0, 1), (2, 3), (3, 4)] {
        let (a, b) = (l[pair.0], l[pair.1]);
        assert!(if darkens { b < a } else { b > a }, "{l:?}: the ramp runs one way");
    }
}

#[test]
fn a_missing_value_is_a_neutral_cell_that_stays_readable() {
    let mut rows = rows();
    rows[4] = json!({"weekday":"Tue","hour":"12","deploys":null});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 9, "the cell keeps its readout");
    assert_eq!(line(observations(&g)[4], "Deploys"), Some("No observation"));
    assert_eq!(fills(&g).len(), 8, "its fill is not a point on the value ramp");
}

#[test]
fn two_values_for_one_cell_are_a_source_error_and_text_is_not_a_value() {
    let mut rows = rows();
    rows.push(json!({"weekday":"Mon","hour":"09","deploys":99}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "ambiguous-numeric-cell").is_some(), "{:?}", g.checks);
    rows.pop();
    rows[0] = json!({"weekday":"Mon","hour":"09","deploys":"lots"});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "invalid-color-values").is_some(), "{:?}", g.checks);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"weekday":"Mon","hour":"09","deploys":5})]);
    // Every cell equal: one colour, no division by an empty range.
    let flat: Vec<Value> = rows()
        .into_iter()
        .map(|mut r| {
            r["deploys"] = json!(7);
            r
        })
        .collect();
    contract(&spec(), &flat);
    let l: Vec<f64> = fills(&draw(&spec(), &flat)).iter().map(lightness).collect();
    assert!(l.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-6), "{l:?}");
    // Signed and huge values stay finite.
    contract(
        &spec(),
        &[
            json!({"weekday":"Mon","hour":"09","deploys":-20}),
            json!({"weekday":"Mon","hour":"12","deploys":0}),
            json!({"weekday":"Tue","hour":"09","deploys":20}),
        ],
    );
    let g = draw(
        &spec(),
        &[
            json!({"weekday":"Mon","hour":"09","deploys":1e300}),
            json!({"weekday":"Mon","hour":"12","deploys":-5e299}),
        ],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn a_large_grid_keeps_every_cell_reachable() {
    let rows: Vec<Value> = (0..30)
        .flat_map(|y| {
            (0..30).map(
                move |x| json!({"weekday": format!("Row {y}"), "hour": format!("Col {x}"), "deploys": (x * y) % 17}),
            )
        })
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 900);
    assert_eq!(richview_chart::hit::order(&g).len(), 900);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("heatmap", &spec(), &rows());
}

#[test]
fn the_runtime_keeps_values_a_grid_step_apart() {
    let rows = vec![
        json!({"weekday":"Mon","hour":"09","deploys":100.01}),
        json!({"weekday":"Mon","hour":"12","deploys":1000}),
        json!({"weekday":"Tue","hour":"09","deploys":100.04}),
        json!({"weekday":"Tue","hour":"12","deploys":0.5}),
    ];
    runtime_matches_native(&spec(), &rows);
}
