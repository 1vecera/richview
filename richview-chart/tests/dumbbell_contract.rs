//! Dumbbell charts: two values joined by a line, the signed change, groups, orientation and the
//! contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Shape};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"dumbbell","title":"Scores","encoding":{"x":"team","lower":"before","upper":"after"},"axes":{"y":{"label":"Score"}}})
}

fn scores() -> Vec<Value> {
    vec![
        json!({"team":"Alpha","before":62,"after":78}),
        json!({"team":"Beta","before":55,"after":51}),
        json!({"team":"Gamma","before":70,"after":88}),
    ]
}

fn grouped_spec() -> Value {
    let mut s = spec();
    s["encoding"]["color"] = json!("year");
    s
}

fn grouped_rows() -> Vec<Value> {
    vec![
        json!({"team":"Alpha","year":"2025","before":62,"after":78}),
        json!({"team":"Alpha","year":"2026","before":70,"after":72}),
        json!({"team":"Beta","year":"2025","before":55,"after":51}),
        json!({"team":"Beta","year":"2026","before":50,"after":60}),
    ]
}

fn dots(g: &richview_chart::geometry::Geometry) -> Vec<(f64, f64, bool)> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point {
                center, fill, stroke, ..
            } => Some((
                center[0],
                center[1],
                stroke.is_some() && fill.is_some() && stroke.is_some(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &scores());
    contract(&grouped_spec(), &grouped_rows());
}

#[test]
fn both_ends_sit_at_their_values_and_the_change_is_signed() {
    let g = draw(&spec(), &scores());
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    assert_eq!(obs[0].readout.heading, "Alpha");
    assert_eq!(line(obs[0], "Lower"), Some("62"));
    assert_eq!(line(obs[0], "Upper"), Some("78"));
    assert_eq!(line(obs[0], "Change"), Some("+16"));
    assert_eq!(line(obs[1], "Change"), Some("−4"));
    let derived = |o: &richview_chart::geometry::Observation, name: &str| {
        o.readout.lines.iter().find(|l| l.name == name).map(|l| l.derived)
    };
    assert_eq!(derived(obs[0], "Lower"), Some(false));
    assert_eq!(derived(obs[0], "Upper"), Some(false));
    assert_eq!(derived(obs[0], "Change"), Some(true));
    // Vertical: a larger value is higher. Alpha rises (end above start); Beta falls.
    let d = dots(&g);
    assert_eq!(d.len(), 6);
    let (a_lo, a_hi) = (d[0], d[1]);
    assert!(a_hi.1 < a_lo.1, "the end of a rise is above its start");
    let (b_lo, b_hi) = (d[2], d[3]);
    assert!(b_hi.1 > b_lo.1, "the end of a fall is below its start");
    // The start is hollow and the end is filled.
    let shapes: Vec<bool> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point {
                fill,
                shape: Shape::Circle,
                stroke,
                ..
            } => Some(stroke.is_some() && fill.is_some()),
            _ => None,
        })
        .collect();
    assert!(shapes[0], "the start dot has an outline");
    assert!(!shapes[1], "the end dot is a plain fill");
}

#[test]
fn the_value_axis_does_not_have_to_include_zero() {
    let g = draw(&spec(), &scores());
    let panel = &g.panels[0];
    let ys: Vec<f64> = panel
        .items
        .iter()
        .filter(|i| i.role() == richview_chart::geometry::Role::Baseline)
        .filter_map(|i| i.bounds().map(|b| b.y))
        .collect();
    assert!(ys.is_empty(), "no zero baseline when every value is far from zero");
}

#[test]
fn groups_sit_side_by_side_in_one_band() {
    let g = draw(&grouped_spec(), &grouped_rows());
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(line(obs[0], "2025 · Upper"), Some("78"));
    assert!(obs[0].x < obs[1].x && obs[1].x < obs[2].x);
    assert!(obs[1].x - obs[0].x < obs[2].x - obs[1].x);
}

#[test]
fn a_row_missing_either_end_is_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = scores();
    rows[1] = json!({"team":"Beta","before":55,"after":null});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 2);
    rows[1] = json!({"team":"Beta","before":"55","after":"fifty"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    rows[1] = json!({"team":"Beta","before":"1e-400","after":4});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"team":"Only","before":5,"after":9})]);
    contract(
        &spec(),
        &[
            json!({"team":"A","before":-5,"after":-7}),
            json!({"team":"B","before":-2,"after":3}),
        ],
    );
    // No change: both dots at one place, still reachable.
    let g = draw(
        &spec(),
        &[
            json!({"team":"A","before":4,"after":4}),
            json!({"team":"B","before":1,"after":2}),
        ],
    );
    assert_eq!(line(observations(&g)[0], "Change"), Some("0"));
    assert!(observations(&g)[0].area.unwrap().h >= 1.0);
    let g = draw(&spec(), &[json!({"team":"A","before":-1e300,"after":1e300})]);
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_charts_keep_every_name() {
    let rows: Vec<Value> = (0..60)
        .map(|i| json!({"team": format!("Team number {i}"), "before": i % 5, "after": (i % 7) + 1}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 60);
}

#[test]
fn horizontal_charts_place_categories_top_to_bottom_and_values_along_x() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &scores());
    let obs = observations(&g);
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    let d = dots(&g);
    assert!(d[1].0 > d[0].0, "a rise ends to the right of its start");
    assert!(d[3].0 < d[2].0, "a fall ends to the left of its start");
    contract(&s, &scores());
}

#[test]
fn facets_keep_their_own_marks() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("unit");
    let rows = vec![
        json!({"team":"A","before":1,"after":10,"unit":"North"}),
        json!({"team":"B","before":5,"after":6,"unit":"North"}),
        json!({"team":"A","before":2,"after":1,"unit":"South"}),
        json!({"team":"B","before":2,"after":3,"unit":"South"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(line(&g.panels[1].observations[0], "Change"), Some("−1"));
    contract(&s, &rows);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("dumbbell", &spec(), &scores());
    gallery("dumbbell-grouped", &grouped_spec(), &grouped_rows());
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    gallery("dumbbell-horizontal", &s, &scores());
}
