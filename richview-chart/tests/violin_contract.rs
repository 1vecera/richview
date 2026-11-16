//! Violin charts: kernel-density shapes mirrored about each category, the quartiles and median
//! inside, one density scale, edge samples and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/violin.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"violin","title":"T","encoding":{"x":"k","y":"v"},"axes":{"y":{"label":"Value"}}})
}

fn sample(key: &str, values: &[f64]) -> Vec<Value> {
    values.iter().map(|v| json!({"k": key, "v": v})).collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("violin", &spec, &rows);
}

#[test]
fn each_category_has_one_shape_with_its_counts_and_quartiles() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    assert_eq!(obs[0].readout.heading, "Standard");
    assert_eq!(line(obs[0], "Observations"), Some("24"));
    let standard: Vec<f64> = rows
        .iter()
        .filter(|r| r["plan"] == "Standard")
        .map(|r| r["score"].as_f64().unwrap())
        .collect();
    let max = standard.iter().cloned().fold(f64::MIN, f64::max);
    assert_eq!(line(obs[0], "Maximum").unwrap().parse::<f64>().unwrap(), max);
    for name in ["Median", "Upper quartile", "Lower quartile"] {
        assert!(obs[0].readout.lines.iter().find(|l| l.name == name).unwrap().derived);
    }
    // Shapes are drawn as areas owned by their observation.
    let shapes = g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Area { obs: Some(_), .. }))
        .count();
    assert_eq!(shapes, 3);
}

#[test]
fn the_shape_is_mirrored_and_stays_within_the_data_range() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)[1];
    let area = o.area.unwrap();
    // Centre line of the violin is the middle of its box, and the median sits on it.
    assert!((o.at[0] - area.center()[0]).abs() < 0.01);
    // The box spans the smallest to the largest value of its sample: the maximum is higher up.
    let flex: Vec<f64> = rows
        .iter()
        .filter(|r| r["plan"] == "Flex")
        .map(|r| r["score"].as_f64().unwrap())
        .collect();
    let (lo, hi) = flex
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let frame = &g.panels[0];
    let span = frame.plot.h;
    assert!(area.h > 0.0 && area.h <= span + 0.01);
    let ratio = (hi - lo) / area.h;
    assert!(ratio.is_finite() && ratio > 0.0);
    // The median lies between the shape's top and bottom.
    assert!(o.at[1] > area.y && o.at[1] < area.bottom());
}

#[test]
fn one_density_scale_gives_equal_samples_equal_areas() {
    // A narrow sample is taller and thinner than a wide one of the same size.
    let mut rows = sample("narrow", &[10.0, 10.5, 11.0, 10.2, 10.8, 10.4, 10.6, 10.9]);
    rows.extend(sample("wide", &[0.0, 5.0, 10.0, 15.0, 20.0, 2.0, 12.0, 18.0]));
    let g = draw(&spec(), &rows);
    let obs = observations(&g);
    assert!(
        obs[0].area.unwrap().w > obs[1].area.unwrap().w,
        "the concentrated sample is wider"
    );
    contract(&spec(), &rows);
}

#[test]
fn groups_sit_side_by_side_in_a_category() {
    let mut s = spec();
    s["encoding"]["color"] = json!("g");
    let mut rows = Vec::new();
    for k in ["x", "y"] {
        for (g, shift) in [("A", 0.0), ("B", 3.0)] {
            for i in 0..8 {
                rows.push(json!({"k": k, "g": g, "v": shift + i as f64 * 0.7 }));
            }
        }
    }
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(obs[0].readout.heading, "x · A");
    assert!(obs[0].at[0] < obs[1].at[0] && obs[1].at[0] < obs[2].at[0]);
    contract(&s, &rows);
}

#[test]
fn a_single_value_or_equal_values_are_a_dot_not_an_invented_shape() {
    let one = sample("only", &[4.0]);
    let g = draw(&spec(), &one);
    let o = observations(&g)[0];
    assert!(o.area.is_none() && o.radius > 0.0);
    assert_eq!(line(o, "Value"), Some("4"));
    assert!(
        g.panels[0]
            .items
            .iter()
            .all(|i| !matches!(i, Item::Area { role: Role::Mark, .. }))
    );
    contract(&spec(), &one);
    contract(&spec(), &sample("same", &[3.0, 3.0, 3.0]));
    let mut mixed = sample("a", &[1.0, 2.0, 3.0, 4.0]);
    mixed.extend(sample("b", &[9.0]));
    contract(&spec(), &mixed);
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = sample("a", &[1.0, 2.0, 3.0, 4.0]);
    rows.push(json!({"k":"a","v":null}));
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(line(observations(&g)[0], "Observations"), Some("4"));
    rows.push(json!({"k":"a","v":"many"}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"k":"a","v":null})]);
}

#[test]
fn extreme_ranges_stay_finite() {
    let rows = sample("a", &[-1e300, 5e299, 1e300, 0.0, 1e299]);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(nonfinite(&g).is_none());
    contract(&spec(), &rows);
    let tiny = sample("a", &[1e-300, 2e-300, 3e-300, 5e-300]);
    let g = draw(&spec(), &tiny);
    assert!(nonfinite(&g).is_none());
}

#[test]
fn horizontal_violins_run_along_x() {
    let (mut s, rows) = fixture();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &rows);
    let obs = observations(&g);
    assert!(obs[0].at[1] < obs[1].at[1] && obs[1].at[1] < obs[2].at[1]);
    contract(&s, &rows);
}

#[test]
fn two_values_closer_than_a_grid_step_keep_their_shape_in_the_runtime() {
    // 100.01 and 100.04 land on one grid value in a 0 to 1000 range; the runtime still draws the
    // shape the exact rows drew, not a dot.
    let mut rows = sample("close", &[100.01, 100.04]);
    rows.extend(sample("wide", &[0.0, 250.0, 500.0, 750.0, 1000.0]));
    runtime_matches_native(&spec(), &rows);
}
