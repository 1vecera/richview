//! 3-D bar charts: boxes on a grid of two categories drawn as the three faces that face the
//! viewer, far to near, standing on zero, sums marked as derived, and the failures that name
//! their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/bar3d.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"bar3d","title":"T","encoding":{"x":"a","y":"b"}})
}

fn bar(x: &str, y: f64) -> Value {
    json!({"a": x, "b": y})
}

/// The faces of observation `k` in the order they are drawn.
fn faces(g: &Geometry, k: usize) -> Vec<&Item> {
    g.items()
        .filter(|i| matches!(i, Item::Area { obs: Some(o), role: Role::Mark, .. } if *o == k))
        .collect()
}

fn headings(g: &Geometry) -> Vec<&str> {
    observations(g)
        .into_iter()
        .map(|o| o.readout.heading.as_str())
        .collect()
}

fn front(spec: &mut Value) {
    spec["style"] = json!({"azimuth": 0, "elevation": 30});
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("bar3d", &spec, &rows);
}

#[test]
fn one_bar_for_each_pair_with_both_categories_and_the_value_in_the_readout() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g);
    assert_eq!(o.len(), 12);
    assert!(g.legend.is_some());
    assert_eq!(g.series.len(), 3);
    let west_online = o.iter().find(|o| o.readout.heading == "West · Online").unwrap();
    assert_eq!(line(west_online, "Revenue (EUR m)"), Some("58"));
    assert_eq!(line(west_online, "channel"), None);
    assert_eq!(line(west_online, "Region"), None);
    assert!(west_online.readout.lines.iter().all(|l| !l.derived));
    assert!(west_online.row.is_some());
}

#[test]
fn bars_are_drawn_far_to_near_and_each_shows_three_faces() {
    let (mut spec, rows) = fixture();
    front(&mut spec);
    let g = draw(&spec, &rows);
    let h = headings(&g);
    // From the front the last group stands farthest back and goes first.
    assert!(h[..4].iter().all(|t| t.ends_with("Partner")), "{h:?}");
    assert!(h[8..].iter().all(|t| t.ends_with("Online")), "{h:?}");
    for k in 0..12 {
        assert_eq!(faces(&g, k).len(), 3, "bar {k} shows its top and two sides");
    }
}

#[test]
fn the_top_is_lightest_and_the_two_sides_differ() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let fills: Vec<_> = faces(&g, 0)
        .into_iter()
        .map(|i| match i {
            Item::Area { fill, .. } => fill.color.oklch().l,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(fills.len(), 3);
    let top = fills[2];
    assert!(top > fills[0] && top > fills[1], "{fills:?}");
    assert!((fills[0] - fills[1]).abs() > 0.01, "{fills:?}");
}

#[test]
fn a_bar_stands_on_zero_and_a_negative_one_hangs_below_it() {
    let mut spec = plain();
    front(&mut spec);
    let rows = [bar("up", 10.0), bar("zero", 0.0), bar("down", -6.0)];
    let g = draw(&spec, &rows);
    let o = observations(&g);
    let top = |name: &str| o.iter().find(|o| o.readout.heading == name).unwrap().at[1];
    assert!(top("up") < top("zero"), "a positive bar rises above the zero plane");
    assert!(
        (top("zero") - top("down")).abs() < 6.0,
        "a negative bar's top is the zero plane"
    );
    assert_eq!(faces(&g, 0).len() + faces(&g, 1).len() + faces(&g, 2).len(), 3 + 1 + 3);
    assert_eq!(
        line(o.iter().find(|o| o.readout.heading == "down").unwrap(), "B"),
        Some("\u{2212}6")
    );
    contract(&spec, &rows);
}

#[test]
fn rows_of_one_pair_are_added_up_and_marked() {
    let rows = [bar("a", 3.0), bar("b", 4.0), bar("a", 5.0)];
    let g = draw(&plain(), &rows);
    let o = observations(&g);
    assert_eq!(o.len(), 2);
    let a = o.iter().find(|o| o.readout.heading == "a").unwrap();
    assert_eq!(line(a, "B"), Some("8"));
    assert!(a.readout.lines.iter().find(|l| l.name == "B").unwrap().derived);
    assert_eq!(line(a, "Source rows"), Some("2"));
    assert!(g.note.as_deref().unwrap().contains("sums"));
    let b = o.iter().find(|o| o.readout.heading == "b").unwrap();
    assert!(!b.readout.lines.iter().find(|l| l.name == "B").unwrap().derived);
}

#[test]
fn without_a_second_category_there_is_one_row_of_bars() {
    let rows = [bar("a", 3.0), bar("b", 4.0), bar("c", 2.0)];
    let g = draw(&plain(), &rows);
    assert!(g.legend.is_none());
    let mut seen = headings(&g);
    seen.sort_unstable();
    assert_eq!(seen, ["a", "b", "c"]);
    contract(&plain(), &rows);
}

#[test]
fn the_view_follows_the_azimuth_and_the_elevation() {
    let (spec, rows) = fixture();
    let centres = |spec: &Value| -> Vec<[f64; 2]> { observations(&draw(spec, &rows)).iter().map(|o| o.at).collect() };
    let mut same = spec.clone();
    same["style"] = json!({"azimuth": 35, "elevation": 28});
    assert_eq!(centres(&spec), centres(&same));
    let mut turned = spec.clone();
    turned["style"] = json!({"azimuth": 215, "elevation": 55});
    assert_ne!(centres(&spec), centres(&turned));
}

#[test]
fn every_view_draws_clean() {
    let (spec, rows) = fixture();
    for azimuth in [0, 45, 90, 135, 180, 225, 270, 315] {
        for elevation in [5, 28, 60, 85] {
            let mut s = spec.clone();
            s["style"] = json!({"azimuth": azimuth, "elevation": elevation});
            contract(&s, &rows);
        }
    }
}

#[test]
fn above_the_limits_the_chart_is_not_drawn_and_at_them_it_is() {
    let xs = |n: usize| -> Vec<Value> { (0..n).map(|k| bar(&format!("category {k}"), k as f64 + 1.0)).collect() };
    let at_limit = xs(24);
    let g = draw(&plain(), &at_limit);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 24);
    contract(&plain(), &at_limit);

    let g = draw(&plain(), &xs(25));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "x categories");
    assert_eq!(c.details["count"], 25);
    assert_eq!(c.details["limit"], 24);
    assert!(observations(&g).is_empty());

    let mut spec = plain();
    spec["encoding"]["color"] = json!("g");
    let groups = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|k| json!({"a": "x", "b": 1, "g": format!("group {k}")}))
            .collect()
    };
    let g = draw(&spec, &groups(12));
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 12);
    let g = draw(&spec, &groups(13));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "groups");
    assert_eq!(c.details["count"], 13);
    assert_eq!(c.details["limit"], 12);
}

#[test]
fn rows_without_a_category_or_a_value_are_disclosed_and_bad_input_says_why() {
    let (spec, mut rows) = fixture();
    rows[0]["revenue"] = Value::Null;
    rows[1]["region"] = json!("");
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 2);
    assert_eq!(observations(&g).len(), 10);

    let g = draw(&plain(), &[bar("a", 1.0), json!({"a": "b", "b": "lots"})]);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let spec = json!({"type":"bar3d","title":"T","encoding":{"x":"a"}});
    let g = draw(&spec, &[bar("a", 1.0)]);
    assert!(g.failure.as_deref().unwrap().contains("encoding.y"));
    stays_visible(&plain(), &[]);
}

#[test]
fn single_zero_and_all_negative_bars_draw() {
    contract(&plain(), &[bar("only", 5.0)]);
    contract(&plain(), &[bar("a", 0.0), bar("b", 0.0)]);
    contract(&plain(), &[bar("a", -3.0), bar("b", -9.0)]);
    contract(&plain(), &[bar("a", 1e-6), bar("b", 2e-6)]);
}

#[test]
fn the_runtime_keeps_the_axis_and_the_sums_of_the_exact_rows() {
    // Bars a grid step apart, a pair of rows added up, and a tall bar that sets the axis.
    let rows = vec![
        bar("A", 100.01),
        bar("B", 100.04),
        bar("C", 1000.0),
        bar("C", 0.03),
        bar("D", 640.0),
    ];
    runtime_matches_native(&plain(), &rows);
}
