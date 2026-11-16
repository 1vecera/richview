//! 2-D bin charts: counts in a grid of rectangles coloured by count, a key that decodes the
//! colour, plain empty cells, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/bin2d.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"bin2d","title":"T","encoding":{"x":"a","y":"b"},"axes":{"x":{"label":"A"},"y":{"label":"B"}}})
}

fn pairs(ps: &[(f64, f64)]) -> Vec<Value> {
    ps.iter().map(|(a, b)| json!({"a": a, "b": b})).collect()
}

fn cells(g: &richview_chart::geometry::Geometry) -> Vec<&Item> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Rect { obs: Some(_), .. }))
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("bin2d", &spec, &rows);
}

#[test]
fn the_counts_of_the_cells_add_up_to_the_rows() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let counted: usize = observations(&g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(counted, rows.len());
    assert_eq!(cells(&g).len(), observations(&g).len());
    assert!(observations(&g).iter().all(|o| o.area.is_some()));
    assert!(
        observations(&g)
            .iter()
            .all(|o| o.readout.lines.iter().all(|l| l.derived))
    );
}

#[test]
fn cells_are_counted_in_the_right_place_and_the_fuller_cell_is_darker() {
    let rows = pairs(&[(0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (10.0, 10.0), (10.0, 10.0)]);
    let g = draw(&spec(), &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 2);
    assert_eq!(line(obs[0], "Observations"), Some("3"));
    assert_eq!(line(obs[1], "Observations"), Some("2"));
    // Low x and low y is at the left and the bottom.
    let (a, b) = (obs[0].area.unwrap(), obs[1].area.unwrap());
    assert!(a.x < b.x && a.y > b.y);
    let fills: Vec<_> = cells(&g)
        .iter()
        .map(|i| match i {
            Item::Rect { fill, .. } => fill.color,
            _ => unreachable!(),
        })
        .collect();
    assert_ne!(fills[0], fills[1]);
}

#[test]
fn a_key_names_both_ends_of_the_colour() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert!(g.figure_items.iter().any(|i| i.role() == Role::Key));
    let most = observations(&g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .max()
        .unwrap();
    let labels: Vec<String> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"1".to_string()), "{labels:?}");
    assert!(labels.contains(&most.to_string()), "{labels:?}");
}

#[test]
fn one_row_is_one_cell_and_a_constant_is_not_a_failure() {
    let one = pairs(&[(4.0, 7.0)]);
    let g = draw(&spec(), &one);
    assert_eq!(observations(&g).len(), 1);
    assert_eq!(line(observations(&g)[0], "Observations"), Some("1"));
    contract(&spec(), &one);
    contract(&spec(), &pairs(&[(3.0, 3.0), (3.0, 3.0), (3.0, 3.0)]));
}

#[test]
fn a_dense_sample_stays_inside_the_plot() {
    let rows: Vec<(f64, f64)> = (0..600)
        .map(|i| ((i * 37 % 101) as f64, (i * 11 % 53) as f64))
        .collect();
    let rows = pairs(&rows);
    contract(&spec(), &rows);
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = pairs(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    rows.push(json!({"a": 4.0, "b": null}));
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    let counted: usize = observations(&g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(counted, 3);
    rows.push(json!({"a": "many", "b": 1.0}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"a": null, "b": null})]);
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(&spec(), &pairs(&[(-1e300, 0.0), (0.0, 1e300), (1e300, -1e300)]));
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &pairs(&[(1e-300, 2e-300), (3e-300, 5e-300)]));
    assert!(nonfinite(&g).is_none());
}

#[test]
fn facets_share_their_cells_and_colour() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for f in ["p", "q"] {
        for i in 0..20 {
            rows.push(json!({"a": i as f64, "b": (i * 3 % 7) as f64, "f": f}));
        }
    }
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    let widths: Vec<f64> = g
        .panels
        .iter()
        .flat_map(|p| p.observations.iter().filter_map(|o| o.area))
        .map(|a| a.w)
        .collect();
    assert!(widths.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-2));
    contract(&s, &rows);
}

#[test]
fn readout_names_the_bin_once() {
    let g = draw(&spec(), &pairs(&[(0.0, 0.0), (10.0, 10.0)]));
    for o in observations(&g) {
        assert!(o.readout.heading.contains('×'), "{}", o.readout.heading);
        assert!(o.readout.heading.contains('–'), "{}", o.readout.heading);
        assert_eq!(o.readout.lines.len(), 1);
        assert_eq!(o.readout.lines[0].name, "Observations");
    }
}
