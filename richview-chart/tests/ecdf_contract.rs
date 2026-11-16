//! Empirical distribution charts: exact steps against hand-computed shares, counts, groups,
//! facets, and the shared contract every type meets (FEA-CH-14, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/ecdf.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec(extra: Value) -> Value {
    let mut s = json!({"type":"ecdf","title":"T","readout_digits":6,"encoding":{"x":"v"}});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn values(v: &[f64]) -> Vec<Value> {
    v.iter().map(|x| json!({ "v": x })).collect()
}

fn percent(text: &str) -> f64 {
    text.trim_end_matches('%').parse().unwrap_or_else(|_| panic!("{text}"))
}

/// The points of the first step line.
fn step_points(g: &Geometry) -> Vec<[f64; 2]> {
    let path = g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => Some(path),
            _ => None,
        })
        .expect("a step line");
    path.cmds
        .iter()
        .map(|c| match c {
            Cmd::M { p } | Cmd::L { p } => *p,
            other => panic!("a step line holds only straight segments, got {other:?}"),
        })
        .collect()
}

#[test]
fn the_golden_sample_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn shares_and_counts_match_the_hand_computed_values() {
    let g = draw(&spec(json!({})), &values(&[1.0, 2.0, 2.0, 3.0]));
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    let want = [("1", 25.0, "1"), ("2", 75.0, "2"), ("3", 100.0, "1")];
    for (o, (x, share, count)) in obs.iter().zip(want) {
        assert_eq!(o.readout.heading, x);
        assert_eq!(percent(line(o, "Share at or below").unwrap()), share);
        assert_eq!(line(o, "Observations").unwrap(), count);
        assert!(o.readout.lines.iter().all(|l| l.derived));
    }
}

#[test]
fn the_step_is_exact_with_each_jump_at_its_value() {
    let g = draw(&spec(json!({})), &values(&[1.0, 2.0, 2.0, 3.0]));
    let panel = &g.panels[0];
    let pts = step_points(&g);
    // Start, then a corner and a jump per distinct value, then the right edge.
    assert_eq!(pts.len(), 1 + 2 * 3 + 1);
    let x = panel.x.clone().unwrap();
    let px = |v: f64| x.range[0] + (v - x.domain[0]) / (x.domain[1] - x.domain[0]) * (x.range[1] - x.range[0]);
    for (k, v) in [1.0, 2.0, 3.0].into_iter().enumerate() {
        let (a, b) = (pts[1 + 2 * k], pts[2 + 2 * k]);
        assert!(
            (a[0] - px(v)).abs() < 1e-6 && (b[0] - px(v)).abs() < 1e-6,
            "{a:?} {b:?}"
        );
        assert!(b[1] < a[1], "a jump goes up the plot");
    }
    // The line starts on the zero baseline and ends at one hundred percent.
    assert!((pts[0][1] - panel.plot.bottom()).abs() < 1e-6);
    assert!((pts[pts.len() - 1][1] - panel.plot.y).abs() < 1e-6);
    // Between jumps it is flat.
    assert_eq!(pts[2][1], pts[3][1]);
}

#[test]
fn the_value_axis_reads_zero_to_one_hundred_percent() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let labels: Vec<String> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisLabel && t.text.ends_with('%') => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(labels.first().map(String::as_str), Some("0%"), "{labels:?}");
    assert!(labels.iter().any(|l| l == "100%"), "{labels:?}");
}

#[test]
fn groups_draw_one_step_line_each_and_read_out_together() {
    let rows: Vec<Value> = (0..12)
        .flat_map(|i| [json!({"v": i as f64, "g": "A"}), json!({"v": 5.0 + i as f64, "g": "B"})])
        .collect();
    let s = spec(json!({"encoding":{"x":"v","color":"g"}}));
    contract(&s, &rows);
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    let lines = g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Line { role: Role::Mark, .. }))
        .count();
    assert_eq!(lines, 2);
    let obs = observations(&g);
    // At 5, A has 6 of 12 at or below it and B has 1 of 12.
    let at5 = obs.iter().find(|o| o.readout.heading == "5").unwrap();
    assert_eq!(percent(line(at5, "A · Share at or below").unwrap()), 50.0);
    assert!((percent(line(at5, "B · Share at or below").unwrap()) - 100.0 / 12.0).abs() < 1e-4);
    assert!(g.legend.is_some());
}

#[test]
fn facets_make_one_panel_each() {
    let rows: Vec<Value> = (0..16)
        .map(|i| json!({"v": (i % 8) as f64 * 1.5, "p": if i < 8 { "left" } else { "right" }}))
        .collect();
    let s = spec(json!({"encoding":{"x":"v","facet":"p"}}));
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
    // One value is a single jump from zero to one hundred percent.
    contract(&s, &values(&[3.0]));
    let g = draw(&s, &values(&[3.0]));
    assert_eq!(percent(line(observations(&g)[0], "Share at or below").unwrap()), 100.0);
    // Identical values are one jump with their count.
    contract(&s, &values(&[2.0, 2.0, 2.0]));
    let g = draw(&s, &values(&[2.0, 2.0, 2.0]));
    assert_eq!(observations(&g).len(), 1);
    assert_eq!(line(observations(&g)[0], "Observations").unwrap(), "3");
    contract(&s, &values(&[-4.0, 0.0, 9.0]));
    let g = draw(&s, &values(&[-1e300, 1e300]));
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let dense: Vec<f64> = (0..5000).map(|i| ((i * 7919) % 5003) as f64 / 7.0).collect();
    let rows = values(&dense);
    contract(&s, &rows);
    let g = draw_at(&s, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 5000);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("ecdf", &spec, &rows);
    let two: Vec<Value> = (0..40)
        .map(|i| {
            let t = i as f64;
            if i % 2 == 0 {
                json!({"minutes": 4.0 + (t * 0.37).sin() * 2.0 + t * 0.1, "who": "Support"})
            } else {
                json!({"minutes": 9.0 + (t * 0.53).cos() * 3.0 + t * 0.15, "who": "Sales"})
            }
        })
        .collect();
    let s = json!({"type":"ecdf","title":"Replies","encoding":{"x":"minutes","color":"who"}});
    gallery("ecdf-groups", &s, &two);
}

#[test]
fn values_closer_than_a_grid_step_keep_their_own_steps_in_the_runtime() {
    // 100.01 and 100.04 land on one grid value in a 0 to 1000 range; the runtime still draws the
    // two steps the exact rows drew.
    runtime_matches_native(&spec(json!({})), &values(&[0.0, 100.01, 100.04, 500.0, 1000.0]));
}
