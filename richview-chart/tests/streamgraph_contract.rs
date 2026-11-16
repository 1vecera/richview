//! Streamgraphs: streams stacked around a centre line in legend order, a scale bar in place of a
//! value axis, a name inside each stream that has room, gaps disclosed and the failures that name
//! their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/streamgraph.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"streamgraph","title":"T","encoding":{"x":"x","y":"v","color":"g"}})
}

fn streams(g: &Geometry) -> usize {
    marks(g).iter().filter(|i| matches!(i, Item::Area { .. })).count()
}

fn texts(g: &Geometry, role: Role) -> Vec<(String, f64)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == role => Some((t.text.clone(), t.y)),
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
fn one_stream_per_group_and_one_observation_per_month() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(streams(&g), 3);
    let obs = observations(&g);
    assert_eq!(obs.len(), 12);
    assert_eq!(obs[0].readout.heading, "Jan 2025");
    assert_eq!(line(obs[0], "Rock"), Some("40"));
    let total = obs[0].readout.lines.iter().find(|l| l.name == "Total").unwrap();
    assert!(total.derived);
    assert_eq!(obs[0].dots.len(), 3);
    assert!(g.legend.is_some());
}

#[test]
fn the_stack_is_centred_on_the_middle_in_legend_order() {
    let spec = plain();
    let rows = [
        json!({"x": 1, "g": "A", "v": 2}),
        json!({"x": 1, "g": "B", "v": 4}),
        json!({"x": 1, "g": "C", "v": 2}),
        json!({"x": 2, "g": "A", "v": 2}),
        json!({"x": 2, "g": "B", "v": 4}),
        json!({"x": 2, "g": "C", "v": 2}),
    ];
    let g = draw(&spec, &rows);
    let o = observations(&g)[0];
    let (a, b, c) = (o.dots[0].at[1], o.dots[1].at[1], o.dots[2].at[1]);
    let plot = g.panels[0].plot;
    // The first group is the lowest stream, and the middle one sits on the centre.
    assert!(a > b && b > c);
    assert!((b - (plot.y + plot.h / 2.0)).abs() < 2.0, "{b} vs {:?}", plot);
    assert!(((a - b) - (b - c)).abs() < 1e-6);
}

#[test]
fn the_value_axis_gives_way_to_a_scale_bar() {
    use richview_chart::geometry::{Anchor, Face};
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 720.0);
    let plot = g.panels[0].plot;
    // No tick label stands at the left of the plot, and no gridline crosses it.
    assert!(texts(&g, Role::AxisLabel).iter().all(|(_, y)| *y > plot.bottom()));
    assert!(
        g.panels[0]
            .items
            .iter()
            .all(|i| i.role() != Role::Grid && i.role() != Role::Baseline)
    );
    let bar = g.panels[0]
        .items
        .iter()
        .filter(|i| i.role() == Role::Annotation)
        .count();
    assert_eq!(bar, 3);
    let calipers: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Text(t) if t.face == Face::Figures && t.anchor == Anchor::Start && t.x == g.panels[0].frame.x => {
                Some(t)
            }
            _ => None,
        })
        .collect();
    assert_eq!(calipers.len(), 1, "the thickness scale has one numeric key");
    assert_eq!(
        calipers[0].role,
        Role::Key,
        "a scale step is not an invariant data label"
    );
}

#[test]
fn streams_are_named_inside_when_the_name_fits() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let names: Vec<String> = texts(&g, Role::Label).into_iter().map(|(t, _)| t).collect();
    for want in ["Rock", "Jazz", "Pop"] {
        assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
    }
}

#[test]
fn a_group_missing_at_an_x_is_drawn_at_zero_and_disclosed() {
    let (spec, mut rows) = fixture();
    rows.remove(5);
    let g = draw(&spec, &rows);
    assert!(check(&g, "missing-stream-values").is_some());
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Jun 2025")
        .unwrap();
    assert_eq!(line(o, "Rock"), Some("No observation"));
    assert_eq!(o.dots.len(), 2);
}

#[test]
fn one_x_draws_each_stream_as_a_bar() {
    let spec = plain();
    let rows = [json!({"x": 1, "g": "A", "v": 3}), json!({"x": 1, "g": "B", "v": 5})];
    let g = draw(&spec, &rows);
    let bars = marks(&g).iter().filter(|i| matches!(i, Item::Rect { .. })).count();
    assert_eq!(bars, 2);
    contract(&spec, &rows);
}

#[test]
fn negative_values_repeats_and_text_fail_the_panel() {
    let (spec, mut rows) = fixture();
    rows[2]["hours"] = json!(-1);
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("negative"));
    assert!(check(&g, "source-error").unwrap().message.contains("Row 3"));
    let (spec, mut rows) = fixture();
    rows.push(rows[0].clone());
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("repeats"));
    let (spec, mut rows) = fixture();
    rows[1]["hours"] = json!("many");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
}

#[test]
fn rows_without_an_x_or_a_value_are_disclosed() {
    let (spec, mut rows) = fixture();
    rows[0]["hours"] = Value::Null;
    rows[1]["month"] = Value::Null;
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 2);
}

#[test]
fn numeric_x_and_a_single_stream_work() {
    let spec = json!({"type":"streamgraph","title":"T","encoding":{"x":"x","y":"v"}});
    let rows: Vec<Value> = (0..6).map(|i| json!({"x": i, "v": 1 + i % 3})).collect();
    let g = draw(&spec, &rows);
    assert_eq!(streams(&g), 1);
    assert!(g.legend.is_none());
    contract(&spec, &rows);
}

#[test]
fn facets_each_get_their_own_streams() {
    let spec = json!({"type":"streamgraph","title":"T","encoding":{"x":"x","y":"v","color":"g","facet":"f"}});
    let mut rows = Vec::new();
    for f in ["North", "South"] {
        for x in 0..4 {
            for (k, g) in ["A", "B"].iter().enumerate() {
                rows.push(json!({"x": x, "g": g, "v": 1 + (x + k as i64) % 3, "f": f}));
            }
        }
    }
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(streams(&g), 4);
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    let zeros: Vec<Value> = (0..4).map(|i| json!({"x": i, "g": "A", "v": 0})).collect();
    let g = draw(&plain(), &zeros);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let huge = [
        json!({"x": 1, "g": "A", "v": 1e300}),
        json!({"x": 2, "g": "A", "v": 1e300}),
        json!({"x": 1, "g": "B", "v": 1e300}),
        json!({"x": 2, "g": "B", "v": 1e300}),
    ];
    let g = draw(&plain(), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let overflow = [
        json!({"x": 1, "g": "A", "v": 1.5e308}),
        json!({"x": 2, "g": "A", "v": 1.0}),
        json!({"x": 1, "g": "B", "v": 1.5e308}),
        json!({"x": 2, "g": "B", "v": 1.0}),
    ];
    let g = draw(&plain(), &overflow);
    assert!(g.failure.is_some() && check(&g, "value-overflow").is_some());
    let dense: Vec<Value> = (0..400)
        .flat_map(|i| (0..3).map(move |k| json!({"x": i, "g": format!("Group {k}"), "v": 1 + (i * (k + 1)) % 7})))
        .collect();
    let g = draw_at(&plain(), &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("streamgraph", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_axis_totals_and_name_places_of_the_exact_rows() {
    // Two streams within a grid step of each other, and one big month that sets the axis.
    let mut rows = Vec::new();
    for (month, (a, b, c)) in [
        ("2025-01", (100.01, 100.04, 10.0)),
        ("2025-02", (100.04, 100.01, 12.0)),
        ("2025-03", (300.0, 100.02, 900.0)),
        ("2025-04", (100.03, 100.03, 14.0)),
    ] {
        for (g, v) in [("A", a), ("B", b), ("C", c)] {
            rows.push(json!({"x": month, "v": v, "g": g}));
        }
    }
    runtime_matches_native(&plain(), &rows);
}
