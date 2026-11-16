//! Scatter charts: one dot per row at its two values, bubble area in proportion to the size
//! channel, colour groups with a legend, radius targeting, excluded rows named, and the contract
//! every type shares (FEA-CH-75). Label density and highlights are in `scatter_highlight.rs`.

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Targeting};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"scatter","title":"Latency against throughput","encoding":{"x":"rps","y":"p95","color":"region"}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"region":"eu","rps":820,"p95":180}),
        json!({"region":"eu","rps":1240,"p95":215}),
        json!({"region":"eu","rps":1680,"p95":258}),
        json!({"region":"us","rps":980,"p95":155}),
        json!({"region":"us","rps":1510,"p95":198}),
        json!({"region":"us","rps":2050,"p95":232}),
    ]
}

fn plain_spec() -> Value {
    json!({"type":"scatter","title":"Latency","encoding":{"x":"rps","y":"p95"}})
}

fn bubble_spec() -> Value {
    json!({"type":"scatter","title":"Latency","encoding":{"x":"rps","y":"p95","size":"n"}})
}

/// Each dot's centre and radius, in drawing order.
fn dots(g: &Geometry) -> Vec<([f64; 2], f64)> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point { center, radius, .. } => Some((*center, *radius)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
    contract(&plain_spec(), &rows());
    let bubbles: Vec<Value> = rows()
        .into_iter()
        .enumerate()
        .map(|(i, mut r)| {
            r["n"] = json!(i + 1);
            r
        })
        .collect();
    contract(&bubble_spec(), &bubbles);
}

#[test]
fn every_row_is_a_dot_at_its_values_read_back_as_the_author_wrote_them() {
    let g = draw(&spec(), &rows());
    assert_eq!(g.panels[0].targeting, Targeting::Radius);
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some(), "two colour groups need a key");
    let obs = observations(&g);
    assert_eq!(obs.len(), 6);
    assert_eq!(obs[1].readout.heading, "eu");
    assert_eq!(line(obs[1], "Rps"), Some("1,240"));
    assert_eq!(line(obs[1], "P95"), Some("215"));
    assert_eq!(line(obs[1], "Region"), None);
    let dots = dots(&g);
    assert_eq!(dots.len(), 6);
    // Right is larger x, up is larger y (a smaller screen y).
    assert!(dots[0].0[0] < dots[1].0[0] && dots[1].0[0] < dots[2].0[0]);
    assert!(dots[2].0[1] < dots[1].0[1] && dots[1].0[1] < dots[0].0[1]);
    // The readout anchor is the dot's centre.
    for (o, (center, _)) in obs.iter().zip(&dots) {
        assert!((o.at[0] - center[0]).abs() < 0.01 && (o.at[1] - center[1]).abs() < 0.01);
    }
}

#[test]
fn equal_steps_in_the_data_are_equal_steps_on_the_page() {
    let rows = vec![
        json!({"rps":100,"p95":10}),
        json!({"rps":200,"p95":20}),
        json!({"rps":300,"p95":30}),
    ];
    let g = draw(&plain_spec(), &rows);
    let d = dots(&g);
    let (dx1, dx2) = (d[1].0[0] - d[0].0[0], d[2].0[0] - d[1].0[0]);
    let (dy1, dy2) = (d[0].0[1] - d[1].0[1], d[1].0[1] - d[2].0[1]);
    assert!((dx1 - dx2).abs() < 0.01 && (dy1 - dy2).abs() < 0.01, "{d:?}");
}

#[test]
fn zoom_keeps_source_order_and_boundary_duplicates_for_ordered_and_unordered_rows() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, Prepared, layout_prepared};
    let block = ChartBlock::from_value(&plain_spec()).unwrap();
    let base: Vec<_> = (0..24)
        .map(|i| json!({"rps": i / 2, "p95": if i % 7 == 0 { Value::Null } else { json!(i) }}))
        .collect();
    for mut rows in [base.clone(), base.iter().rev().cloned().collect()] {
        for rotate in [0, 5] {
            rows.rotate_left(rotate);
            let prepared = Prepared::default();
            for window in [None, Some([3.0, 7.0]), Some([4.0, 4.5]), None] {
                let opts = Options {
                    zoom: window,
                    ..Options::default()
                };
                let geometry = layout_prepared(&block, &rows, &ChartTheme::default(), &opts, &prepared);
                let expected: Vec<_> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| {
                        let x = row["rps"].as_f64()?;
                        row["p95"].as_f64()?;
                        window.is_none_or(|w| x >= w[0] && x <= w[1]).then_some(Some(i))
                    })
                    .collect();
                let actual: Vec<_> = geometry.panels[0].observations.iter().map(|o| o.row).collect();
                assert_eq!(actual, expected, "window {window:?}, rotate {rotate}");
                for observation in &geometry.panels[0].observations {
                    let row = &rows[observation.row.unwrap()];
                    assert_eq!(line(observation, "Rps"), Some(row["rps"].to_string().as_str()));
                    assert_eq!(line(observation, "P95"), Some(row["p95"].to_string().as_str()));
                }
            }
        }
    }
}

#[test]
fn numeric_x_mapping_keeps_linear_scale_bits_including_overflowing_domains() {
    use richview_chart::scale::Linear;
    for values in [[0.0, 50.0, 100.0], [1e-9, 2e-9, 3e-9], [-1e308, 0.0, 1e308]] {
        let rows: Vec<_> = values.into_iter().map(|x| json!({"rps": x, "p95": 1})).collect();
        for width in [390.0, 1440.0] {
            let geometry = draw_at(&plain_spec(), &rows, width);
            let panel = &geometry.panels[0];
            let scale = panel.x.as_ref().unwrap();
            let reference = Linear::new((scale.domain[0], scale.domain[1]), (scale.range[0], scale.range[1]));
            for observation in &panel.observations {
                let value = rows[observation.row.unwrap()]["rps"].as_f64().unwrap();
                assert_eq!(observation.at[0].to_bits(), reference.map(value).to_bits());
            }
        }
    }
}

#[test]
fn bubble_area_is_in_proportion_to_the_size_value() {
    let rows = vec![
        json!({"rps":100,"p95":10,"n":1}),
        json!({"rps":200,"p95":20,"n":4}),
        json!({"rps":300,"p95":30,"n":9}),
    ];
    let g = draw(&bubble_spec(), &rows);
    let d = dots(&g);
    assert!(
        (d[1].1 / d[0].1 - 2.0).abs() < 0.01 && (d[2].1 / d[0].1 - 3.0).abs() < 0.01,
        "radius follows the square root: {d:?}"
    );
    assert_eq!(line(observations(&g)[2], "N"), Some("9"));
}

#[test]
fn a_zero_size_has_no_dot_but_stays_readable_and_a_negative_size_is_named() {
    let rows = vec![json!({"rps":100,"p95":10,"n":0}), json!({"rps":200,"p95":20,"n":4})];
    let g = draw(&bubble_spec(), &rows);
    assert_eq!(dots(&g).len(), 1);
    assert_eq!(observations(&g).len(), 2, "the zero-size row keeps its readout");
    let rows = vec![json!({"rps":100,"p95":10,"n":-1}), json!({"rps":200,"p95":20,"n":4})];
    let g = draw(&bubble_spec(), &rows);
    assert!(check(&g, "invalid-bubble-size").is_some(), "{:?}", g.checks);
    assert_eq!(dots(&g).len(), 1, "the negative-size row is left out, and said to be");
}

#[test]
fn rows_without_a_position_are_named_and_bad_text_fails_the_chart() {
    let g = draw(
        &plain_spec(),
        &[json!({"rps":null,"p95":10}), json!({"rps":200,"p95":20})],
    );
    assert!(check(&g, "excluded-observations").is_some(), "{:?}", g.checks);
    assert_eq!(dots(&g).len(), 1);
    let g = draw(
        &plain_spec(),
        &[json!({"rps":100,"p95":null}), json!({"rps":200,"p95":20})],
    );
    assert!(check(&g, "missing-observations").is_some(), "{:?}", g.checks);
    let g = draw(
        &plain_spec(),
        &[json!({"rps":"1,000","p95":10}), json!({"rps":200,"p95":20})],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&plain_spec(), &[]);
    contract(&plain_spec(), &[json!({"rps":100,"p95":10})]);
    // Identical points sit on one another and stay individually reachable.
    let same = [json!({"rps":100,"p95":10}), json!({"rps":100,"p95":10})];
    contract(&plain_spec(), &same);
    assert_eq!(observations(&draw(&plain_spec(), &same)).len(), 2);
    contract(
        &plain_spec(),
        &[json!({"rps":-100,"p95":-10}), json!({"rps":50,"p95":-30})],
    );
    let g = draw(
        &plain_spec(),
        &[json!({"rps":1e300,"p95":-5e299}), json!({"rps":-1e300,"p95":5e299})],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn dense_clouds_keep_every_point_reachable() {
    let rows: Vec<Value> = (0..800)
        .map(|i| json!({"rps": (i * 37 % 211) as f64, "p95": (i * 53 % 197) as f64, "region": format!("r{}", i % 3)}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 800);
    assert_eq!(
        richview_chart::hit::order(&g).len(),
        800,
        "the keyboard visits every point"
    );
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("scatter", &spec(), &rows());
}

#[test]
fn the_runtime_keeps_values_a_grid_step_apart() {
    let rows = vec![
        json!({"rps":100.01,"p95":1000}),
        json!({"rps":1000,"p95":100.04}),
        json!({"rps":100.04,"p95":0.5}),
    ];
    runtime_matches_native(&plain_spec(), &rows);
}

#[test]
fn a_numeric_axis_label_becomes_a_named_heading_once() {
    let mut spec = plain_spec();
    spec["encoding"]["label"] = json!("rps");
    spec["axes"] = json!({"x":{"label":"Throughput","unit":"req/s"},"y":{"label":"Latency"}});
    let g = draw(&spec, &[json!({"rps":100,"p95":10})]);
    let o = observations(&g)[0];
    assert_eq!(o.readout.heading, "Throughput 100\u{a0}req/s");
    assert_eq!(line(o, "Throughput"), None);
    assert_eq!(line(o, "Latency"), Some("10"));
}
