//! Icicle charts: bands by depth, widths that are shares of the whole, parents summing their
//! children, and the shared contract every type meets (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::Item;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/icicle.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

#[test]
fn the_golden_hierarchy_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn widths_are_shares_of_the_whole_and_depth_sets_the_band() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let plot = g.panels[0].plot;
    let obs = observations(&g);
    assert_eq!(obs.len(), 12, "one observation per part, parents included");
    let rect = |h: &str| obs.iter().find(|o| o.readout.heading == h).unwrap().area.unwrap();
    // Budget total: 1,710.
    let ops = rect("Operations");
    assert!((ops.w / plot.w - 180.0 / 1710.0).abs() < 1e-9);
    let eng = rect("Engineering");
    assert!((eng.w / plot.w - 880.0 / 1710.0).abs() < 1e-9);
    assert!((eng.x - plot.x).abs() < 1e-9, "the first part starts at the left edge");
    assert!((eng.y - plot.y).abs() < 1e-9, "the top level is the top band");
    // Children fill their parent's width exactly and sit one band lower.
    let platform = rect("Engineering › Platform");
    let core = rect("Engineering › Platform › Core");
    let tooling = rect("Engineering › Platform › Tooling");
    assert!((core.x - platform.x).abs() < 1e-9 && (tooling.right() - platform.right()).abs() < 1e-9);
    assert!((core.right() - tooling.x).abs() < 1e-9);
    assert!((core.y - (platform.y + platform.h)).abs() < 1e-9);
    assert!((platform.y - (eng.y + eng.h)).abs() < 1e-9);
    // Siblings keep source order from the left.
    assert!(rect("Engineering › Product").x < rect("Engineering › Data").x);
    assert!(eng.right() <= rect("Sales").x + 1e-9);
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
    let ops = get("Operations");
    assert!(line(ops, "Share of the whole").is_some());
    assert!(ops.readout.lines[1].derived);
}

#[test]
fn parts_stay_inside_the_plot_and_do_not_overlap() {
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
fn colour_follows_the_top_level_part_and_deeper_bands_are_lighter() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let panel = &g.panels[0];
    let of = |h: &str| {
        let i = panel.observations.iter().position(|o| o.readout.heading == h).unwrap();
        let alpha = panel
            .items
            .iter()
            .find_map(|it| match it {
                Item::Rect { fill, obs, .. } if *obs == Some(i) => Some(fill.color.alpha()),
                _ => None,
            })
            .unwrap();
        (panel.observations[i].series.unwrap(), alpha)
    };
    let (s1, a1) = of("Engineering");
    let (s2, a2) = of("Engineering › Platform › Core");
    assert_eq!(s1, s2);
    assert!(a2 < a1);
    assert_ne!(of("Sales").0, s1);
}

#[test]
fn a_name_that_does_not_fit_is_reported_never_clipped() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"part":"A very long name that cannot fit in any band","budget":1}));
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A very long")));
    // Every name that is drawn lies inside its own band.
    for it in &g.panels[0].items {
        if let Item::Text(t) = it {
            assert!(
                g.panels[0]
                    .observations
                    .iter()
                    .any(|o| o.area.is_some_and(|a| t.bounds.inside(&a, 0.01)))
            );
        }
    }
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
    let g = draw(&spec, &[json!({"part":"A","budget":1}), json!({"part":"A","budget":2})]);
    assert!(g.failure.is_some());
}

#[test]
fn facets_draw_one_icicle_each() {
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
    let r = g.panels[0].observations[0].area.unwrap();
    assert!(
        (r.w - g.panels[0].plot.w).abs() < 1e-9,
        "a single part spans the whole width"
    );
    contract(&spec, &[json!({"part":"A","budget":0}), json!({"part":"B","budget":3})]);
    let g = draw(&spec, &[json!({"part":"A","budget":0}), json!({"part":"B","budget":3})]);
    assert_eq!(observations(&g).len(), 1, "a part of zero has no width");
    let g = draw(&spec, &[json!({"part":"A","budget":0})]);
    assert!(g.failure.is_some(), "nothing positive says why");
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
    gallery("icicle", &spec, &rows);
}

fn plain_spec() -> Value {
    json!({"type":"icicle","title":"T","encoding":{"label":"part","parent":"group","y":"budget","color":"group"}})
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
    assert!(g.summary.starts_with("Icicle chart of 2 parts"), "{}", g.summary);
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
