//! 2-D bin charts: counts in a grid of rectangles coloured by count and printing it, a key that decodes the
//! colour, plain empty cells, and the contract every type shares (FEA-CH-75).

use super::common;

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

/// The numbers of a chart's value labels, with those it records as dropped, in order.
fn counts(g: &richview_chart::geometry::Geometry) -> Vec<String> {
    let mut all: Vec<String> = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t.text.clone()),
            _ => None,
        })
        .chain(g.dropped.iter().map(|d| d.text.clone()))
        .collect();
    all.sort();
    all
}

fn has_key(g: &richview_chart::geometry::Geometry) -> bool {
    g.figure_items.iter().any(|i| i.role() == Role::Key)
}

#[test]
fn every_cell_prints_its_count_and_the_chart_draws_no_key() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 720.0);
    let obs = observations(&g);
    let labels: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), obs.len());
    // Each count stands inside its own cell and names it.
    let ids = g.mark_ids();
    for (k, (label, o)) in labels.iter().zip(&obs).enumerate() {
        assert_eq!(Some(label.text.as_str()), line(o, "Observations"));
        assert_eq!(label.mark.as_deref(), Some(ids[k].as_str()));
        let area = o.area.unwrap();
        assert!(
            label.bounds.x >= area.x
                && label.bounds.right() <= area.right()
                && label.bounds.y >= area.y
                && label.bounds.bottom() <= area.bottom(),
            "{} leaves its cell",
            label.text
        );
    }
    assert!(!has_key(&g), "the counts decode the colour");
    assert!(g.dropped.is_empty());
}

#[test]
fn a_cell_too_small_for_its_count_brings_the_key_back_and_records_each_count() {
    // 600 observations make 24 by 24 bins, whose cells are too small at any report width.
    let rows: Vec<(f64, f64)> = (0..600)
        .map(|i| ((i * 37 % 101) as f64, (i * 11 % 53) as f64))
        .collect();
    let g = draw_at(&spec(), &pairs(&rows), 390.0);
    let obs = observations(&g);
    let most = obs
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert!(has_key(&g));
    let ends: Vec<String> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ends, ["1".to_string(), most.to_string()]);
    // Never neither: a cell without its count is one the key decodes, and its count is on record.
    let mut expected: Vec<String> = obs
        .iter()
        .map(|o| line(o, "Observations").unwrap().to_owned())
        .collect();
    expected.sort();
    assert_eq!(counts(&g), expected);
    assert!(g.dropped.iter().all(|d| d.kind.as_deref() == Some("cell-value")));
    assert!(g.dropped.iter().all(|d| d.observation.is_some()));
}

#[test]
fn a_chart_carries_the_same_counts_at_every_width() {
    let (spec, rows) = fixture();
    let wide = counts(&draw_at(&spec, &rows, 1440.0));
    assert!(!wide.is_empty());
    for width in [1100.0, 720.0, 390.0, 320.0] {
        assert_eq!(counts(&draw_at(&spec, &rows, width)), wide, "at {width}");
    }
}

#[test]
fn the_author_can_have_the_key_instead_of_the_counts() {
    let (mut spec, rows) = fixture();
    spec["auto_annotations"] = json!(false);
    let g = draw(&spec, &rows);
    assert!(has_key(&g));
    assert!(
        counts(&g).is_empty(),
        "no count printed and none dropped: the author chose it"
    );
    let most = observations(&g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .max()
        .unwrap();
    let ends: Vec<String> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ends, ["1".to_string(), most.to_string()]);
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
