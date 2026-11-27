//! Gauge charts: one value on an arc, bands and a needle or a filled arc, a key that names the
//! bands, and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/gauge.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"gauge","title":"T","encoding":{"y":"v"},"style":{"min":0,"max":100}})
}

fn areas(g: &Geometry) -> usize {
    marks(g).iter().filter(|i| matches!(i, Item::Area { .. })).count()
}

fn key_texts(g: &Geometry) -> Vec<String> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Key => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn one_observation_reads_the_value_the_band_and_the_scale() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0].readout.heading, "Satisfaction");
    assert_eq!(line(obs[0], "Score"), Some("72"));
    assert_eq!(line(obs[0], "Band"), Some("Fair"));
    assert_eq!(line(obs[0], "Scale"), Some("0 to 100"));
    let band = obs[0].readout.lines.iter().find(|l| l.name == "Band").unwrap();
    assert!(band.derived);
    assert!(!obs[0].readout.lines[0].derived);
}

#[test]
fn bands_split_the_arc_and_a_key_names_each() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    // Three bands and the needle are filled marks.
    assert_eq!(areas(&g), 4);
    assert_eq!(key_texts(&g), vec!["Poor", "Fair", "Good"]);
}

#[test]
fn a_band_without_a_label_is_named_by_its_range() {
    let (mut spec, rows) = fixture();
    spec["style"]["bands"] = json!([{"to": 40}, {"to": 100}]);
    let g = draw(&spec, &rows);
    assert_eq!(key_texts(&g), vec!["0–40", "40–100"]);
    assert_eq!(line(observations(&g)[0], "Band"), Some("40–100"));
}

#[test]
fn without_bands_the_arc_fills_to_the_value() {
    let rows = [json!({"v": 40})];
    let g = draw(&plain(), &rows);
    assert_eq!(areas(&g), 1);
    assert!(key_texts(&g).is_empty());
    assert!(line(observations(&g)[0], "Band").is_none());
    // A full gauge fills the whole track.
    let full = draw(&plain(), &[json!({"v": 100})]);
    let track = full.panels[0]
        .items
        .iter()
        .find(|i| i.role() == Role::Band)
        .and_then(|i| i.bounds())
        .unwrap();
    let arc = marks(&full)[0].bounds().unwrap();
    assert!((track.w - arc.w).abs() < 1e-9 && (track.h - arc.h).abs() < 1e-9);
    // An empty one has no filled arc.
    assert_eq!(areas(&draw(&plain(), &[json!({"v": 0})])), 0);
}

#[test]
fn the_scale_follows_the_value_when_nothing_sets_it() {
    let spec = json!({"type":"gauge","title":"T","encoding":{"y":"v"}});
    let g = draw(&spec, &[json!({"v": 70})]);
    assert_eq!(line(observations(&g)[0], "Scale"), Some("0 to 100"));
    contract(&spec, &[json!({"v": 70})]);
}

#[test]
fn a_value_beyond_the_scale_is_drawn_at_the_edge_and_counted() {
    let g = draw(&plain(), &[json!({"v": 140})]);
    assert!(check(&g, "value-outside-scale").is_some());
    assert_eq!(line(observations(&g)[0], "v"), Some("140"));
    let below = draw(&plain(), &[json!({"v": -5})]);
    assert!(check(&below, "value-outside-scale").is_some());
    assert_eq!(areas(&below), 0);
}

#[test]
fn bands_that_do_not_ascend_are_ignored_with_a_finding() {
    let (mut spec, rows) = fixture();
    spec["style"]["bands"] = json!([{"to": 60}, {"to": 30}]);
    let g = draw(&spec, &rows);
    assert!(check(&g, "style-invalid").unwrap().message.contains("band 2"));
    assert!(key_texts(&g).is_empty());
    spec["style"]["bands"] = json!("red");
    assert!(check(&draw(&spec, &rows), "style-invalid").is_some());
}

#[test]
fn a_panel_with_several_rows_fails_and_facets_give_each_a_gauge() {
    let spec = plain();
    let g = draw(&spec, &[json!({"v": 10}), json!({"v": 20})]);
    assert!(g.failure.as_deref().unwrap().contains("one value"));
    assert!(check(&g, "source-error").is_some());
    let spec = json!({"type":"gauge","title":"T","encoding":{"y":"v","facet":"f"},"style":{"max":100}});
    let rows = [json!({"v": 10, "f": "North"}), json!({"v": 90, "f": "South"})];
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(observations(&g).len(), 2);
    contract(&spec, &rows);
}

#[test]
fn a_scale_that_cannot_be_drawn_says_why() {
    let mut spec = plain();
    spec["style"] = json!({"min": 50, "max": 50});
    let g = draw(&spec, &[json!({"v": 50})]);
    assert!(g.failure.as_deref().unwrap().contains("style.min"));
    spec["style"] = json!({"min": -1e308, "max": 1e308});
    let g = draw(&spec, &[json!({"v": 1})]);
    assert!(g.failure.is_some() && nonfinite(&g).is_none());
}

#[test]
fn text_that_is_not_a_number_fails_and_a_missing_value_is_never_zero() {
    let g = draw(&plain(), &[json!({"v": "high"})]);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let g = draw(&plain(), &[json!({"v": Value::Null})]);
    assert!(g.failure.as_deref().unwrap().contains("No row has a value"));
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows);
    for v in [0.0, 100.0, 1e-9, 99.999] {
        let g = draw(&spec, &[json!({"measure": "A", "score": v})]);
        assert!(g.failure.is_none() && nonfinite(&g).is_none(), "{v}");
    }
    let huge = draw(
        &json!({"type":"gauge","title":"T","encoding":{"y":"v"}}),
        &[json!({"v": 1e300})],
    );
    assert!(huge.failure.is_none() && nonfinite(&huge).is_none());
    let long = [json!({"measure": "A measure with an excessively long name that cannot fit", "score": 5})];
    let g = draw_at(&spec, &long, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert!(check(&g, "clipping").is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("gauge", &spec, &rows);
}

#[test]
fn a_value_beyond_every_band_names_no_band() {
    let mut spec = plain();
    spec["style"]["bands"] = json!([{"to": 40, "label": "Low"}, {"to": 70, "label": "Mid"}]);
    let band = |spec: &Value, v: f64| {
        let g = draw(spec, &[json!({"v": v})]);
        line(observations(&g)[0], "Band").map(str::to_owned)
    };
    assert_eq!(band(&spec, 85.0).as_deref(), Some("Above the last band"));
    assert_eq!(band(&spec, 70.0).as_deref(), Some("Mid"));
    assert_eq!(band(&spec, 20.0).as_deref(), Some("Low"));
    assert_eq!(band(&spec, 130.0).as_deref(), Some("Above the last band"));
    spec["style"]["min"] = json!(10);
    assert_eq!(band(&spec, 5.0).as_deref(), Some("Below the first band"));
}

/// The canvas check reads a path's extent from its end points (DEL-72), so an arc must have its
/// extremes there: the painted extent equals the end-point extent.
#[test]
fn arcs_have_their_extremes_at_end_points() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 390.0);
    for item in g.items() {
        if let Item::Area { path, .. } | Item::Line { path, .. } = item {
            let ends: Vec<[f64; 2]> = path
                .cmds
                .iter()
                .filter_map(|c| match c {
                    richview_chart::geometry::Cmd::M { p }
                    | richview_chart::geometry::Cmd::L { p }
                    | richview_chart::geometry::Cmd::C { p, .. } => Some(*p),
                    richview_chart::geometry::Cmd::Z => None,
                })
                .collect();
            let drawn: Vec<[f64; 2]> = path.flatten(0.25).into_iter().flatten().collect();
            let extent = |pts: &[[f64; 2]], axis: usize| {
                pts.iter()
                    .fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])))
            };
            for axis in 0..2 {
                let (a, b) = (extent(&ends, axis), extent(&drawn, axis));
                assert!(
                    (a.0 - b.0).abs() < 0.3 && (a.1 - b.1).abs() < 0.3,
                    "axis {axis}: end points {a:?} against drawn {b:?}"
                );
            }
        }
    }
}
