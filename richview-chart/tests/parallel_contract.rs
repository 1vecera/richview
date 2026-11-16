//! Parallel coordinates: an axis for each dimension scaled to its own range, a line for each row,
//! and the failures that name their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/parallel.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain(dims: &[&str]) -> Value {
    json!({"type":"parallel","title":"T","encoding":{"dimensions":dims}})
}

/// The points of every row's line, in source order.
fn polylines(g: &Geometry) -> Vec<Vec<[f64; 2]>> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => Some(
                path.cmds
                    .iter()
                    .filter_map(|c| match c {
                        Cmd::M { p } | Cmd::L { p } => Some(*p),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn axes(g: &Geometry) -> usize {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Line { role: Role::Grid, .. }))
        .count()
}

fn rows_of(values: &[[f64; 3]]) -> Vec<Value> {
    values
        .iter()
        .map(|v| json!({"a": v[0], "b": v[1], "c": v[2]}))
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn an_axis_for_each_dimension_and_a_line_for_each_row() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(axes(&g), 4);
    let lines = polylines(&g);
    assert_eq!(lines.len(), 9);
    assert!(lines.iter().all(|l| l.len() == 4));
    assert_eq!(observations(&g).len(), 9);
    // The axes stand at the same x for every row, left to right in the order written.
    for l in &lines {
        for (p, q) in l.iter().zip(&lines[0]) {
            assert!((p[0] - q[0]).abs() < 1e-9);
        }
        assert!(l.windows(2).all(|w| w[0][0] < w[1][0]));
    }
    assert!(g.legend.is_some());
}

#[test]
fn each_axis_runs_from_its_own_smallest_to_its_own_largest_value() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let lines = polylines(&g);
    let plot = g.panels[0].plot;
    // Power: GT (row 8) is the largest and Zip (row 1) the smallest. The axis ends are widened to
    // short decimals (75 reads 70), so the largest sits at the top and the smallest just above the foot.
    assert!((lines[8][1][1] - plot.y).abs() < 1e-6);
    assert!(lines.iter().all(|l| l[1][1] <= lines[1][1][1] + 1e-9));
    assert!(lines[1][1][1] < plot.bottom());
    // Weight: Sedan (row 5) is the largest and Zip (row 1) the smallest.
    assert!((lines[5][2][1] - plot.y).abs() < 1e-6);
    assert!((lines[1][2][1] - plot.bottom()).abs() < 1e-6);
    // Mileage: Zip is the largest, so the same row is at the top of one axis and the bottom of another.
    assert!((lines[1][0][1] - plot.y).abs() < 1e-6);
}

#[test]
fn the_ends_of_every_axis_are_labelled_with_their_values() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let labels: Vec<&str> = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisLabel => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    for want in ["17", "42", "70", "410", "980", "1,700", "3.9", "13.8"] {
        assert!(labels.contains(&want), "{want} labels an axis end: {labels:?}");
    }
}

#[test]
fn the_readout_lists_every_dimension_and_the_group() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g);
    assert_eq!(o[0].readout.heading, "Mini");
    assert_eq!(line(o[0], "Class"), Some("Small"));
    assert_eq!(line(o[0], "Mpg"), Some("38"));
    assert_eq!(line(o[0], "Power"), Some("85"));
    assert!(o[0].readout.lines.iter().all(|l| !l.derived));
    // Without a label the readout names the row, and without a colour there is no legend.
    let g = draw(&plain(&["a", "b", "c"]), &rows_of(&[[1.0, 2.0, 3.0], [2.0, 1.0, 5.0]]));
    assert_eq!(observations(&g)[1].readout.heading, "Row 2");
    assert!(g.legend.is_none());
}

#[test]
fn a_row_with_a_missing_value_is_left_out_and_disclosed() {
    let (spec, mut rows) = fixture();
    rows[0]["power"] = Value::Null;
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 1);
    assert_eq!(polylines(&g).len(), 8);
    assert!(g.failure.is_none());
}

#[test]
fn text_that_is_not_a_number_fails_with_its_row() {
    let (spec, mut rows) = fixture();
    rows[2]["weight"] = json!("heavy");
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("not a number"));
    let c = check(&g, "non-numeric").unwrap();
    assert_eq!(c.path.as_deref(), Some("rows[2].weight"));
}

#[test]
fn fewer_than_two_dimensions_say_what_is_needed() {
    let rows = rows_of(&[[1.0, 2.0, 3.0]]);
    for dims in [vec![], vec!["a"], vec!["a", "a"]] {
        let g = draw(&plain(&dims), &rows);
        assert!(g.failure.as_deref().unwrap().contains("at least two"), "{dims:?}");
        assert!(check(&g, "source-error").is_some());
    }
}

#[test]
fn above_eight_axes_the_chart_is_not_drawn_and_at_eight_it_is() {
    let make = |n: usize| -> (Vec<String>, Vec<Value>) {
        let names: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
        let rows = [1, 2]
            .map(|m| {
                Value::Object(
                    names
                        .iter()
                        .enumerate()
                        .map(|(i, k)| (k.clone(), json!(i * m)))
                        .collect(),
                )
            })
            .to_vec();
        (names, rows)
    };
    let (names, rows) = make(8);
    let dims: Vec<&str> = names.iter().map(String::as_str).collect();
    let g = draw(&plain(&dims), &rows);
    assert!(g.failure.is_none());
    assert_eq!(axes(&g), 8);
    assert!(check(&g, "limit-exceeded").is_none());

    let (names, rows) = make(10);
    let dims: Vec<&str> = names.iter().map(String::as_str).collect();
    let g = draw(&plain(&dims), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "axes");
    assert_eq!(c.details["count"], 10);
    assert_eq!(c.details["limit"], 8);
    assert!(check(&g, "excluded-observations").is_none());
}

#[test]
fn an_axis_without_spread_draws_in_the_middle() {
    let g = draw(
        &plain(&["a", "b", "c"]),
        &rows_of(&[[1.0, 5.0, 3.0], [2.0, 5.0, 9.0], [3.0, 5.0, 1.0]]),
    );
    let plot = g.panels[0].plot;
    for l in polylines(&g) {
        assert!((l[1][1] - (plot.y + plot.h / 2.0)).abs() < 1e-6);
    }
}

#[test]
fn facets_each_get_their_own_panel_on_the_same_ranges() {
    let mut spec = plain(&["a", "b", "c"]);
    spec["encoding"]["facet"] = json!("f");
    let mut rows = rows_of(&[[1.0, 2.0, 3.0], [4.0, 6.0, 1.0], [2.0, 9.0, 2.0], [3.0, 1.0, 8.0]]);
    for (i, r) in rows.iter_mut().enumerate() {
        r["f"] = json!(if i < 2 { "North" } else { "South" });
    }
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(polylines(&g).len(), 4);
    // The range of b is 1 to 9 in both panels: row 2 (9) tops the South panel, row 3 (1) is at
    // its foot, and row 1 (6) is well below the top of the North panel although it is the largest there.
    let lines = polylines(&g);
    assert!((lines[2][1][1] - g.panels[1].plot.y).abs() < 1e-6);
    assert!((lines[3][1][1] - g.panels[1].plot.bottom()).abs() < 1e-6);
    assert!(lines[1][1][1] > g.panels[0].plot.y + 1.0);
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    let huge = rows_of(&[[-1e300, 0.0, 1e300], [1e300, -1e300, 0.0], [0.0, 1e300, -1e300]]);
    let g = draw(&plain(&["a", "b", "c"]), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let g = draw_at(&spec, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    // Many axes and many rows at the narrowest width still draw.
    let names: Vec<String> = (0..8).map(|i| format!("Measure {i}")).collect();
    let dims: Vec<&str> = names.iter().map(String::as_str).collect();
    let dense: Vec<Value> = (0..300)
        .map(|r| {
            Value::Object(
                names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.clone(), json!((r * (i + 3)) % 17)))
                    .collect(),
            )
        })
        .collect();
    let g = draw_at(&plain(&dims), &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 300);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("parallel", &spec, &rows);
}

#[test]
fn the_runtime_writes_the_axis_ends_the_exact_rows_have() {
    // The smallest values sit a hair apart, far below the largest, so a grid step moves the ends.
    let rows = vec![
        json!({"a": 38.01, "b": 900.0, "c": 7.0}),
        json!({"a": 38.04, "b": 1000.0, "c": 6.5}),
        json!({"a": 42.0, "b": 100.04, "c": 9.0}),
        json!({"a": 1000.0, "b": 100.01, "c": 8.0}),
    ];
    runtime_matches_native(&plain(&["a", "b", "c"]), &rows);
}

#[test]
fn a_label_shared_with_a_dimension_is_named_once() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["label"] = json!("mpg");
    let g = draw(&spec, &rows);
    let o = observations(&g)[0];
    assert_eq!(o.readout.heading, "Mpg 38");
    assert_eq!(line(o, "Mpg"), None);
    assert_eq!(line(o, "Power"), Some("85"));
}
