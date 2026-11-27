//! Scatterplot matrices: a cell for every pair of fields, points below the diagonal, field names on
//! it, computed correlations above it, and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/splom.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain(dims: &[&str]) -> Value {
    json!({"type":"splom","title":"T","encoding":{"dimensions":dims}})
}

fn abc(values: &[[f64; 3]]) -> Vec<Value> {
    values
        .iter()
        .map(|v| json!({"a": v[0], "b": v[1], "c": v[2]}))
        .collect()
}

/// The centres of every point, in drawing order.
fn points(g: &Geometry) -> Vec<[f64; 2]> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Point {
                center,
                role: Role::Mark,
                ..
            } => Some(*center),
            _ => None,
        })
        .collect()
}

fn names(g: &Geometry) -> Vec<(String, [f64; 2])> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisTitle => Some((t.text.clone(), [t.x, t.y])),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn a_point_for_each_row_below_the_diagonal_and_a_cell_for_every_pair() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    // Four fields: six cells below the diagonal, each with twelve rows.
    assert_eq!(points(&g).len(), 72);
    // Four on the diagonal, six above it, and a point for each row below it.
    assert_eq!(observations(&g).len(), 4 + 6 + 72);
    let grid = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Line { role: Role::Grid, .. }))
        .count();
    assert_eq!(grid, 2 * 5);
    assert!(g.legend.is_some());
}

#[test]
fn the_diagonal_names_the_fields_in_the_order_written() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let names = names(&g);
    let text: Vec<&str> = names.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(text, ["bill", "flipper", "mass", "depth"]);
    assert!(names.windows(2).all(|w| w[0].1[0] < w[1].1[0] && w[0].1[1] < w[1].1[1]));
}

#[test]
fn every_cell_of_a_field_shares_its_range() {
    let g = draw(
        &plain(&["a", "b", "c"]),
        &abc(&[[1.0, 2.0, 5.0], [2.0, 4.0, 3.0], [3.0, 6.0, 1.0]]),
    );
    let p = points(&g);
    assert_eq!(p.len(), 9);
    // Cells in drawing order: (row 1, col 0), (row 2, col 0), (row 2, col 1). The first two share
    // column 0, so they place a at the same offset; the last two share row 2, so they place c alike.
    for k in 0..3 {
        assert!((p[k][0] - p[3 + k][0]).abs() < 1e-9);
        assert!((p[3 + k][1] - p[6 + k][1]).abs() < 1e-9);
    }
    // In the first cell b rises with a, so the points climb to the right.
    assert!(p[0][0] < p[1][0] && p[1][0] < p[2][0]);
    assert!(p[0][1] > p[1][1] && p[1][1] > p[2][1]);
}

#[test]
fn the_cells_above_the_diagonal_give_the_computed_correlation() {
    let g = draw(
        &plain(&["a", "b", "c"]),
        &abc(&[[1.0, 2.0, 3.0], [2.0, 4.0, 2.0], [3.0, 6.0, 1.0]]),
    );
    let o = observations(&g);
    let cell = |heading: &str| o.iter().find(|o| o.readout.heading == heading).unwrap();
    let up = cell("b and a");
    assert!(line(up, "Correlation").unwrap().starts_with('1'));
    assert!(
        up.readout
            .lines
            .iter()
            .find(|l| l.name == "Correlation")
            .unwrap()
            .derived
    );
    let down = cell("c and a");
    let text = line(down, "Correlation").unwrap();
    assert!(text.ends_with('1') && text.len() > 1, "{text}");
    assert_eq!(line(up, "Rows"), Some("3"));
    assert!(g.note.as_deref().unwrap_or_default().contains("correlation"));
}

#[test]
fn a_constant_field_has_no_correlation_and_draws_in_the_middle() {
    let g = draw(
        &plain(&["a", "b", "c"]),
        &abc(&[[1.0, 5.0, 3.0], [2.0, 5.0, 9.0], [3.0, 5.0, 1.0]]),
    );
    let o = observations(&g);
    assert_eq!(
        line(
            o.iter().find(|o| o.readout.heading == "b and a").unwrap(),
            "Correlation"
        ),
        Some("n/a")
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn the_readout_of_a_point_names_the_group_and_both_values() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let point = observations(&g).into_iter().find(|o| o.row == Some(0)).unwrap();
    assert_eq!(point.readout.heading, "Adelie");
    assert_eq!(line(point, "species"), None);
    assert!(line(point, "Bill").is_some() && line(point, "Flipper").is_some());
    assert!(point.readout.lines.iter().all(|l| !l.derived));
}

#[test]
fn above_six_dimensions_the_chart_is_not_drawn() {
    let names: Vec<String> = (0..7).map(|i| format!("f{i}")).collect();
    let dims: Vec<&str> = names.iter().map(String::as_str).collect();
    let make = |m: usize| -> Value {
        Value::Object(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), json!(i * m)))
                .collect(),
        )
    };
    let g = draw(&plain(&dims), &[make(1), make(2), make(3)]);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "dimensions");
    assert_eq!(c.details["count"], 7);
    assert_eq!(c.details["limit"], 6);
    assert!(points(&g).is_empty());
    assert!(check(&g, "excluded-observations").is_none());
    // Six dimensions are the limit and draw (see the edge fixtures).
    let six = &dims[..6];
    let g = draw(&plain(six), &[make(1), make(2), make(3)]);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
}

#[test]
fn fewer_than_two_fields_say_what_is_needed() {
    let rows = abc(&[[1.0, 2.0, 3.0]]);
    for dims in [vec![], vec!["a"], vec!["b", "b"]] {
        let g = draw(&plain(&dims), &rows);
        assert!(g.failure.as_deref().unwrap().contains("at least two"), "{dims:?}");
    }
}

#[test]
fn a_missing_value_is_disclosed_and_text_that_is_not_a_number_fails() {
    let (spec, mut rows) = fixture();
    rows[0]["mass"] = Value::Null;
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 1);
    assert_eq!(points(&g).len(), 66);
    rows[1]["flipper"] = json!("long");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert_eq!(
        check(&g, "non-numeric").unwrap().path.as_deref(),
        Some("rows[1].flipper")
    );
}

#[test]
fn facets_each_get_their_own_grid() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("species");
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 3);
    assert_eq!(points(&g).len(), 72);
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    contract(&spec, &rows[..2]);
    let huge = abc(&[[-1e300, 0.0, 1e300], [1e300, -1e300, 0.0], [0.0, 1e300, -1e300]]);
    let g = draw(&plain(&["a", "b", "c"]), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let g = draw_at(&spec, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let names: Vec<String> = (0..6).map(|i| format!("Field {i}")).collect();
    let dims: Vec<&str> = names.iter().map(String::as_str).collect();
    let dense: Vec<Value> = (0..400)
        .map(|r| {
            Value::Object(
                names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.clone(), json!((r * (i + 3)) % 23)))
                    .collect(),
            )
        })
        .collect();
    let g = draw_at(&plain(&dims), &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(points(&g).len(), 15 * 400);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("splom", &spec, &rows);
}

#[test]
fn a_strong_correlation_short_of_perfect_never_shows_as_one() {
    // y = x plus a small wobble: r is about 0.9998, which two digits would print as "1".
    let rows: Vec<Value> = (0..40)
        .map(|i| {
            let x = f64::from(i);
            let wobble = if i % 2 == 0 { 0.4 } else { -0.4 };
            json!({"a": x, "b": x + wobble, "c": -x - wobble})
        })
        .collect();
    let g = draw(&plain(&["a", "b", "c"]), &rows);
    let o = observations(&g);
    let cell = |heading: &str| o.iter().find(|o| o.readout.heading == heading).unwrap();
    let up = line(cell("b and a"), "Correlation").unwrap().to_owned();
    assert!(up.starts_with("0.99"), "{up}");
    let down = line(cell("c and a"), "Correlation").unwrap().to_owned();
    assert!(down.ends_with("0.99") || down.contains("0.99"), "{down}");
    assert!(down.starts_with('-') || down.starts_with('\u{2212}'), "{down}");
}

#[test]
fn the_runtime_writes_the_ranges_and_correlations_of_the_exact_rows() {
    // Values a grid step apart at the low end, so the ranges and the correlations move on the grid.
    let rows = abc(&[
        [100.01, 3.0, 40.0],
        [100.04, 5.0, 38.04],
        [250.0, 4.0, 90.0],
        [1000.0, 9.0, 38.01],
        [640.0, 7.5, 61.0],
    ]);
    runtime_matches_native(&plain(&["a", "b", "c"]), &rows);
}

#[test]
fn a_label_shared_with_a_dimension_is_named_once() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["label"] = json!("bill");
    let g = draw(&spec, &rows);
    let points: Vec<_> = observations(&g).into_iter().filter(|o| o.row == Some(0)).collect();
    assert!(!points.is_empty());
    for o in points {
        assert_eq!(o.readout.heading, "Bill 39");
        assert_eq!(line(o, "Bill"), None);
    }
}
