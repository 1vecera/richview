//! Contour charts: iso-lines at round levels over a grid, interpolation of scattered rows,
//! a key that decodes the colour, readouts at the nearest grid node and the contract every type
//! shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::hit;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/contour.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"contour","title":"T","encoding":{"x":"a","y":"b","z":"c"},"axes":{"color":{"label":"C"}}})
}

fn triples(ps: &[(f64, f64, f64)]) -> Vec<Value> {
    ps.iter().map(|(a, b, c)| json!({"a": a, "b": b, "c": c})).collect()
}

/// A complete grid of `nx` by `ny` rows with a single peak.
fn grid(nx: usize, ny: usize) -> Vec<Value> {
    let mut rows = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let d = ((i as f64 - 3.0).powi(2) + (j as f64 - 3.0).powi(2)).sqrt();
            rows.push(json!({"a": i as f64, "b": j as f64, "c": 20.0 - d * 3.0}));
        }
    }
    rows
}

/// Scattered rows on a bumpy surface, none on a grid.
fn scattered(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            let (a, b) = ((i * 37 % 101) as f64 / 10.0, (i * 11 % 53) as f64 / 5.0);
            json!({"a": a, "b": b, "c": (a * 0.7).sin() * 5.0 + b})
        })
        .collect()
}

fn lines(g: &Geometry) -> Vec<&Item> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Line { role: Role::Mark, .. }))
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("contour", &spec, &rows);
}

#[test]
fn a_grid_draws_lines_in_the_ramp_colours_and_a_key_decodes_them() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert!(lines(&g).len() >= 6, "{} lines", lines(&g).len());
    let colours: std::collections::HashSet<String> = lines(&g)
        .iter()
        .filter_map(|i| match i {
            Item::Line { stroke, .. } => Some(stroke.color.hex()),
            _ => None,
        })
        .collect();
    assert!(colours.len() >= 4, "{colours:?}");
    assert!(g.figure_items.iter().any(|i| i.role() == Role::Key));
    let ends: Vec<String> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert!(ends.len() >= 2, "{ends:?}");
}

#[test]
fn a_grid_reads_back_the_rows_it_was_given() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let obs = observations(&g);
    assert!(!obs.is_empty());
    for o in &obs {
        assert!(o.readout.heading.starts_with("Near "));
        assert_eq!(o.readout.lines.len(), 1);
        assert!(o.readout.lines.iter().all(|l| !l.derived && !l.value.starts_with("≈ ")));
        let row = &rows[o.row.expect("a grid node is a row")];
        let want = row["height"].as_f64().unwrap();
        let got: f64 = line(o, "Height (m)").unwrap().replace(',', "").parse().unwrap();
        assert!((got - want).abs() < want.abs() * 0.01 + 0.06, "{got} vs {want}");
    }
}

#[test]
fn readout_headings_are_independent_of_plot_size() {
    let rows = grid(11, 3);
    let headings = |width| {
        let g = draw_at(&spec(), &rows, width);
        observations(&g)
            .iter()
            .map(|o| o.readout.heading.clone())
            .collect::<Vec<_>>()
    };
    let narrow = headings(320.0);
    assert!(!narrow.is_empty());
    assert_eq!(narrow, headings(1440.0));
}

#[test]
fn the_pointer_reads_the_nearest_node_anywhere_in_the_plot() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let plot = g.panels[0].plot;
    for (fx, fy) in [(0.1, 0.1), (0.5, 0.5), (0.9, 0.9), (0.3, 0.8)] {
        let p = [plot.x + plot.w * fx, plot.y + plot.h * fy];
        assert!(hit::at(&g, p).is_some(), "nothing at {p:?}");
    }
}

#[test]
fn scattered_rows_are_interpolated_and_say_so() {
    let rows = scattered(60);
    contract(&spec(), &rows);
    let g = draw(&spec(), &rows);
    assert!(!lines(&g).is_empty());
    assert!(observations(&g).iter().all(|o| o.row.is_none()));
    assert!(
        observations(&g)
            .iter()
            .all(|o| o.readout.lines.iter().all(|l| l.derived))
    );
    assert!(
        observations(&g)
            .iter()
            .all(|o| o.readout.lines[0].value.starts_with("≈ "))
    );
    assert!(g.note.as_deref().unwrap_or_default().contains("interpolated"));
    assert!(g.items().any(|i| i.role() == Role::Rug), "the rows are marked");
}

#[test]
fn a_cone_draws_a_ring_for_each_level_it_reaches() {
    let g = draw(&spec(), &grid(9, 9));
    assert!(g.failure.is_none());
    assert!(lines(&g).len() >= 3);
    contract(&spec(), &grid(9, 9));
}

#[test]
fn a_constant_surface_has_no_lines_and_says_why() {
    let rows = triples(&[(0.0, 0.0, 5.0), (1.0, 0.0, 5.0), (0.0, 1.0, 5.0), (1.0, 1.0, 5.0)]);
    let g = draw(&spec(), &rows);
    assert!(g.failure.as_deref().unwrap_or_default().contains("single value"));
    assert!(!g.panels.is_empty());
}

#[test]
fn too_few_rows_or_a_single_coordinate_fail_with_a_reason() {
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &triples(&[(1.0, 1.0, 1.0)]));
    stays_visible(&spec(), &triples(&[(1.0, 1.0, 1.0), (2.0, 2.0, 2.0)]));
    stays_visible(&spec(), &triples(&[(1.0, 1.0, 1.0), (1.0, 2.0, 2.0), (1.0, 3.0, 3.0)]));
    stays_visible(&spec(), &[json!({"a": null, "b": null, "c": null})]);
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = grid(5, 5);
    rows.push(json!({"a": 9.0, "b": 9.0, "c": null}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"a": "far", "b": 1.0, "c": 1.0}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn a_missing_channel_fails_with_a_source_error() {
    let s = json!({"type":"contour","title":"T","encoding":{"x":"a","y":"b"}});
    let g = draw(&s, &grid(4, 4));
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(
        &spec(),
        &triples(&[
            (-1e300, 0.0, 0.0),
            (0.0, 1e300, 1.0),
            (1e300, -1e300, 2.0),
            (0.0, 0.0, 3.0),
        ]),
    );
    assert!(nonfinite(&g).is_none());
    let g = draw(
        &spec(),
        &triples(&[
            (-1e308, 0.0, 0.0),
            (1e308, 1.0, 1.0),
            (0.0, -1e308, 2.0),
            (0.0, 0.0, 3.0),
        ]),
    );
    assert!(nonfinite(&g).is_none());
    let rows: Vec<Value> = grid(6, 6)
        .into_iter()
        .map(|r| json!({"a": r["a"].as_f64().unwrap() * 1e12, "b": r["b"], "c": r["c"].as_f64().unwrap() * 1e9}))
        .collect();
    assert!(nonfinite(&draw(&spec(), &rows)).is_none());
}

#[test]
fn facets_share_levels_and_colours() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for f in ["p", "q"] {
        for r in grid(6, 6) {
            let k = if f == "p" { 1.0 } else { 0.5 };
            rows.push(json!({"a": r["a"], "b": r["b"], "c": r["c"].as_f64().unwrap() * k, "f": f}));
        }
    }
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert!(g.panels.iter().all(|p| p.failure.is_none()));
    contract(&s, &rows);
}

#[test]
fn a_large_complete_grid_stays_a_grid() {
    let rows = grid(300, 300);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_none());
    let obs = observations(&g);
    assert!(obs.iter().all(|o| o.row.is_some()));
    assert!(obs.iter().all(|o| o.readout.lines.iter().all(|l| !l.derived)));
}

#[test]
fn scattered_rows_over_the_limit_are_not_drawn_and_name_the_limit() {
    let g = draw(&spec(), &scattered(4001));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 4001);
    assert_eq!(c.details["limit"], 4000);
    assert_eq!(c.details["what"], "scattered rows per panel");
    assert!(observations(&g).is_empty());
}

#[test]
fn scattered_rows_at_the_limit_are_all_weighed() {
    let g = draw(&spec(), &scattered(4000));
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert!(check(&g, "excluded-observations").is_none());
}

#[test]
fn a_grid_beyond_the_node_count_counts_as_scattered_rows_and_hits_the_limit() {
    let g = draw(&spec(), &grid(600, 500));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.details["count"], 300_000);
}

#[test]
fn the_key_writes_each_level_value_under_its_place() {
    let g = draw(&spec(), &grid(9, 9));
    assert!(g.failure.is_none());
    let texts: Vec<String> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "10"), "{texts:?}");
    assert!(texts.len() >= 5, "{texts:?}");
}

#[test]
fn a_grid_whose_columns_fall_in_one_grid_step_stays_a_grid_in_the_runtime() {
    // Columns 100.01 and 100.04 are one step of the 4,096-step grid over 0..1000.
    let mut rows = Vec::new();
    for (i, a) in [0.0, 100.01, 100.04, 1000.0].into_iter().enumerate() {
        for j in 0..4 {
            rows.push(json!({"a": a, "b": j as f64, "c": (i * 4 + j) as f64 * 1.5}));
        }
    }
    runtime_matches_native(&spec(), &rows);
}
