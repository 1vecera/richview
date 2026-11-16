//! Sunbursts: ring slices whose angles are shares of the whole, parents summing their children,
//! and the shared contract every type meets (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::Item;
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/sunburst.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

#[test]
fn the_golden_hierarchy_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn angles_are_shares_of_the_whole_and_depth_sets_the_ring() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 12, "one observation per part, parents included");
    let wedge = |heading: &str| {
        obs.iter()
            .find(|o| o.readout.heading == heading)
            .unwrap()
            .wedge
            .unwrap()
    };
    // Budget total: 1,710.
    let ops = wedge("Operations");
    assert!(((ops.end - ops.start) / TAU - 180.0 / 1710.0).abs() < 1e-9);
    let eng = wedge("Engineering");
    assert!(((eng.end - eng.start) / TAU - 880.0 / 1710.0).abs() < 1e-9);
    assert_eq!(eng.start, 0.0, "the first part starts at 12 o'clock");
    // Children fill their parent's arc exactly and sit one ring further out.
    let platform = wedge("Engineering › Platform");
    let core = wedge("Engineering › Platform › Core");
    let tooling = wedge("Engineering › Platform › Tooling");
    assert!((core.start - platform.start).abs() < 1e-9 && (tooling.end - platform.end).abs() < 1e-9);
    assert!((core.end - tooling.start).abs() < 1e-9);
    assert_eq!(core.inner, platform.radius);
    assert_eq!(wedge("Engineering").inner, 0.0);
    assert!((wedge("Engineering").radius - platform.inner).abs() < 1e-9);
}

#[test]
fn readouts_name_the_path_and_mark_a_parent_value_as_derived() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let get = |h: &str| obs.iter().find(|o| o.readout.heading == h).unwrap();
    let core = get("Engineering › Platform › Core");
    assert_eq!(line(core, "Budget"), Some("$260"));
    assert!(!core.readout.lines[0].derived);
    let platform = get("Engineering › Platform");
    assert_eq!(line(platform, "Budget"), Some("$420"));
    assert!(platform.readout.lines[0].derived);
    let share = get("Operations");
    assert!(line(share, "Share of the whole").is_some());
    assert!(share.readout.lines[1].derived);
}

#[test]
fn the_pointer_hits_only_the_ring_of_a_slice() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    for (i, o) in g.panels[0].observations.iter().enumerate() {
        let w = o.wedge.unwrap();
        let mid = (w.start + w.end) / 2.0;
        let at = |r: f64| {
            [
                w.center[0] + r * (mid - std::f64::consts::FRAC_PI_2).cos(),
                w.center[1] + r * (mid - std::f64::consts::FRAC_PI_2).sin(),
            ]
        };
        let t = richview_chart::hit::at(&g, at((w.inner + w.radius) / 2.0)).unwrap();
        assert_eq!(t.observation, i);
    }
    let edge = g.panels[0]
        .observations
        .iter()
        .map(|o| o.wedge.unwrap().radius)
        .fold(0.0, f64::max);
    let c = g.panels[0].observations[0].wedge.unwrap().center;
    let outside = [c[0] + edge + 1.0, c[1]];
    assert!(richview_chart::hit::at(&g, outside).is_none());
}

#[test]
fn colour_follows_the_top_level_part_and_deeper_rings_are_lighter() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let alpha_of = |h: &str| {
        let o = observations(&g).into_iter().find(|o| o.readout.heading == h).unwrap();
        let series = o.series.unwrap();
        let a = g.panels[0]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Area { fill, obs, .. }
                    if *obs
                        == Some(
                            g.panels[0]
                                .observations
                                .iter()
                                .position(|x| std::ptr::eq(x, o))
                                .unwrap(),
                        ) =>
                {
                    Some(fill.color.alpha())
                }
                _ => None,
            })
            .unwrap();
        (series, a)
    };
    let (s1, a1) = alpha_of("Engineering");
    let (s2, a2) = alpha_of("Engineering › Platform › Core");
    assert_eq!(s1, s2);
    assert!(a2 < a1);
    assert_ne!(alpha_of("Sales").0, s1);
}

#[test]
fn a_parent_that_disagrees_with_its_children_is_reported_and_keeps_the_sum() {
    let (spec, mut rows) = fixture();
    rows[0] = json!({"part":"Engineering","group":null,"budget":1000});
    let g = draw(&spec, &rows);
    assert!(check(&g, "hierarchy-total").is_some());
    assert_eq!(observations(&g).len(), 12);
}

#[test]
fn broken_hierarchies_fail_with_the_row_named() {
    let (spec, mut rows) = fixture();
    rows[1]["group"] = json!("Nowhere");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("Nowhere"));
    let g = draw(
        &spec,
        &[
            json!({"part":"A","group":"B","budget":1}),
            json!({"part":"B","group":"A","budget":1}),
        ],
    );
    assert!(g.failure.is_some());
    let g = draw(&spec, &[json!({"part":"A","budget":-1})]);
    assert!(g.failure.is_some());
    assert!(
        g.panels[0]
            .items
            .iter()
            .any(|i| matches!(i, Item::Text(t) if t.text.contains("sunburst")))
    );
    let g = draw(&spec, &[json!({"part":"A","budget":1}), json!({"part":"A","budget":2})]);
    assert!(g.failure.is_some());
}

#[test]
fn a_name_that_does_not_fit_is_reported_never_clipped() {
    let (spec, rows) = fixture();
    let mut rows = rows;
    rows.push(json!({"part":"A very long name that cannot fit in any slice","budget":1}));
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A very long")));
}

#[test]
fn facets_draw_one_sunburst_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("year");
    let rows: Vec<Value> = [2024, 2025]
        .iter()
        .flat_map(|y| {
            rows.iter().cloned().map(move |mut r| {
                r["year"] = json!(y);
                r
            })
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 12);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, _) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &[json!({"part":"Only","budget":5})]);
    let g = draw(&spec, &[json!({"part":"Only","budget":5})]);
    let w = g.panels[0].observations[0].wedge.unwrap();
    assert!(
        (w.end - w.start - TAU).abs() < 1e-9,
        "a single part is the whole circle"
    );
    contract(&spec, &[json!({"part":"A","budget":0}), json!({"part":"B","budget":3})]);
    let g = draw(&spec, &[json!({"part":"A","budget":0}), json!({"part":"B","budget":3})]);
    assert_eq!(observations(&g).len(), 1, "a part of zero has no angle");
    let g = draw(&spec, &[json!({"part":"A","budget":0})]);
    assert!(g.failure.is_none() || g.panels[0].failure.is_some() || !g.summary.is_empty());
    let dense: Vec<Value> = (0..300)
        .map(|i| json!({"part":format!("Part {i}"),"budget":(i % 17 + 1)}))
        .collect();
    let g = draw_at(&spec, &dense, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 300);
    let g = draw(
        &spec,
        &[json!({"part":"A","budget":1e300}), json!({"part":"B","budget":1e300})],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("sunburst", &spec, &rows);
}

fn plain_spec() -> Value {
    json!({"type":"sunburst","title":"T","encoding":{"label":"part","parent":"group","y":"budget","color":"group"}})
}

#[test]
fn a_leaf_without_a_value_is_counted_and_left_out_of_the_summary() {
    let rows = vec![
        json!({"part":"A","group":null,"budget":3}),
        json!({"part":"B","group":null,"budget":null}),
        json!({"part":"C","group":null,"budget":1}),
    ];
    let g = draw(&plain_spec(), &rows);
    let c = check(&g, "excluded-observations").expect("reported");
    assert!(c.message.starts_with("1 part "));
    assert!(g.summary.starts_with("Sunburst of 2 parts"), "{}", g.summary);
    assert!(g.note.as_deref().is_some_and(|n| n.contains("without a value")));
}

#[test]
fn the_share_label_says_visible_once_a_group_is_hidden() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let rows = vec![
        json!({"part":"A","group":null,"budget":3}),
        json!({"part":"B","group":null,"budget":1}),
    ];
    let block = ChartBlock::from_value(&plain_spec()).unwrap();
    let hidden = layout(
        &block,
        &rows,
        &ChartTheme::default(),
        &Options {
            hidden: vec!["A".into()],
            ..Options::default()
        },
    );
    let b = observations(&hidden)
        .into_iter()
        .find(|o| o.readout.heading == "B")
        .unwrap();
    assert!(line(b, "Share of the visible").is_some());
    assert!(line(b, "Share of the whole").is_none());
}

#[test]
fn the_runtime_keeps_the_parts_the_exact_rows_keep() {
    // A part of 0.05 beside one of 1000 is zero on the grid, and 100.01 against 100.04 swap.
    let (spec, _) = fixture();
    let field = |name: &str| spec["encoding"][name].as_str().unwrap_or_default().to_owned();
    let (part, group, value) = (field("label"), field("parent"), field("y"));
    let row = |name: &str, parent: Option<&str>, v: Option<f64>| {
        let mut r = serde_json::Map::new();
        r.insert(part.clone(), json!(name));
        r.insert(group.clone(), json!(parent));
        r.insert(value.clone(), json!(v));
        Value::Object(r)
    };
    let rows = vec![
        row("Big", None, None),
        row("Tiny", Some("Big"), Some(0.05)),
        row("Beta", Some("Big"), Some(100.01)),
        row("Alpha", Some("Big"), Some(100.04)),
        row("Huge", Some("Big"), Some(1000.0)),
        row("Rest", None, Some(300.5)),
    ];
    runtime_matches_native(&spec, &rows);
}
