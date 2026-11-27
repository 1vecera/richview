//! Surfaces: a mesh of quadrilaterals projected into 2-D items, far quads first, coloured by z with
//! a key, a grid as given or an interpolated one, and the views the author can ask for
//! (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/surface.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"surface","title":"T","encoding":{"x":"a","y":"b","z":"c"}})
}

fn point(a: f64, b: f64, c: f64) -> Value {
    json!({"a": a, "b": b, "c": c})
}

fn grid(nx: usize, ny: usize, z: impl Fn(usize, usize) -> f64) -> Vec<Value> {
    let mut out = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            out.push(point(i as f64, j as f64, z(i, j)));
        }
    }
    out
}

/// The quads in the order they are drawn: centre height and fill.
fn quads(g: &Geometry) -> Vec<(f64, richview_chart::geometry::Rgba)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Area {
                path,
                fill,
                role: Role::Mark,
                ..
            } => {
                let b = path.bounds().unwrap();
                Some((b.y + b.h / 2.0, fill.color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("surface", &spec, &rows);
}

#[test]
fn a_full_grid_is_one_quad_per_cell_and_its_readout_gives_the_corners_mean() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(quads(&g).len(), 36);
    let o = observations(&g);
    assert_eq!(o.len(), 36);
    let z = o[0].readout.lines.iter().find(|l| l.name == "Yield (t/ha)").unwrap();
    assert!(z.derived, "the mean of four corners is computed");
    assert!(!z.value.starts_with("≈ ") && !o[0].readout.heading.contains("≈ "));
    assert_eq!(o[0].readout.lines.len(), 1);
    assert!(
        o[0].readout.heading.contains('–') && o[0].readout.heading.contains(" × "),
        "{}",
        o[0].readout.heading
    );
    assert!(g.note.is_none(), "{:?}", g.note);
}

#[test]
fn nearer_quads_are_drawn_over_farther_ones() {
    // Seen from the front (azimuth 0), a flat plane's far rows sit higher on the page and go first.
    let mut spec = plain();
    spec["style"] = json!({"azimuth": 0, "elevation": 30});
    let g = draw(&spec, &grid(4, 4, |_, _| 1.0));
    let heights: Vec<f64> = quads(&g).into_iter().map(|(y, _)| y).collect();
    assert_eq!(heights.len(), 9);
    assert!(heights.windows(2).all(|w| w[0] <= w[1] + 1e-9), "{heights:?}");
    contract(&spec, &grid(4, 4, |_, _| 1.0));
}

#[test]
fn higher_z_is_darker_on_the_ramp() {
    let g = draw(&plain(), &grid(3, 3, |i, j| (i + j) as f64));
    let o = observations(&g);
    let z = |k: usize| -> f64 {
        let v = &o[k].readout.lines.iter().find(|l| l.name == "C").unwrap().value;
        v.parse().unwrap()
    };
    let colors: Vec<_> = quads(&g).into_iter().map(|(_, c)| c).collect();
    let lo = (0..o.len()).min_by(|a, b| z(*a).total_cmp(&z(*b))).unwrap();
    let hi = (0..o.len()).max_by(|a, b| z(*a).total_cmp(&z(*b))).unwrap();
    assert!(
        colors[hi].oklch().l < colors[lo].oklch().l,
        "the highest cell is the darkest"
    );
}

#[test]
fn a_key_under_the_chart_decodes_the_colour() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let keys = g.figure_items.iter().filter(|i| i.role() == Role::Key).count();
    assert!(keys >= 50, "a ramp and its labels: {keys}");
    let labels: Vec<&str> = g
        .figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"Yield (t/ha)"), "{labels:?}");
    let bottom = g
        .figure_items
        .iter()
        .filter_map(Item::bounds)
        .map(|b| b.bottom())
        .fold(0.0, f64::max);
    assert!(bottom <= g.height + 1e-9);
}

#[test]
fn the_view_follows_the_azimuth_and_the_elevation() {
    let (spec, rows) = fixture();
    let centres = |spec: &Value| -> Vec<[f64; 2]> { observations(&draw(spec, &rows)).iter().map(|o| o.at).collect() };
    let default = centres(&spec);
    let mut same = spec.clone();
    same["style"] = json!({"azimuth": 35, "elevation": 28});
    assert_eq!(default, centres(&same), "35 and 28 degrees are the defaults");
    let mut turned = spec.clone();
    turned["style"] = json!({"azimuth": 120, "elevation": 50});
    assert_ne!(default, centres(&turned));
}

#[test]
fn every_view_draws_clean() {
    let (spec, rows) = fixture();
    for azimuth in [0, 35, 90, 135, 180, 225, 270, 315] {
        for elevation in [5, 28, 60, 85, 200] {
            let mut s = spec.clone();
            s["style"] = json!({"azimuth": azimuth, "elevation": elevation});
            contract(&s, &rows);
        }
    }
}

#[test]
fn scattered_rows_are_interpolated_onto_a_grid_and_say_so() {
    let rows: Vec<Value> = (0..40)
        .map(|k| {
            let (a, b) = ((k * 7 % 13) as f64, (k * 5 % 11) as f64);
            point(a, b, a + 2.0 * b)
        })
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 23 * 23);
    assert!(g.note.as_deref().unwrap().contains("interpolated"));
    let o = observations(&g)[0];
    assert!(
        o.readout.lines.iter().all(|l| l.derived && l.value.starts_with("≈ ")),
        "{:?}",
        o.readout.lines
    );
    assert!(o.readout.heading.starts_with("≈ ") && o.readout.heading.contains(" × ≈ "));
    contract(&plain(), &rows);
}

#[test]
fn a_grid_over_the_node_limit_is_not_drawn_and_names_the_limit() {
    let rows = grid(101, 3, |i, j| ((i * j) % 17) as f64);
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 101);
    assert_eq!(c.details["limit"], 100);
    assert_eq!(c.details["what"], "grid nodes per axis");
    assert!(observations(&g).is_empty());
}

#[test]
fn a_grid_at_the_node_limit_draws_every_cell() {
    let rows = grid(100, 100, |i, j| ((i * j) % 17) as f64);
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 99 * 99);
}

#[test]
fn scattered_rows_over_the_limit_are_not_drawn_and_at_it_are() {
    let scattered = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|k| point((k * 7 % 13) as f64, (k * 5 % 11) as f64, (k % 9) as f64))
            .collect()
    };
    let g = draw(&plain(), &scattered(4001));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 4001);
    assert_eq!(c.details["limit"], 4000);
    assert_eq!(c.details["what"], "scattered rows");
    let g = draw(&plain(), &scattered(4000));
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
}

#[test]
fn rows_that_cannot_be_placed_are_disclosed_and_bad_input_says_why() {
    let mut rows = grid(3, 3, |i, j| (i * j) as f64);
    rows[4]["c"] = Value::Null;
    let g = draw(&plain(), &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 1);
    assert!(g.failure.is_none());

    let g = draw(&plain(), &[point(0.0, 0.0, 1.0), point(1.0, 1.0, 2.0)]);
    assert!(g.failure.as_deref().unwrap().contains("at least three rows"));
    let g = draw(
        &plain(),
        &[point(1.0, 0.0, 1.0), point(1.0, 1.0, 2.0), point(1.0, 2.0, 3.0)],
    );
    assert!(g.failure.as_deref().unwrap().contains("two different x values"));
    let g = draw(
        &plain(),
        &[
            json!({"a": 1, "b": 2, "c": "tall"}),
            point(1.0, 1.0, 2.0),
            point(2.0, 3.0, 4.0),
        ],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let spec = json!({"type":"surface","title":"T","encoding":{"x":"a","y":"b"}});
    let g = draw(&spec, &[point(1.0, 1.0, 1.0)]);
    assert!(g.failure.as_deref().unwrap().contains("encoding.z"));
    stays_visible(&plain(), &[]);
}

#[test]
fn flat_negative_and_tiny_surfaces_draw() {
    contract(&plain(), &grid(3, 3, |_, _| 4.0));
    contract(&plain(), &grid(2, 2, |i, j| -(i as f64) - j as f64));
    contract(&plain(), &grid(5, 5, |i, j| 1e-3 * (i * j) as f64));
}

#[test]
fn the_runtime_keeps_a_grid_whose_columns_fall_in_one_grid_step() {
    // Columns 100.01 and 100.04 are one step of the 4,096-step grid over 0..1000: the grid rows
    // would read as scattered, and the mesh is the exact rows' own.
    let mut rows = Vec::new();
    for (i, a) in [0.0, 100.01, 100.04, 1000.0].into_iter().enumerate() {
        for j in 0..4 {
            rows.push(point(a, j as f64, (i * 4 + j) as f64 * 1.5));
        }
    }
    runtime_matches_native(&plain(), &rows);
}
