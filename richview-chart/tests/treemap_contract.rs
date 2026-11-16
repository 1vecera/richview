//! Treemaps: areas proportional to values, parents summing their children, and the shared
//! contract every type meets (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/treemap.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

#[test]
fn the_golden_hierarchy_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn leaf_areas_are_proportional_to_their_values() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 8, "one observation per leaf");
    let area = |name: &str| {
        let o = obs.iter().find(|o| o.readout.heading.ends_with(name)).unwrap();
        let r = o.area.unwrap();
        r.w * r.h
    };
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(g.derived.iter().find(|d| d.key == "treemap.tiles").unwrap().value["grid"].clone())
            .unwrap();
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    let plot = g.panels[0].plot;
    let total: f64 = rows.iter().filter_map(|r| r["budget"].as_f64()).sum();
    for (name, value, share) in [("Operations", 180.0, "10.5%"), ("People", 90.0, "5.3%")] {
        let drawn = area(name) / (plot.w * plot.h);
        assert!((drawn - value / total).abs() <= step, "{name}: {drawn}");
        let o = obs.iter().find(|o| o.readout.heading == name).unwrap();
        assert_eq!(line(o, "Share of the whole"), Some(share));
    }
    let platform = obs
        .iter()
        .find(|o| o.readout.heading == "Engineering › Platform")
        .unwrap();
    assert_eq!(line(platform, "Budget"), Some("$420"));
    assert!(line(platform, "Share of the whole").is_some());
}

#[test]
fn tiles_stay_inside_the_plot_and_do_not_overlap() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let plot = g.panels[0].plot;
    let rects: Vec<_> = observations(&g).iter().map(|o| o.area.unwrap()).collect();
    for r in &rects {
        assert!(r.inside(&plot, 0.01));
    }
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            assert!(!a.overlaps(b, 0.5));
        }
    }
}

#[test]
fn a_parent_that_disagrees_with_its_children_is_reported_and_keeps_the_sum() {
    let (spec, mut rows) = fixture();
    rows[0] = json!({"part":"Engineering","group":null,"budget":1000});
    let g = draw(&spec, &rows);
    assert!(check(&g, "hierarchy-total").is_some());
    assert_eq!(observations(&g).len(), 8);
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
    let g = draw(&spec, &[json!({"part":"A","budget":1}), json!({"part":"A","budget":2})]);
    assert!(g.failure.is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, _) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &[json!({"part":"Only","budget":5})]);
    contract(&spec, &[json!({"part":"A","budget":0}), json!({"part":"B","budget":3})]);
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
    gallery("treemap", &spec, &rows);
}

fn plain_spec() -> Value {
    json!({"type":"treemap","title":"T","encoding":{"label":"part","parent":"group","y":"budget","color":"group"}})
}

fn area_of(g: &richview_chart::geometry::Geometry, heading: &str) -> f64 {
    let o = observations(g)
        .into_iter()
        .find(|o| o.readout.heading == heading)
        .unwrap();
    let r = o.area.unwrap();
    r.w * r.h
}

#[test]
fn equal_values_get_equal_tiles_whatever_group_they_sit_in() {
    let spec = plain_spec();
    let rows = vec![
        json!({"part":"Small","group":null,"budget":null}),
        json!({"part":"Big","group":null,"budget":null}),
        json!({"part":"S1","group":"Small","budget":10}),
        json!({"part":"S2","group":"Small","budget":10}),
        json!({"part":"B1","group":"Big","budget":10}),
        json!({"part":"B2","group":"Big","budget":400}),
    ];
    let g = draw(&spec, &rows);
    let (a, b) = (area_of(&g, "Small › S1"), area_of(&g, "Big › B1"));
    let grid: richview_chart::payload::Quant =
        serde_json::from_value(g.derived.iter().find(|d| d.key == "treemap.tiles").unwrap().value["grid"].clone())
            .unwrap();
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    let plot = g.panels[0].plot;
    for area in [a, b] {
        assert!((area / (plot.w * plot.h) - 10.0 / 430.0).abs() <= step, "{a} vs {b}");
    }
}

#[test]
fn many_small_groups_still_give_every_leaf_a_tile() {
    let spec = plain_spec();
    let mut rows = Vec::new();
    for gi in 0..40 {
        rows.push(json!({"part":format!("G{gi}"),"group":null,"budget":null}));
        for li in 0..4 {
            rows.push(json!({"part":format!("G{gi}L{li}"),"group":format!("G{gi}"),"budget":1}));
        }
    }
    let g = draw_at(&spec, &rows, 240.0);
    let obs = observations(&g);
    assert_eq!(obs.len(), 160);
    assert!(obs.iter().all(|o| o.area.is_some()), "every leaf has a tile");
    assert!(g.summary.contains("160 parts"));
}

#[test]
fn a_leaf_too_small_for_a_tile_is_reported_and_stays_reachable_by_keyboard() {
    let spec = plain_spec();
    let rows = vec![
        json!({"part":"Huge","group":null,"budget":1e9}),
        json!({"part":"Speck","group":null,"budget":1}),
    ];
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 2, "the speck keeps an observation");
    let speck = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Speck")
        .unwrap();
    assert!(speck.area.is_none());
    assert!(g.dropped.iter().any(|d| d.text == "Speck"));
    assert!(g.note.as_deref().is_some_and(|n| n.contains("too small to draw")));
    let order = richview_chart::hit::order(&g);
    assert_eq!(order.len(), 2);
    assert!(
        order
            .iter()
            .any(|t| g.panels[t.panel].observations[t.observation].readout.heading == "Speck")
    );
}

#[test]
fn a_leaf_without_a_value_is_counted_not_silently_dropped() {
    let spec = plain_spec();
    let rows = vec![
        json!({"part":"A","group":null,"budget":3}),
        json!({"part":"B","group":null,"budget":null}),
        json!({"part":"C","group":null,"budget":1}),
    ];
    let g = draw(&spec, &rows);
    let c = check(&g, "excluded-observations").expect("reported");
    assert!(c.message.starts_with("1 part "));
    assert_eq!(observations(&g).len(), 2);
    assert!(g.summary.starts_with("Treemap of 2 parts"), "{}", g.summary);
    assert!(g.note.as_deref().is_some_and(|n| n.contains("without a value")));
}

#[test]
fn the_share_label_says_visible_once_a_group_is_hidden() {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    let spec = plain_spec();
    let rows = vec![
        json!({"part":"A","group":null,"budget":3}),
        json!({"part":"B","group":null,"budget":1}),
    ];
    let block = ChartBlock::from_value(&spec).unwrap();
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
    let all = draw(&spec, &rows);
    let b = observations(&all)
        .into_iter()
        .find(|o| o.readout.heading == "B")
        .unwrap();
    assert!(line(b, "Share of the whole").is_some());
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
