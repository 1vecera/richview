//! Strip charts: every observation a point, the deterministic beeswarm, categories, groups,
//! orientation, facets, and the shared contract every type meets (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::hit;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/strip.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec(extra: Value) -> Value {
    let mut s = json!({"type":"strip","title":"T","encoding":{"y":"v"}});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn values(v: &[f64]) -> Vec<Value> {
    v.iter().map(|x| json!({ "v": x })).collect()
}

/// Centre and radius of every point mark, in paint order.
fn points(g: &Geometry) -> Vec<([f64; 2], f64)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Point {
                center,
                radius,
                role: Role::Mark,
                ..
            } => Some((*center, *radius)),
            _ => None,
        })
        .collect()
}

fn nearest(points: &[([f64; 2], f64)]) -> f64 {
    let mut least = f64::INFINITY;
    for (a, (p, _)) in points.iter().enumerate() {
        for (q, _) in &points[..a] {
            least = least.min((p[0] - q[0]).hypot(p[1] - q[1]));
        }
    }
    least
}

#[test]
fn the_golden_sample_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn every_row_is_a_point_and_overlaps_are_cleared() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let pts = points(&g);
    assert_eq!(pts.len(), rows.len());
    assert_eq!(observations(&g).len(), rows.len());
    let radius = pts[0].1;
    assert!(
        nearest(&pts) >= 2.0 * radius - 1e-6,
        "{} vs {}",
        nearest(&pts),
        2.0 * radius
    );
}

#[test]
fn identical_values_alternate_sides_by_one_diameter() {
    let g = draw(&spec(json!({})), &values(&[5.0, 5.0, 5.0, 5.0, 5.0]));
    let pts = points(&g);
    let plot = g.panels[0].plot;
    let centre = plot.x + plot.w / 2.0;
    let r = pts[0].1;
    let offsets: Vec<f64> = pts.iter().map(|p| p.0[0] - centre).collect();
    // Value order is row order for equal values: the first sits on the line, then the sides
    // alternate, each one diameter out.
    assert!(offsets[0].abs() < 1e-6, "{offsets:?}");
    assert!((offsets[1].abs() - 2.0 * r).abs() < 1e-6 && (offsets[2].abs() - 2.0 * r).abs() < 1e-6);
    assert!(offsets[1] * offsets[2] < 0.0, "{offsets:?}");
    assert!(
        pts.iter().all(|p| (p.0[1] - pts[0].0[1]).abs() < 1e-9),
        "same value, same height"
    );
}

#[test]
fn the_same_data_gives_the_same_picture() {
    let (spec, rows) = fixture();
    let (a, b) = (draw(&spec, &rows), draw(&spec, &rows));
    assert_eq!(points(&a), points(&b));
    assert_eq!(observations(&a), observations(&b));
}

#[test]
fn a_point_names_its_category_once_and_reads_its_value() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)[0];
    assert_eq!(o.row, Some(0));
    assert_eq!(o.readout.heading, "North");
    assert_eq!(line(o, "Score").unwrap(), "62");
    assert_eq!(line(o, "team"), None);
    assert_eq!(line(o, "Row"), None);
    assert!(o.readout.lines.iter().all(|l| !l.derived), "nothing here is computed");
    for o in observations(&g) {
        assert!(hit::at(&g, o.at).is_some(), "{:?} is reached", o.readout.heading);
    }
}

#[test]
fn each_category_keeps_to_its_own_band() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let plot = g.panels[0].plot;
    let band = plot.w / 3.0;
    for (o, row) in observations(&g).iter().zip(&rows) {
        let c = ["North", "South", "West"]
            .iter()
            .position(|n| *n == row["team"])
            .unwrap();
        let lo = plot.x + c as f64 * band;
        assert!(
            o.at[0] >= lo && o.at[0] <= lo + band,
            "{:?} outside its band",
            o.readout.heading
        );
    }
}

#[test]
fn groups_sit_side_by_side_within_a_category() {
    let rows: Vec<Value> = (0..16)
        .map(|i| json!({"v": (i % 8) as f64, "g": if i % 2 == 0 { "A" } else { "B" }}))
        .collect();
    let s = spec(json!({"encoding":{"y":"v","color":"g"}}));
    contract(&s, &rows);
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    let obs = observations(&g);
    let mean = |group: &str| {
        let xs: Vec<f64> = obs
            .iter()
            .filter(|o| o.readout.heading == group)
            .map(|o| o.at[0])
            .collect();
        xs.iter().sum::<f64>() / xs.len() as f64
    };
    assert!(mean("A") < mean("B"), "the first group is left of the second");
}

#[test]
fn the_value_axis_keeps_zero_only_for_values_close_to_it() {
    let baseline = |rows: &[Value]| {
        draw(&spec(json!({})), rows).panels[0]
            .items
            .iter()
            .any(|i| i.role() == Role::Baseline)
    };
    assert!(baseline(&values(&[0.0, 5.0, 9.0, 40.0])));
    assert!(!baseline(&values(&[1000.0, 1040.0, 1100.0])));
}

#[test]
fn horizontal_strips_put_the_value_on_the_x_axis() {
    let (mut spec, rows) = fixture();
    spec["style"] = json!({"orientation": "horizontal"});
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let first = obs[0].at[0];
    let last_in_north = obs[11].at[0];
    assert!(last_in_north > first, "a larger value lies further right");
}

#[test]
fn facets_make_one_panel_each() {
    let rows: Vec<Value> = (0..16)
        .map(|i| json!({"v": (i % 8) as f64, "p": if i < 8 { "left" } else { "right" }}))
        .collect();
    let s = spec(json!({"encoding":{"y":"v","facet":"p"}}));
    contract(&s, &rows);
    assert_eq!(draw(&s, &rows).panels.len(), 2);
}

#[test]
fn missing_values_are_disclosed_and_text_fails_the_panel() {
    let mut rows = values(&[1.0, 2.0, 4.0]);
    rows.push(json!({"v": null}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"v": "many"}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").unwrap().message.contains("Row 5"));
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let s = spec(json!({}));
    stays_visible(&s, &[]);
    contract(&s, &values(&[3.0]));
    assert_eq!(points(&draw(&s, &values(&[3.0]))).len(), 1);
    contract(&s, &values(&[2.0, 2.0, 2.0, 2.0]));
    contract(&s, &values(&[-4.0, 0.0, 9.0]));
    let g = draw(&s, &values(&[-1e300, 1e300]));
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let dense: Vec<f64> = (0..5000).map(|i| ((i * 7919) % 5003) as f64 / 7.0).collect();
    let rows = values(&dense);
    contract(&s, &rows);
    let g = draw_at(&s, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(points(&g).len(), 5000);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("strip", &spec, &rows);
    let two: Vec<Value> = (0..60)
        .map(|i| {
            let t = i as f64;
            let score = ((t * 7.0) % 23.0).round() + if i % 2 == 0 { 10.0 } else { 0.0 };
            let shift = if i % 2 == 0 { "Day" } else { "Night" };
            let team = ["North", "South", "West"][i % 3];
            json!({ "score": score, "team": team, "shift": shift })
        })
        .collect();
    let s = json!({"type":"strip","title":"Scores","encoding":{"x":"team","y":"score","color":"shift"}});
    gallery("strip-groups", &s, &two);
}

#[test]
fn values_closer_than_a_grid_step_keep_their_exact_order_and_spread_in_the_runtime() {
    // 100.04 comes before 100.01 in the rows and 100.01 twice: the grid ties them all, but the
    // runtime spreads the points the way the exact rows did.
    runtime_matches_native(&spec(json!({})), &values(&[500.0, 100.04, 100.01, 100.01, 0.0, 1000.0]));
}
