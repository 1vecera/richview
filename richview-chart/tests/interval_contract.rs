//! Interval charts: authored bounds and an optional estimate, never an inferred distribution or
//! confidence, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type": "interval", "title": "CPI", "encoding": {"x": "year", "y": "cpi", "lower": "low", "upper": "high"},
        "axes": {"y": {"label": "Estimate", "unit": "%"}}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"year": "2023", "cpi": 4.1, "low": 3.2, "high": 5.3}),
        json!({"year": "2024", "cpi": 2.9, "low": 2.4, "high": 3.6}),
        json!({"year": "2025", "cpi": 2.5, "low": 1.8, "high": 3.1}),
    ]
}

/// The straight mark lines of the first panel, as their two ends.
fn segments(g: &Geometry) -> Vec<([f64; 2], [f64; 2])> {
    g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => match path.cmds.as_slice() {
                [Cmd::M { p: a }, Cmd::L { p: b }] => Some((*a, *b)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The ranges, as `(x, top, bottom)` in paint order.
fn spans(g: &Geometry) -> Vec<(f64, f64, f64)> {
    segments(g)
        .into_iter()
        .filter(|(a, b)| a[0] == b[0])
        .map(|(a, b)| (a[0], a[1].min(b[1]), a[1].max(b[1])))
        .collect()
}

/// How many caps close the ranges.
fn caps(g: &Geometry) -> usize {
    segments(g).into_iter().filter(|(a, b)| a[1] == b[1]).count()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
    let mut turned = spec();
    turned["style"] = json!({"orientation": "horizontal"});
    contract(&turned, &rows());
}

#[test]
fn authored_bounds_and_the_estimate_are_read_as_written() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    assert_eq!(line(obs[0], "Upper"), Some("5.3%"));
    assert_eq!(line(obs[0], "Estimate"), Some("4.1%"));
    assert_eq!(line(obs[0], "Lower"), Some("3.2%"));
    assert!(
        obs[0].readout.lines.iter().all(|l| !l.derived),
        "every readout value is authored"
    );
    // Each range spans from its lower to its upper bound, and the estimate dot sits inside it.
    let ranges = spans(&g);
    assert_eq!(ranges.len(), 3, "one range per row: {ranges:?}");
    // The pixel spans keep the authored spans' proportions: 2.1, 1.2 and 1.3 points.
    let heights: Vec<f64> = ranges.iter().map(|(_, top, bottom)| bottom - top).collect();
    for (k, span) in [2.1, 1.2, 1.3].into_iter().enumerate() {
        let ratio = heights[k] / heights[0];
        assert!((ratio - span / 2.1).abs() < 0.01, "range {k}: {heights:?}");
    }
    let dots: Vec<[f64; 2]> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point { center, .. } => Some(*center),
            _ => None,
        })
        .collect();
    assert_eq!(dots.len(), 3, "one estimate per row");
    for (dot, (x, top, bottom)) in dots.iter().zip(&ranges) {
        assert!((dot[0] - x).abs() < 0.5 && dot[1] > *top && dot[1] < *bottom);
    }
}

#[test]
fn reversed_bounds_and_repeated_categories_are_actionable_source_errors() {
    let mut reversed = rows();
    reversed[1] = json!({"year": "2024", "cpi": 2.9, "low": 3.6, "high": 2.4});
    let g = draw(&spec(), &reversed);
    assert!(g.failure.is_some());
    let error = check(&g, "source-error").expect("the reversed row is named");
    assert!(error.message.contains("Row 2"), "{}", error.message);
    assert!(error.fix.is_some());
    let mut repeated = rows();
    repeated.push(json!({"year": "2025", "cpi": 2.6, "low": 2.0, "high": 3.0}));
    let g = draw(&spec(), &repeated);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some_and(|c| c.message.contains("an interval needs one")));
}

#[test]
fn a_row_missing_a_bound_is_left_out_and_disclosed() {
    let mut gap = rows();
    gap[1] = json!({"year": "2024", "cpi": 2.9, "low": null, "high": 3.6});
    let g = draw(&spec(), &gap);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 2);
    assert!(check(&g, "excluded-observations").is_some());
}

#[test]
fn a_range_cut_at_an_authored_limit_has_no_cap_at_the_cut() {
    let mut limited = spec();
    limited["axes"]["y"]["min"] = json!(2.0);
    let g = draw(&limited, &rows());
    assert!(check(&g, "clipped-interval").is_some(), "the cut is disclosed");
    assert_eq!(caps(&draw(&spec(), &rows())), 6, "two caps per whole range");
    assert_eq!(caps(&g), 5, "the range below 2.0 loses its lower cap");
}
