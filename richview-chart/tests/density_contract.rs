//! Density charts: the kernel estimate against hand-computed references, the method disclosed,
//! groups, facets, and the shared contract every type meets (FEA-CH-14, FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Cmd, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/density.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec(extra: Value) -> Value {
    let mut s = json!({"type":"density","title":"T","readout_digits":6,"encoding":{"x":"v"}});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn values(v: &[f64]) -> Vec<Value> {
    v.iter().map(|x| json!({ "v": x })).collect()
}

fn number(text: &str) -> f64 {
    text.replace('\u{2212}', "-")
        .parse()
        .unwrap_or_else(|_| panic!("{text} is not a number"))
}

/// The first number in `text` after `marker`.
fn after(text: &str, marker: &str) -> f64 {
    let rest = &text[text.find(marker).unwrap_or_else(|| panic!("{marker} in {text}")) + marker.len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '\u{2212}' || c == 'e'))
        .unwrap_or(rest.len());
    number(&rest[..end])
}

fn areas(g: &richview_chart::geometry::Geometry) -> usize {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Area { role: Role::Band, .. }))
        .count()
}

#[test]
fn the_golden_sample_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn the_density_at_a_point_matches_the_hand_computed_value() {
    // Points 0 and 1 with h = 0.5 span [-1.5, 2.5]; the ninth of 17 positions is x = 0.5, where
    // each point contributes phi(1) / (n h) = 0.2419707 / 1.
    let g = draw(&spec(json!({"style":{"bandwidth":0.5}})), &values(&[0.0, 1.0]));
    let obs = observations(&g);
    assert_eq!(obs.len(), 17);
    assert_eq!(number(&obs[8].readout.heading), 0.5);
    let d = number(line(obs[8], "Density").unwrap());
    assert!((d - 2.0 * 0.241_970_7).abs() < 1e-6, "{d}");
    assert!(obs[8].readout.lines.iter().all(|l| l.derived));
    // The ends of the range are three bandwidths from the data.
    assert_eq!(number(&obs[0].readout.heading), -1.5);
    assert_eq!(number(&obs[16].readout.heading), 2.5);
    let note = g.note.as_deref().unwrap();
    assert!(note.contains("Gaussian kernel"), "{note}");
    assert!(note.contains("bandwidth h = 0.5"), "{note}");
}

#[test]
fn silverman_sets_the_bandwidth_and_the_note_says_so() {
    // 1..=5: sd 1.58, IQR 2, so 0.9 * (2 / 1.34) * 5^(-1/5).
    let g = draw(&spec(json!({})), &values(&[1.0, 2.0, 3.0, 4.0, 5.0]));
    let want = 0.9 * (2.0 / 1.34) * 5f64.powf(-0.2);
    let note = g.note.as_deref().unwrap();
    assert!((after(note, "h = ") - want).abs() < 1e-5, "{note}");
    assert!(note.contains("Silverman"), "{note}");
    // The estimate is symmetric about the centre of a symmetric sample.
    let obs = observations(&g);
    let (a, b) = (
        obs[5].readout.lines[0].value.clone(),
        obs[11].readout.lines[0].value.clone(),
    );
    assert!((number(&a) - number(&b)).abs() < 1e-5, "{a} {b}");
}

#[test]
fn the_curve_is_drawn_on_a_128_point_grid_from_zero() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let panel = &g.panels[0];
    let outline = panel
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => Some(path),
            _ => None,
        })
        .unwrap();
    assert_eq!(outline.cmds.len(), 128);
    assert_eq!(areas(&g), 1);
    // The area closes on the baseline, which sits at the bottom of the plot.
    let Some(Item::Area { path, .. }) = panel.items.iter().find(|i| matches!(i, Item::Area { .. })) else {
        panic!("no area")
    };
    let Cmd::M { p } = path.cmds[0] else {
        panic!("starts with a move")
    };
    assert!((p[1] - panel.plot.bottom()).abs() < 1e-6, "{p:?} {:?}", panel.plot);
    assert!(panel.items.iter().any(|i| i.role() == Role::Baseline));
}

#[test]
fn groups_draw_one_curve_each_and_read_out_together() {
    let rows: Vec<Value> = (0..20)
        .flat_map(|i| {
            [
                json!({"v": i as f64 * 0.5, "g": "A"}),
                json!({"v": 3.0 + i as f64 * 0.7, "g": "B"}),
            ]
        })
        .collect();
    let s = spec(json!({"encoding":{"x":"v","color":"g"}}));
    contract(&s, &rows);
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    assert_eq!(areas(&g), 2);
    let obs = observations(&g);
    assert!(
        obs.iter()
            .any(|o| line(o, "A · Density").is_some() && line(o, "B · Density").is_some())
    );
    assert!(g.legend.is_some());
}

#[test]
fn facets_make_one_panel_each() {
    let rows: Vec<Value> = (0..16)
        .map(|i| json!({"v": (i % 8) as f64 + (i / 8) as f64, "p": if i < 8 { "left" } else { "right" }}))
        .collect();
    let s = spec(json!({"encoding":{"x":"v","facet":"p"}}));
    contract(&s, &rows);
    assert_eq!(draw(&s, &rows).panels.len(), 2);
}

#[test]
fn a_group_with_one_distinct_value_is_reported_and_not_drawn() {
    let mut rows = values(&[1.0, 2.0, 3.0, 4.0]);
    for r in &mut rows {
        r["g"] = json!("A");
    }
    rows.push(json!({"v": 7.0, "g": "B"}));
    rows.push(json!({"v": 7.0, "g": "B"}));
    let g = draw(&spec(json!({"encoding":{"x":"v","color":"g"}})), &rows);
    assert!(g.failure.is_none());
    assert_eq!(areas(&g), 1);
    let c = check(&g, "too-few-observations").unwrap();
    assert!(c.message.contains("'B'"), "{}", c.message);
    // When nothing is left the panel says why.
    let g = draw(&spec(json!({})), &values(&[5.0, 5.0, 5.0]));
    assert!(g.failure.as_deref().unwrap().contains("two distinct"));
    assert!(check(&g, "too-few-observations").is_some());
}

#[test]
fn missing_values_are_disclosed_and_text_fails_the_panel() {
    let mut rows = values(&[1.0, 2.0, 4.0, 8.0]);
    rows.push(json!({"v": null}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"v": "many"}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").unwrap().message.contains("Row 6"));
}

#[test]
fn a_bad_bandwidth_falls_back_to_the_rule_and_says_so() {
    let g = draw(&spec(json!({"style":{"bandwidth":-1}})), &values(&[1.0, 2.0, 4.0]));
    assert!(g.failure.is_none());
    assert!(check(&g, "style-invalid").unwrap().message.contains("bandwidth"));
    assert!(g.note.as_deref().unwrap().contains("Silverman"));
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let s = spec(json!({}));
    stays_visible(&s, &[]);
    stays_visible(&s, &values(&[3.0]));
    contract(&s, &values(&[1.0, 2.0]));
    let g = draw(&s, &values(&[-1e300, 1e300]));
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let g = draw(&s, &values(&[1e-300, 3e-300, 2e-300]));
    assert!(g.failure.is_none() && nonfinite(&g).is_none(), "{:?}", g.failure);
    let dense: Vec<f64> = (0..5000).map(|i| ((i * 7919) % 1000) as f64 / 10.0).collect();
    let rows = values(&dense);
    contract(&s, &rows);
    let g = draw_at(&s, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 17);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("density", &spec, &rows);
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
    let s = json!({"type":"density","title":"Replies","encoding":{"x":"minutes","color":"who"}});
    gallery("density-groups", &s, &two);
}

#[test]
fn a_group_whose_values_the_grid_merges_keeps_its_curve_in_the_runtime() {
    // 100.01 and 100.04 land on one grid value in a 0 to 1000 range; the runtime still draws the
    // curve the exact rows drew, not "fewer than 2 distinct values".
    let mut rows: Vec<Value> = [100.01, 100.04].iter().map(|v| json!({"v": v, "g": "close"})).collect();
    rows.extend(
        [0.0, 250.0, 500.0, 750.0, 1000.0]
            .iter()
            .map(|v| json!({"v": v, "g": "wide"})),
    );
    let spec = spec(json!({"encoding": {"x": "v", "color": "g"}}));
    runtime_matches_native(&spec, &rows);
}
