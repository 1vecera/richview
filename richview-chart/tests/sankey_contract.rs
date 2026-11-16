//! Sankey diagrams: columns by longest path, node heights and ribbon widths proportional to flow,
//! cycles left out with a finding, and the failures that name their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Observation, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/sankey.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"sankey","title":"T","encoding":{"source":"a","target":"b","y":"v"}})
}

fn row(a: &str, b: &str, v: Value) -> Value {
    json!({"a": a, "b": b, "v": v})
}

#[test]
fn a_value_field_without_an_authored_axis_title_has_a_readable_name() {
    let g = draw(&plain(), &[row("A", "B", json!(7))]);
    assert_eq!(line(flow(&g, "A", "B"), "V"), Some("7"));
}

fn node<'a>(g: &'a Geometry, name: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.row.is_none() && o.readout.heading == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}

fn flow<'a>(g: &'a Geometry, from: &str, to: &str) -> &'a Observation {
    let heading = format!("{from} → {to}");
    observations(g)
        .into_iter()
        .find(|o| o.row.is_some() && o.readout.heading == heading)
        .unwrap_or_else(|| panic!("no flow {heading}"))
}

/// The ribbons of the first panel, in observation order.
fn ribbons(g: &Geometry) -> Vec<&Item> {
    g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Area { .. }) && i.role() == Role::Mark)
        .collect()
}

/// How thick a ribbon is where it leaves its source: the step from its upper to its lower edge.
fn thickness(ribbon: &Item) -> f64 {
    let Item::Area { path, .. } = ribbon else {
        panic!("not a ribbon")
    };
    match (path.cmds[1], path.cmds[2]) {
        (Cmd::C { p: top, .. }, Cmd::L { p: bottom }) => bottom[1] - top[1],
        other => panic!("unexpected ribbon path {other:?}"),
    }
}

fn thickness_of(g: &Geometry, from: &str, to: &str) -> f64 {
    let o = flow(g, from, to);
    let index = g.panels[0]
        .observations
        .iter()
        .position(|x| std::ptr::eq(x, o))
        .unwrap();
    let item = ribbons(g)
        .into_iter()
        .find(|i| matches!(i, Item::Area { obs: Some(k), .. } if *k == index))
        .unwrap();
    thickness(item)
}

fn draw_hidden(spec: &Value, rows: &[Value], hidden: &[&str]) -> Geometry {
    layout(
        &ChartBlock::from_value(spec).unwrap(),
        rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            hidden: hidden.iter().map(|h| h.to_string()).collect(),
            ..Options::default()
        },
    )
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn each_node_and_ribbon_owns_its_observation() {
    let alluvial: Value = serde_json::from_str(include_str!("fixtures/types/alluvial.json")).unwrap();
    let fixtures = [
        fixture(),
        (alluvial["spec"].clone(), alluvial["rows"].as_array().unwrap().clone()),
    ];
    for (spec, rows) in &fixtures {
        let g = draw(spec, rows);
        for panel in &g.panels {
            for (index, observation) in panel.observations.iter().enumerate() {
                let marks: Vec<_> = panel
                    .items
                    .iter()
                    .filter(|item| match item {
                        Item::Rect {
                            role: Role::Mark,
                            obs: Some(owner),
                            ..
                        }
                        | Item::Area {
                            role: Role::Mark,
                            obs: Some(owner),
                            ..
                        } => *owner == index,
                        _ => false,
                    })
                    .collect();
                assert_eq!(marks.len(), 1, "{} owns one mark", observation.readout.heading);
                let bounds = match marks[0] {
                    Item::Rect { rect, .. } => Some(*rect),
                    Item::Area { path, .. } => path.bounds(),
                    _ => unreachable!(),
                };
                assert_eq!(
                    bounds, observation.area,
                    "{} owns its own shape",
                    observation.readout.heading
                );
            }
        }
    }
}

#[test]
fn columns_follow_the_longest_path_from_a_source() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let x = |n: &str| node(&g, n).at[0];
    assert_eq!(x("Coal"), x("Gas"));
    assert_eq!(x("Coal"), x("Solar"));
    assert_eq!(x("Power"), x("Heat"));
    assert_eq!(x("Homes"), x("Losses"));
    assert!(x("Coal") < x("Power") && x("Power") < x("Homes"));
    // Nine nodes plus the ten links of the rows.
    assert_eq!(observations(&g).len(), 9 + 10);
}

#[test]
fn a_node_is_as_tall_as_its_flow() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let h = |n: &str| node(&g, n).area.unwrap().h;
    assert!((h("Power") / h("Heat") - 95.0 / 25.0).abs() < 1e-6);
    assert!((h("Coal") / h("Gas") - 35.0 / 55.0).abs() < 1e-6);
    // The tallest column holds the most flow: the one of Homes, Industry and Losses is 120.
    assert!((h("Homes") / h("Losses") - 60.0 / 10.0).abs() < 1e-6);
}

#[test]
fn a_ribbon_is_as_wide_as_its_flow_and_the_widths_fill_the_node() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let t = |a: &str, b: &str| thickness_of(&g, a, b);
    assert!((t("Coal", "Power") / t("Gas", "Power") - 35.0 / 30.0).abs() < 1e-6);
    let leaving = t("Power", "Homes") + t("Power", "Industry") + t("Power", "Losses");
    assert!((leaving - node(&g, "Power").area.unwrap().h).abs() < 1e-6);
}

#[test]
fn ribbons_leave_the_right_edge_of_a_node_and_reach_the_left_edge_of_another() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let rects: Vec<_> = observations(&g)
        .into_iter()
        .filter(|o| o.row.is_none())
        .map(|o| o.area.unwrap())
        .collect();
    for item in ribbons(&g) {
        let Item::Area { path, .. } = item else { unreachable!() };
        let (Cmd::M { p: start }, Cmd::L { p: end }) = (path.cmds[0], path.cmds[2]) else {
            panic!("unexpected ribbon path")
        };
        assert!(rects.iter().any(|r| (r.right() - start[0]).abs() < 1e-6));
        assert!(rects.iter().any(|r| (r.x - end[0]).abs() < 1e-6));
    }
}

#[test]
fn readouts_give_the_value_the_shares_and_the_sides_of_a_node() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let f = flow(&g, "Power", "Homes");
    assert_eq!(line(f, "TWh"), Some("45"));
    assert_eq!(line(f, "Share of source"), Some("47.4%"));
    assert!(!f.readout.lines[0].derived, "the author gave the value");
    assert!(f.readout.lines.iter().any(|l| l.name == "Share of source" && l.derived));
    let power = node(&g, "Power");
    assert_eq!(line(power, "TWh"), Some("95"));
    assert_eq!(line(power, "In"), Some("95"));
    assert_eq!(line(power, "Out"), Some("95"));
    assert_eq!(line(power, "Share of column"), Some("79.2%"));
    assert!(power.readout.lines.iter().all(|l| l.derived), "a node's size is a sum");
    let coal = node(&g, "Coal");
    assert_eq!(line(coal, "In"), None, "a source has no inflow to read");
}

#[test]
fn colour_follows_the_source_a_node_mostly_comes_from() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 4, "one series for each source");
    let s = |n: &str| node(&g, n).series;
    assert_eq!(s("Coal"), s("Power"));
    assert_eq!(s("Gas"), s("Heat"));
    assert_ne!(s("Coal"), s("Gas"));
    assert_eq!(flow(&g, "Wind", "Power").series, s("Wind"));
}

#[test]
fn a_link_that_closes_a_cycle_is_left_out_and_reported() {
    let spec = plain();
    let rows = [
        row("A", "B", json!(2)),
        row("B", "C", json!(2)),
        row("C", "A", json!(1)),
    ];
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert_eq!(observations(&g).len(), 3 + 2);
    let c = check(&g, "excluded-observations").expect("the cycle is reported");
    assert!(c.message.contains("cycle"), "{}", c.message);
    assert_eq!(c.details["rows"], json!([2]));
    contract(&spec, &rows);
    // A link from a node to itself is left out, and said so.
    let g = draw(&spec, &[row("A", "B", json!(2)), row("B", "B", json!(1))]);
    assert_eq!(observations(&g).len(), 2 + 1);
    assert!(check(&g, "excluded-observations").is_some());
}

#[test]
fn rows_without_a_usable_link_are_left_out_and_counted() {
    let spec = plain();
    let rows = [
        row("A", "B", json!(2)),
        row("A", "C", json!(-1)),
        row("A", "D", Value::Null),
        json!({"a": "A", "v": 3}),
        row("B", "C", json!(0)),
    ];
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 2 + 1);
    let c = check(&g, "excluded-observations").expect("reported");
    assert_eq!(c.details["count"], 3);
    assert!(g.note.as_deref().unwrap_or("").contains("zero"));
}

#[test]
fn bad_values_fail_the_chart_with_the_row_named() {
    let spec = plain();
    let g = draw(&spec, &[row("A", "B", json!("lots"))]);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").unwrap().message.contains("lots"));
    let g = draw(&spec, &[row("A", "B", json!(1)), row("A", "B", json!(2))]);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("A → B"));
    let g = draw(&spec, &[row("A", "B", json!(1e308)), row("B", "C", json!(1e308))]);
    assert!(g.failure.is_some());
    assert!(check(&g, "value-overflow").is_some());
    let g = draw(
        &json!({"type":"sankey","title":"T","encoding":{"source":"a","y":"v"}}),
        &[row("A", "B", json!(1))],
    );
    assert!(g.failure.is_some());
}

#[test]
fn a_name_that_does_not_fit_is_reported_never_clipped() {
    let (spec, mut rows) = fixture();
    rows.push(
        json!({"from": "A very long source name that cannot fit beside its node at all", "to": "Power", "twh": 5}),
    );
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(check(&g, "label-collision").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A very long")));
}

#[test]
fn hiding_a_source_removes_its_ribbons_and_keeps_the_rest() {
    let (spec, rows) = fixture();
    let g = draw_hidden(&spec, &rows, &["Wind"]);
    assert!(g.failure.is_none());
    assert!(observations(&g).iter().all(|o| o.readout.heading != "Wind"));
    assert!(observations(&g).iter().all(|o| o.readout.heading != "Wind → Power"));
    assert_eq!(line(node(&g, "Power"), "In"), Some("75"));
}

#[test]
fn facets_share_one_scale_unless_freed() {
    let spec = json!({"type":"sankey","title":"T","encoding":{"source":"a","target":"b","y":"v","facet":"f"}});
    let rows: Vec<Value> = vec![
        json!({"a":"A","b":"B","v":10,"f":"small"}),
        json!({"a":"A","b":"B","v":20,"f":"large"}),
    ];
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    let h = |p: usize| {
        g.panels[p]
            .observations
            .iter()
            .find(|o| o.readout.heading == "A")
            .unwrap()
            .area
            .unwrap()
            .h
    };
    assert!((h(1) / h(0) - 2.0).abs() < 1e-6);
    let mut free = spec.clone();
    free["style"] = json!({"facet_scale": "free"});
    let g = draw(&free, &rows);
    let freed = |p: usize| {
        g.panels[p]
            .observations
            .iter()
            .find(|o| o.readout.heading == "A")
            .unwrap()
            .area
            .unwrap()
            .h
    };
    assert!((freed(1) / freed(0) - 1.0).abs() < 1e-6);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let spec = plain();
    stays_visible(&spec, &[]);
    stays_visible(&spec, &[row("A", "A", json!(1))]);
    contract(&spec, &[row("A", "B", json!(1))]);
    let g = draw(&spec, &[row("A", "B", json!(1e300)), row("A", "C", json!(1e-300))]);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(
        observations(&g).len(),
        3 + 2,
        "a flow too small to see is still drawn as a hairline"
    );
    let chain: Vec<Value> = (0..30)
        .map(|i| row(&format!("Step {i}"), &format!("Step {}", i + 1), json!(i + 1)))
        .collect();
    let g = draw_at(&spec, &chain, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 31 + 30);
    let wide: Vec<Value> = (0..60)
        .map(|i| row("Hub", &format!("Leaf {i}"), json!(i % 7 + 1)))
        .collect();
    let g = draw_at(&spec, &wide, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 61 + 60);
    assert!(
        !g.dropped.is_empty(),
        "60 names in one column do not fit and are reported"
    );
}

#[test]
fn a_sankey_over_a_limit_is_not_drawn_and_names_the_limit_and_at_it_draws() {
    let rows: Vec<Value> = (0..300)
        .map(|i| row(&format!("s{i}"), &format!("s{}", i + 1), json!(1000 - i)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "nodes");
    assert_eq!(c.details["count"], 301);
    assert_eq!(c.details["limit"], 300);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..46)
        .flat_map(|i| (0..46).map(move |j| row(&format!("a{i}"), &format!("b{j}"), json!(1 + (i + j) % 5))))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "links");
    assert_eq!(c.details["count"], 2116);
    assert_eq!(c.details["limit"], 2000);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..299)
        .map(|i| row(&format!("s{i}"), &format!("s{}", i + 1), json!(1000 - i)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(check(&g, "limit-exceeded").is_none());
    assert!(nonfinite(&g).is_none());
}

#[test]
fn nodes_stay_inside_the_plot_when_the_smallest_flows_need_their_floor() {
    let mut rows = vec![row("S", "Big", json!(1e6))];
    rows.extend((0..250).map(|i| row("S", &format!("Tiny {i}"), json!(1))));
    let g = draw_at(&plain(), &rows, 390.0);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    let plot = g.panels[0].plot;
    let nodes: Vec<_> = observations(&g).into_iter().filter(|o| o.row.is_none()).collect();
    assert_eq!(nodes.len(), 252);
    for o in nodes {
        let r = o.area.expect("a node has a bar");
        assert!(
            r.y >= plot.y - 0.01 && r.bottom() <= plot.bottom() + 0.01,
            "{} {r:?} outside {plot:?}",
            o.readout.heading
        );
    }
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("sankey", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_flows_the_exact_rows_keep() {
    // A flow of 0.1 beside one of 1000 is smaller than a step of the grid and snaps to zero.
    let tiny = vec![
        row("Grid", "Homes", json!(1000)),
        row("Grid", "Shops", json!(7)),
        row("Grid", "Kiosk", json!(0.1)),
    ];
    let g = draw(&plain(), &tiny);
    assert_eq!(observations(&g).iter().filter(|o| o.row.is_some()).count(), 3);
    runtime_matches_native(&plain(), &tiny);
    // The cycle is broken at the same link when a small flow is zero on the grid.
    let cycle = vec![
        row("X", "Y", json!(0)),
        row("A", "B", json!(0.05)),
        row("B", "A", json!(1000)),
    ];
    runtime_matches_native(&plain(), &cycle);
    // Two inflows that differ by less than a step colour the node alike in both layouts.
    let close = vec![
        row("Alpha", "Sink", json!(100.04)),
        row("Beta", "Sink", json!(100.01)),
        row("Gamma", "Big", json!(1000)),
    ];
    runtime_matches_native(&plain(), &close);
}
