//! A pie with `style.inner` is a donut: the same shares, a hole, and slices that are hit only
//! on the ring (FEA-CH-08).

mod common;

use common::*;
use richview_chart::hit;
use serde_json::{Value, json};

fn spec(inner: f64) -> Value {
    json!({"type":"pie","title":"Share","encoding":{"x":"channel","y":"share"},"style":{"inner":inner}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"channel":"Search","share":50}),
        json!({"channel":"Social","share":30}),
        json!({"channel":"Email","share":20}),
    ]
}

#[test]
fn a_donut_keeps_the_pie_shares_and_meets_the_shared_contract() {
    let pie = draw(&spec(0.0), &rows());
    let donut = draw(&spec(0.55), &rows());
    let shares = |g: &richview_chart::geometry::Geometry| -> Vec<String> {
        observations(g)
            .iter()
            .map(|o| line(o, "Share").unwrap().to_owned())
            .collect()
    };
    assert_eq!(shares(&pie), shares(&donut));
    assert!(donut.failure.is_none() && nonfinite(&donut).is_none());
}

#[test]
fn the_hole_is_not_part_of_a_slice() {
    let g = draw(&spec(0.55), &rows());
    let w = observations(&g)[0].wedge.unwrap();
    assert!(w.inner > 0.0 && w.inner < w.radius);
    assert!(hit::at(&g, w.center).is_none(), "the hole selects nothing");
    let ring = [
        w.center[0] + (w.inner + w.radius) / 2.0 * 0.0,
        w.center[1] - (w.inner + w.radius) / 2.0,
    ];
    // Straight up from the centre is the start of the first slice.
    assert!(hit::at(&g, [ring[0] + 1.0, ring[1]]).is_some());
}

#[test]
fn a_single_category_is_a_closed_ring() {
    let g = draw(&spec(0.6), &[json!({"channel":"All","share":7})]);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 1);
    assert!(nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("donut", &spec(0.55), &rows());
}

#[test]
fn the_runtime_names_and_orders_the_slices_the_exact_rows_do() {
    // 0.04 beside 1000 is zero on the grid, and shares that round differently come from the sums.
    let rows = vec![
        json!({"channel":"Search","share":1000.0}),
        json!({"channel":"Social","share":333.33}),
        json!({"channel":"Email","share":0.04}),
        json!({"channel":"Social","share":100.01}),
    ];
    for inner in [0.0, 0.55] {
        runtime_matches_native(&spec(inner), &rows);
    }
}
