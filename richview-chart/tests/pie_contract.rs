//! Pie charts: one slice per category with a positive total, spans in proportion to the values in
//! the author's order from 12 o'clock, shares that add up, rows that cannot be a slice named, and
//! the contract every type shares (FEA-CH-75). Share labels are in `pie_share_labels.rs` and rings
//! in `donut_contract.rs`.

use super::common;

use common::*;
use richview_chart::geometry::Targeting;
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn spec() -> Value {
    json!({"type":"pie","title":"Where the engineering hour goes","encoding":{"x":"activity","y":"hours"}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"activity":"coding","hours":3.1}),
        json!({"activity":"review","hours":1.4}),
        json!({"activity":"meetings","hours":1.8}),
        json!({"activity":"ops","hours":0.9}),
    ]
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn spans_follow_the_values_in_the_authors_order_from_twelve_oclock() {
    let g = draw(&spec(), &rows());
    assert_eq!(g.panels[0].targeting, Targeting::Contain);
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    let heads: Vec<&str> = obs.iter().map(|o| o.readout.heading.as_str()).collect();
    assert_eq!(
        heads,
        ["coding", "review", "meetings", "ops"],
        "the author's order, not largest first"
    );
    let wedges: Vec<_> = obs.iter().map(|o| o.wedge.expect("a slice is hit by angle")).collect();
    assert!(wedges[0].start.abs() < 1e-9, "the first slice starts at 12 o'clock");
    assert!((wedges[3].end - TAU).abs() < 1e-9, "the last slice closes the disk");
    for pair in wedges.windows(2) {
        assert!((pair[0].end - pair[1].start).abs() < 1e-9, "slices touch");
    }
    // Edges sit on the share grid of TYP-36 (4,096 steps), so a span matches its value to within a step.
    for (w, value) in wedges.iter().zip([3.1, 1.4, 1.8, 0.9]) {
        assert!(
            ((w.end - w.start) / TAU - value / 7.2).abs() < 1.0 / 4096.0,
            "{value} spans {}",
            (w.end - w.start) / TAU
        );
    }
}

#[test]
fn readouts_give_the_authors_value_and_a_share_and_the_shares_add_up() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(line(obs[0], "Hours"), Some("3.1"));
    assert_eq!(line(obs[0], "Share"), Some("43.1%"));
    assert_eq!(line(obs[3], "Share"), Some("12.5%"));
    let sum: f64 = obs
        .iter()
        .map(|o| line(o, "Share").unwrap().trim_end_matches('%').parse::<f64>().unwrap())
        .sum();
    assert!(
        (sum - 100.0).abs() < 0.11,
        "shares add up to 100% within their rounding: {sum}"
    );
    assert!(
        g.summary.contains("coding") && g.summary.contains("43.1%"),
        "{}",
        g.summary
    );
}

#[test]
fn each_slice_is_reached_at_the_middle_of_its_span() {
    let g = draw(&spec(), &rows());
    for (i, o) in observations(&g).iter().enumerate() {
        let w = o.wedge.unwrap();
        let mid = (w.start + w.end) / 2.0;
        let r = w.radius * 0.6;
        let at = [w.center[0] + r * mid.sin(), w.center[1] - r * mid.cos()];
        let found = richview_chart::hit::at(&g, at).unwrap_or_else(|| panic!("slice {i} is not hit at {at:?}"));
        assert_eq!(found.observation, i);
    }
}

#[test]
fn rows_that_cannot_be_a_slice_are_named_or_left_out_without_moving_the_others() {
    // A negative value is an error that names its row; the others still draw.
    let g = draw(
        &spec(),
        &[json!({"activity":"a","hours":-1}), json!({"activity":"b","hours":2})],
    );
    assert!(check(&g, "negative-pie-value").is_some(), "{:?}", g.checks);
    assert_eq!(observations(&g).len(), 1);
    // A row with no value gets no slice, and the author is told.
    let g = draw(
        &spec(),
        &[json!({"activity":"a","hours":null}), json!({"activity":"b","hours":2})],
    );
    assert!(g.failure.is_none());
    assert!(check(&g, "missing-observations").is_some(), "{:?}", g.checks);
    assert_eq!(observations(&g).len(), 1);
    assert_eq!(line(observations(&g)[0], "Share"), Some("100%"));
    // Text in the value channel is an error.
    let g = draw(
        &spec(),
        &[json!({"activity":"a","hours":"1,0"}), json!({"activity":"b","hours":2})],
    );
    assert!(check(&g, "non-numeric").is_some());
    // A zero is no slice; it has no angle to be reached by.
    let g = draw(
        &spec(),
        &[json!({"activity":"a","hours":0}), json!({"activity":"b","hours":2})],
    );
    assert_eq!(observations(&g).len(), 1);
}

#[test]
fn rows_of_one_category_are_one_slice_with_their_total() {
    let g = draw(
        &spec(),
        &[
            json!({"activity":"a","hours":1}),
            json!({"activity":"a","hours":2}),
            json!({"activity":"b","hours":3}),
        ],
    );
    let obs = observations(&g);
    assert_eq!(obs.len(), 2);
    assert_eq!(line(obs[0], "Hours"), Some("3"));
    assert_eq!(line(obs[0], "Share"), Some("50%"));
}

#[test]
fn a_pie_with_nothing_positive_says_why() {
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"activity":"a","hours":-1})]);
    stays_visible(
        &spec(),
        &[json!({"activity":"a","hours":0}), json!({"activity":"b","hours":0})],
    );
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    // One slice is the whole disk and stays reachable.
    let one = [json!({"activity":"a","hours":5})];
    contract(&spec(), &one);
    let w = observations(&draw(&spec(), &one))[0].wedge.unwrap();
    assert!((w.end - w.start - TAU).abs() < 1e-9);
    // Wildly different sizes: the small slice keeps a span and a readout.
    contract(
        &spec(),
        &[
            json!({"activity":"big","hours":1e6}),
            json!({"activity":"small","hours":1}),
        ],
    );
    // Huge values keep their shares; a total beyond the finite range is named, not drawn.
    let g = draw(
        &spec(),
        &[
            json!({"activity":"a","hours":1e300}),
            json!({"activity":"b","hours":1e300}),
        ],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(line(observations(&g)[0], "Share"), Some("50%"));
    let g = draw(
        &spec(),
        &[
            json!({"activity":"a","hours":1.5e308}),
            json!({"activity":"b","hours":1.5e308}),
        ],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "value-overflow").is_some());
}

#[test]
fn more_slices_than_palette_colours_still_draw_and_warn_that_colours_repeat() {
    let many: Vec<Value> = (0..10)
        .map(|i| json!({"activity": format!("s{i}"), "hours": i + 1}))
        .collect();
    let g = draw(&spec(), &many);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 10);
    assert!(check(&g, "series-differentiation").is_some(), "{:?}", g.checks);
    contract(&spec(), &many);
}

#[test]
fn facets_draw_one_pie_each() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("quarter");
    let rows = vec![
        json!({"activity":"a","hours":1,"quarter":"Q1"}),
        json!({"activity":"b","hours":3,"quarter":"Q1"}),
        json!({"activity":"a","hours":2,"quarter":"Q2"}),
        json!({"activity":"b","hours":2,"quarter":"Q2"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(line(&g.panels[0].observations[0], "Share"), Some("25%"));
    assert_eq!(line(&g.panels[1].observations[0], "Share"), Some("50%"));
    contract(&s, &rows);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("pie", &spec(), &rows());
}

#[test]
fn the_runtime_keeps_values_a_grid_step_apart() {
    let rows = vec![
        json!({"activity":"a","hours":100.01}),
        json!({"activity":"b","hours":1000}),
        json!({"activity":"c","hours":100.04}),
        json!({"activity":"d","hours":0.5}),
    ];
    runtime_matches_native(&spec(), &rows);
}
