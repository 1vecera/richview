//! Rose charts: wedges at equal angles whose radius (or area) follows the value from a zero
//! centre, and the shared contract every type meets (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Targeting};
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/rose.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn wedge_of<'a>(g: &'a Geometry, heading: &str) -> &'a richview_chart::geometry::Wedge {
    observations(g)
        .into_iter()
        .find(|o| o.readout.heading == heading)
        .and_then(|o| o.wedge.as_ref())
        .unwrap()
}

#[test]
fn the_golden_fixture_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn wedges_sit_at_equal_angles_clockwise_and_say_which_scale_they_use() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.panels[0].targeting, Targeting::Contain);
    let sector = TAU / 12.0;
    for (i, month) in ["Jan", "Feb", "Mar", "Dec"].iter().enumerate() {
        let i = if *month == "Dec" { 11 } else { i };
        let w = wedge_of(&g, month);
        let mid = (w.start + w.end) / 2.0;
        assert!((mid - (i as f64 + 0.5) * sector).abs() < 1e-9, "{month}");
        assert!(w.end - w.start < sector, "wedges leave a gap between them");
        assert_eq!(w.inner, 0.0);
    }
    assert!(g.note.as_deref().unwrap().contains("radius is proportional"));
}

#[test]
fn radius_is_linear_in_the_value_by_default() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let (jul, feb) = (wedge_of(&g, "Jul").radius, wedge_of(&g, "Feb").radius);
    assert!((jul / feb - 110.0 / 25.0).abs() < 1e-9, "{}", jul / feb);
    // The longest wedge reaches the outer ring and never beyond the plot.
    assert!(jul <= g.panels[0].plot.w / 2.0);
}

#[test]
fn area_scale_makes_area_proportional() {
    let (mut spec, rows) = fixture();
    spec["style"] = json!({"scale": "area"});
    let g = draw(&spec, &rows);
    let (jul, feb) = (wedge_of(&g, "Jul").radius, wedge_of(&g, "Feb").radius);
    assert!(((jul / feb).powi(2) - 110.0 / 25.0).abs() < 1e-9);
    assert!(g.note.as_deref().unwrap().contains("area is proportional"));
    contract(&spec, &rows);
}

#[test]
fn readouts_head_with_the_category_and_give_the_value() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Jul")
        .unwrap();
    assert_eq!(line(o, "Rainfall (mm)"), Some("110"));
    assert_eq!(observations(&g).len(), 12);
    // One wedge Item::Area per category, each tied to its observation.
    let wedges = marks(&g)
        .into_iter()
        .filter(|i| matches!(i, Item::Area { obs: Some(_), .. }))
        .count();
    assert_eq!(wedges, 12);
}

#[test]
fn groups_share_a_sector_side_by_side() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["color"] = json!("year");
    let rows: Vec<Value> = rows
        .iter()
        .flat_map(|r| {
            [("2025", 1.0), ("2026", 0.5)].iter().map(move |(y, k)| {
                let mut r = r.clone();
                r["year"] = json!(y);
                r["mm"] = json!(r["mm"].as_f64().unwrap() * k);
                r
            })
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 24);
    assert!(g.legend.is_some());
    let jan: Vec<_> = observations(&g)
        .into_iter()
        .filter(|o| o.readout.heading == "Jan")
        .collect();
    let (a, b) = (jan[0].wedge.unwrap(), jan[1].wedge.unwrap());
    assert!(a.end <= b.start, "side by side, not overlapping");
    assert!(a.radius > b.radius);
    assert!(a.start >= 0.0 && b.end <= TAU / 12.0);
}

#[test]
fn negative_values_fail_with_a_source_error() {
    let (spec, mut rows) = fixture();
    rows[3]["mm"] = json!(-5);
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("negative"));
    let c = check(&g, "source-error").unwrap();
    assert!(c.message.contains("negative") && c.path.as_deref() == Some("rows[3].mm"));
}

#[test]
fn missing_text_and_repeats_are_disclosed_or_fail() {
    let (spec, mut rows) = fixture();
    rows[0]["mm"] = json!(null);
    let g = draw(&spec, &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 11);
    contract(&spec, &rows);
    let (_, mut rows) = fixture();
    rows[0]["mm"] = json!("lots");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
    let (_, mut rows) = fixture();
    rows[0]["mm"] = json!("1e-400");
    assert!(
        check(&draw(&spec, &rows), "source-error")
            .unwrap()
            .message
            .contains("too small")
    );
    let (_, mut rows) = fixture();
    rows.push(json!({"month": "Jan", "mm": 3}));
    assert!(
        check(&draw(&spec, &rows), "source-error")
            .unwrap()
            .message
            .contains("repeats")
    );
}

#[test]
fn a_value_beyond_the_authored_scale_is_drawn_at_the_edge_and_reported() {
    let (mut spec, rows) = fixture();
    spec["axes"]["y"]["max"] = json!(60);
    let g = draw(&spec, &rows);
    assert!(check(&g, "value-outside-scale").is_some());
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    // One category fills the circle; two and three split it; zeros draw no wedge.
    contract(&spec, &[json!({"month": "Only", "mm": 5})]);
    contract(&spec, &rows[..2]);
    contract(&spec, &rows[..3]);
    contract(&spec, &[json!({"month": "A", "mm": 0}), json!({"month": "B", "mm": 3})]);
    let zeros = [json!({"month": "A", "mm": 0}), json!({"month": "B", "mm": 0})];
    contract(&spec, &zeros);
    assert_eq!(marks(&draw(&spec, &zeros)).len(), 0);
    let huge = [json!({"month": "A", "mm": 1e300}), json!({"month": "B", "mm": 5e299})];
    assert!(nonfinite(&draw(&spec, &huge)).is_none());
    // A tiny value beside a large one stays a wedge, not a hole.
    let tiny = [json!({"month": "A", "mm": 1000}), json!({"month": "B", "mm": 0.01})];
    contract(&spec, &tiny);
    // Many categories with long names drop what cannot fit and report it.
    let dense: Vec<Value> = (0..60)
        .map(|i| json!({"month": format!("Category number {i}"), "mm": (i * 7) % 31 + 1}))
        .collect();
    for width in WIDTHS {
        let g = draw_at(&spec, &dense, width);
        assert!(g.failure.is_none() && nonfinite(&g).is_none());
        assert!(check(&g, "clipping").is_none() && check(&g, "label-collision").is_none());
        assert_eq!(observations(&g).len(), 60);
    }
    let mut area = spec.clone();
    area["style"] = json!({"scale": "area"});
    contract(&area, &dense);
}

#[test]
fn facets_draw_one_rose_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("site");
    let rows: Vec<Value> = rows
        .iter()
        .flat_map(|r| {
            ["North", "South"].iter().map(move |s| {
                let mut r = r.clone();
                r["site"] = json!(s);
                r
            })
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[1].observations.len(), 12);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("rose", &spec, &rows);
    let mut area = spec.clone();
    area["style"] = json!({"scale": "area"});
    gallery("rose-area", &area, &rows);
    let mut grouped = spec.clone();
    grouped["encoding"]["color"] = json!("year");
    let many: Vec<Value> = rows
        .iter()
        .take(6)
        .flat_map(|r| {
            [("2025", 1.0), ("2026", 0.6)]
                .iter()
                .map(move |(y, k)| json!({"month": r["month"], "year": y, "mm": r["mm"].as_f64().unwrap() * k}))
        })
        .collect();
    gallery("rose-groups", &grouped, &many);
}

#[test]
fn the_runtime_draws_the_wedges_the_exact_rows_draw() {
    // 0.05 beside 1000 is zero on the grid but still has a wedge.
    let (spec, _) = fixture();
    let rows: Vec<Value> = ["a", "b", "c", "d"]
        .iter()
        .zip([1000.0, 0.05, 100.01, 100.04])
        .map(|(c, v)| {
            let mut r = serde_json::Map::new();
            r.insert(spec["encoding"]["x"].as_str().unwrap_or("x").to_owned(), json!(c));
            r.insert(spec["encoding"]["y"].as_str().unwrap_or("y").to_owned(), json!(v));
            Value::Object(r)
        })
        .collect();
    runtime_matches_native(&spec, &rows);
}
