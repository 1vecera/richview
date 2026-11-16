//! Vector fields: arrows centred on their positions, pointing the way their angle says, as long
//! as their value, a colour key, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/vector.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"vector","title":"T","encoding":{"x":"a","y":"b","angle":"t"}})
}

fn with_size() -> Value {
    json!({"type":"vector","title":"T","encoding":{"x":"a","y":"b","angle":"t","size":"s"},"axes":{"color":{"label":"S"}}})
}

fn arrows(ps: &[(f64, f64, f64)]) -> Vec<Value> {
    ps.iter().map(|(a, b, t)| json!({"a": a, "b": b, "t": t})).collect()
}

/// Each arrow's shaft as (tail, base) points, in source order.
fn shafts(g: &Geometry) -> Vec<[[f64; 2]; 2]> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => match path.cmds.as_slice() {
                [Cmd::M { p: a }, Cmd::L { p: b }] => Some([*a, *b]),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("vector", &spec, &rows);
}

#[test]
fn an_arrow_points_the_way_its_angle_says() {
    // East, north, west and south-west on the screen, where north is up.
    let rows = arrows(&[
        (0.0, 0.0, 0.0),
        (10.0, 0.0, 90.0),
        (20.0, 0.0, 180.0),
        (30.0, 0.0, 225.0),
    ]);
    let g = draw_at(&spec(), &rows, 1440.0);
    let s = shafts(&g);
    assert_eq!(s.len(), 4);
    let heading = |s: &[[f64; 2]; 2]| (s[1][0] - s[0][0], s[1][1] - s[0][1]);
    let east = heading(&s[0]);
    assert!(east.0 > 0.0 && east.1.abs() < 1e-9);
    let north = heading(&s[1]);
    assert!(north.1 < 0.0 && north.0.abs() < 1e-9);
    let west = heading(&s[2]);
    assert!(west.0 < 0.0 && west.1.abs() < 1e-9);
    let south_west = heading(&s[3]);
    assert!(south_west.0 < 0.0 && south_west.1 > 0.0);
}

#[test]
fn arrows_are_centred_on_their_positions_and_inside_the_plot() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let plot = g.panels[0].plot;
        let obs = observations(&g);
        assert_eq!(obs.len(), rows.len());
        for item in marks(&g) {
            if let Some(b) = item.bounds() {
                assert!(b.inside(&plot, 0.5), "{width}: {b:?} leaves {plot:?}");
            }
        }
    }
}

#[test]
fn length_follows_the_value_and_colour_follows_it_too() {
    let rows = vec![
        json!({"a": 0.0, "b": 0.0, "t": 0.0, "s": 1.0}),
        json!({"a": 10.0, "b": 0.0, "t": 0.0, "s": 2.0}),
        json!({"a": 20.0, "b": 0.0, "t": 0.0, "s": 4.0}),
    ];
    let g = draw_at(&with_size(), &rows, 1440.0);
    let lengths: Vec<f64> = shafts(&g)
        .iter()
        .map(|s| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]))
        .collect();
    assert_eq!(lengths.len(), 3);
    assert!(lengths[0] < lengths[1] && lengths[1] < lengths[2]);
    assert!(g.figure_items.iter().any(|i| i.role() == Role::Key));
    let colours: std::collections::HashSet<String> = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Line {
                stroke,
                role: Role::Mark,
                ..
            } => Some(stroke.color.hex()),
            _ => None,
        })
        .collect();
    assert_eq!(colours.len(), 3);
    // Without a value the arrows are alike and there is no key.
    let g = draw(&spec(), &arrows(&[(0.0, 0.0, 0.0), (10.0, 0.0, 30.0)]));
    let s = shafts(&g);
    let l = |s: &[[f64; 2]; 2]| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]);
    assert!((l(&s[0]) - l(&s[1])).abs() < 1e-9);
    assert!(g.figure_items.is_empty());
}

#[test]
fn zero_value_arrows_are_dots_and_readouts_are_the_authors() {
    let rows = vec![
        json!({"a": 0.0, "b": 0.0, "t": 45.0, "s": 0.0}),
        json!({"a": 10.0, "b": 5.0, "t": 135.0, "s": 3.0}),
    ];
    let g = draw(&with_size(), &rows);
    assert!(g.failure.is_none());
    let dots = marks(&g).iter().filter(|i| matches!(i, Item::Point { .. })).count();
    assert_eq!(dots, 1);
    let obs = observations(&g);
    assert_eq!(obs.len(), 2);
    assert!(obs.iter().all(|o| o.readout.lines.iter().all(|l| !l.derived)));
    let o = obs.iter().find(|o| o.readout.heading == "Near 10, 5").unwrap();
    assert_eq!(line(o, "Direction"), Some("135°"));
    assert_eq!(line(o, "S"), Some("3"));
    assert_eq!(o.readout.lines.len(), 2);
}

#[test]
fn readout_headings_are_independent_of_plot_size() {
    let rows: Vec<Value> = (0..=10)
        .flat_map(|x| (0..=2).map(move |y| json!({"a": x, "b": y, "t": 0})))
        .collect();
    let headings = |width| {
        let g = draw_at(&spec(), &rows, width);
        observations(&g)
            .iter()
            .map(|o| o.readout.heading.clone())
            .collect::<Vec<_>>()
    };
    let narrow = headings(320.0);
    assert_eq!(narrow.len(), rows.len());
    assert_eq!(narrow, headings(1440.0));
}

#[test]
fn readout_coordinates_respect_author_precision_even_for_one_arrow() {
    let mut chart = spec();
    chart["readout_digits"] = json!(3);
    let g = draw(&chart, &arrows(&[(1.23456, 2.34567, 30.0)]));
    let obs = observations(&g);
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0].readout.heading, "Near 1.23, 2.35");
    let g = draw(&chart, &arrows(&[(10_000.0, 4.0, 30.0)]));
    assert_eq!(observations(&g)[0].readout.heading, "Near 10K, 4");
    chart["axes"] = json!({"x": {"unit": "km"}, "y": {"unit": "km"}});
    let g = draw(&chart, &arrows(&[(0.0, 4.0, 30.0)]));
    assert_eq!(observations(&g)[0].readout.heading, "Near 0, 4\u{a0}km");
    chart["readout_digits"] = json!(12);
    chart["axes"] = json!({"x": {"format": "number"}});
    let g = draw(&chart, &arrows(&[(999_999.125, 4.0, 30.0)]));
    assert_eq!(observations(&g)[0].readout.heading, "Near 999,999.125, 4");
    chart["readout_digits"] = json!(3);
    chart["axes"] = json!({"x": {"format": "percent"}});
    let g = draw(&chart, &arrows(&[(0.123, 4.0, 30.0)]));
    assert_eq!(observations(&g)[0].readout.heading, "Near 12.3%, 4");
}

#[test]
fn one_arrow_and_coinciding_arrows_draw() {
    let one = arrows(&[(3.0, 4.0, 60.0)]);
    contract(&spec(), &one);
    let same = arrows(&[(1.0, 1.0, 0.0), (1.0, 1.0, 90.0), (1.0, 1.0, 180.0)]);
    contract(&spec(), &same);
    let line_of_them = arrows(&[(0.0, 5.0, 0.0), (1.0, 5.0, 0.0), (2.0, 5.0, 0.0)]);
    contract(&spec(), &line_of_them);
}

fn crowd(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| json!({"a": (i % 50) as f64, "b": (i / 50) as f64, "t": (i * 7 % 360) as f64}))
        .collect()
}

#[test]
fn a_panel_over_the_arrow_limit_is_not_drawn_and_names_the_limit() {
    let g = draw(&spec(), &crowd(1501));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 1501);
    assert_eq!(c.details["limit"], 1500);
    assert_eq!(c.details["what"], "arrows per panel");
    assert!(observations(&g).is_empty());
}

#[test]
fn a_panel_at_the_arrow_limit_draws_every_arrow() {
    let rows = crowd(1500);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 1500);
    assert!(nonfinite(&g).is_none());
    contract(&spec(), &rows[..300]);
}

#[test]
fn missing_and_negative_values_are_left_out_and_bad_text_fails() {
    let mut rows = vec![
        json!({"a": 0.0, "b": 0.0, "t": 0.0, "s": 1.0}),
        json!({"a": 1.0, "b": 1.0, "t": 10.0, "s": 2.0}),
        json!({"a": 2.0, "b": 2.0, "t": 20.0, "s": -1.0}),
        json!({"a": 3.0, "b": 3.0, "t": null, "s": 1.0}),
        json!({"a": 4.0, "b": null, "t": 5.0, "s": 1.0}),
        json!({"a": 5.0, "b": 5.0, "t": 5.0, "s": null}),
    ];
    let g = draw(&with_size(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 2);
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"a": "far", "b": 1.0, "t": 1.0, "s": 1.0}));
    let g = draw(&with_size(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"a": null, "b": null, "t": null})]);
}

#[test]
fn a_missing_channel_fails_with_a_source_error() {
    let s = json!({"type":"vector","title":"T","encoding":{"x":"a","y":"b"}});
    let g = draw(&s, &arrows(&[(0.0, 0.0, 0.0)]));
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(
        &spec(),
        &arrows(&[(-1e300, 0.0, 0.0), (0.0, 1e300, 90.0), (1e300, -1e300, 1e300)]),
    );
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &arrows(&[(1e-300, 2e-300, 10.0), (3e-300, 5e-300, 20.0)]));
    assert!(nonfinite(&g).is_none());
    let rows = vec![
        json!({"a": 0.0, "b": 0.0, "t": 0.0, "s": 1e300}),
        json!({"a": 1.0, "b": 1.0, "t": 0.0, "s": 1e-300}),
    ];
    let g = draw(&with_size(), &rows);
    assert!(nonfinite(&g).is_none());
}

#[test]
fn facets_share_axes_and_one_length_scale() {
    let mut s = with_size();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for (f, v) in [("p", 4.0), ("q", 2.0)] {
        for i in 0..6 {
            rows.push(json!({"a": i as f64, "b": (i % 2) as f64, "t": 30.0 * i as f64, "s": v, "f": f}));
        }
    }
    let g = draw_at(&s, &rows, 1440.0);
    assert_eq!(g.panels.len(), 2);
    let first = |p: usize| -> f64 {
        g.panels[p]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Line {
                    path, role: Role::Mark, ..
                } => match path.cmds.as_slice() {
                    [Cmd::M { p: a }, Cmd::L { p: b }] => Some((b[0] - a[0]).hypot(b[1] - a[1])),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    };
    // Shafts differ by a little more than the values do (4 to 2), because a head has a size of its own.
    let ratio = first(0) / first(1);
    assert!((2.0..2.6).contains(&ratio), "{} {}", first(0), first(1));
    contract(&s, &rows);
}

#[test]
fn the_direction_keeps_only_the_authors_readout_digits() {
    let rows = arrows(&[(0.0, 0.0, 37.123456789)]);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(line(observations(&g)[0], "Direction"), Some("37.1°"));
    let precise = json!({"type":"vector","title":"T","readout_digits":5,"encoding":{"x":"a","y":"b","angle":"t"}});
    let g = draw(&precise, &rows);
    assert_eq!(line(observations(&g)[0], "Direction"), Some("37.123°"));
}

#[test]
fn the_runtime_draws_the_arrows_the_exact_rows_kept_and_sized() {
    // Two positions a hair apart, far from the rest, and a value just below zero.
    let rows = vec![
        json!({"a": 0.0, "b": 0.0, "t": 10.0, "s": 5.0}),
        json!({"a": 100.01, "b": 500.01, "t": 80.0, "s": 3.0}),
        json!({"a": 100.04, "b": 500.04, "t": 200.0, "s": 4.0}),
        json!({"a": 1000.0, "b": 1000.0, "t": 300.0, "s": -0.0001}),
        json!({"a": 400.0, "b": 900.0, "t": 45.0, "s": 2.0}),
    ];
    runtime_matches_native(&with_size(), &rows);
}
