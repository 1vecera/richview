//! Network diagrams: a fixed force layout that gives the same picture for the same rows, node
//! size by number of links, lines for links, names beside nodes that give way and are reported,
//! and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, DropReason, Geometry, Item, Observation, Rect, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/network.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"network","title":"T","encoding":{"source":"a","target":"b"}})
}

fn row(a: &str, b: &str) -> Value {
    json!({"a": a, "b": b})
}

fn node<'a>(g: &'a Geometry, name: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.row.is_none() && o.readout.heading == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}

fn link<'a>(g: &'a Geometry, heading: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.row.is_some() && o.readout.heading == heading)
        .unwrap_or_else(|| panic!("no link {heading}"))
}

fn points(g: &Geometry) -> Vec<&Item> {
    g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Point { .. }))
        .collect()
}

fn lines(g: &Geometry) -> Vec<&Item> {
    g.panels[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Line { .. }))
        .collect()
}

// Independent oracle: rectangle-edge intersections, including closed endpoint/corner contact.
fn link_touches_box(a: [f64; 2], b: [f64; 2], r: Rect) -> bool {
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let sides = [
        ([r.x, r.y], [r.right(), r.y]),
        ([r.right(), r.y], [r.right(), r.bottom()]),
        ([r.right(), r.bottom()], [r.x, r.bottom()]),
        ([r.x, r.bottom()], [r.x, r.y]),
    ];
    r.contains(a)
        || r.contains(b)
        || sides.iter().any(|&(c, d)| {
            a[0].min(b[0]) <= c[0].max(d[0])
                && a[0].max(b[0]) >= c[0].min(d[0])
                && a[1].min(b[1]) <= c[1].max(d[1])
                && a[1].max(b[1]) >= c[1].min(d[1])
                && cross(a, b, c) * cross(a, b, d) <= 0.0
                && cross(c, d, a) * cross(c, d, b) <= 0.0
        })
}

#[test]
fn retained_node_names_clear_every_visible_link_stroke() {
    let (original, rows) = fixture();
    for weighted in [true, false] {
        let mut spec = original.clone();
        if !weighted {
            spec["encoding"].as_object_mut().unwrap().remove("y");
        }
        let block = ChartBlock::from_value(&spec).unwrap();
        for width in [320.0, 640.0, 1440.0] {
            for hidden in [vec![], vec!["Group 2".to_owned()]] {
                let g = layout(
                    &block,
                    &rows,
                    &ChartTheme::default(),
                    &Options {
                        width,
                        hidden,
                        ..Options::default()
                    },
                );
                let p = &g.panels[0];
                let names: Vec<_> = p
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Text(t) if t.role == Role::Label => Some(t),
                        _ => None,
                    })
                    .collect();
                assert!(!names.is_empty(), "safe adjacent positions retain readable names");
                let dropped = g.dropped.iter().filter(|d| d.kind.as_deref() == Some("node")).count();
                assert_eq!(
                    names.len() + dropped,
                    p.observations.iter().filter(|o| o.row.is_none()).count(),
                    "each visible node is named or its name has a reported placement failure"
                );
                assert!(g.dropped.iter().all(|d| d.reason == DropReason::Collision));
                for (i, t) in names.iter().enumerate() {
                    assert!(t.bounds.inside(&p.plot, 0.0));
                    assert!(names.iter().skip(i + 1).all(|u| !t.bounds.overlaps(&u.bounds, 0.0)));
                    for item in &p.items {
                        match item {
                            Item::Line {
                                path,
                                stroke,
                                role: Role::Mark,
                                ..
                            } => {
                                let [Cmd::M { p: a }, Cmd::L { p: b }] = path.cmds.as_slice() else {
                                    panic!("network links stay straight");
                                };
                                assert!(
                                    !link_touches_box(*a, *b, t.bounds.inflate(stroke.width / 2.0)),
                                    "{} crosses a visible link at width{width}, weighted={weighted}",
                                    t.text
                                );
                            }
                            Item::Point {
                                center,
                                radius,
                                role: Role::Mark,
                                ..
                            } => {
                                assert!(
                                    !t.bounds.overlaps(
                                        &Rect::new(center[0] - radius, center[1] - radius, radius * 2.0, radius * 2.0),
                                        0.0
                                    ),
                                    "names avoid nodes"
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn the_same_rows_always_give_the_same_picture() {
    let (spec, rows) = fixture();
    let (a, b) = (draw(&spec, &rows), draw(&spec, &rows));
    let at = |g: &Geometry| -> Vec<[f64; 2]> { observations(g).iter().map(|o| o.at).collect() };
    assert_eq!(at(&a), at(&b));
    let wide = (draw_at(&spec, &rows, 1440.0), draw_at(&spec, &rows, 1440.0));
    assert_eq!(at(&wide.0), at(&wide.1));
    // A different order of rows names the nodes in another order and starts elsewhere.
    let mut shuffled = rows.clone();
    shuffled.reverse();
    assert_eq!(observations(&draw(&spec, &shuffled)).len(), observations(&a).len());
}

#[test]
fn a_node_for_every_name_and_a_line_for_every_row() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 12 + 15);
    assert_eq!(points(&g).len(), 12);
    assert_eq!(lines(&g).len(), 15);
    // A line runs between the centres of its two nodes, and its readout sits halfway.
    let (a, b) = (node(&g, "Ada").at, node(&g, "Ben").at);
    let l = link(&g, "Ada – Ben");
    assert!((l.at[0] - (a[0] + b[0]) / 2.0).abs() < 1e-9 && (l.at[1] - (a[1] + b[1]) / 2.0).abs() < 1e-9);
    let found = lines(&g).into_iter().any(|i| match i {
        Item::Line { path, .. } => path == &richview_chart::geometry::Path::polyline(&[a, b]),
        _ => false,
    });
    assert!(found, "a straight line joins Ada and Ben");
}

#[test]
fn a_node_is_as_large_as_its_number_of_links() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let r = |n: &str| node(&g, n).radius;
    assert!(r("Dee") > r("Ada") && r("Ada") > r("Eli") && r("Eli") > r("Jo"));
    assert_eq!(r("Dee"), r("Gus"));
    assert_eq!(r("Jo"), r("Kim"));
    for p in points(&g) {
        let Item::Point { radius, .. } = p else { unreachable!() };
        assert!((4.0..=13.0).contains(radius));
    }
}

#[test]
fn readouts_give_the_links_the_total_and_the_value_of_a_line() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let dee = node(&g, "Dee");
    assert_eq!(line(dee, "Links"), Some("4"));
    assert_eq!(line(dee, "Total Messages"), Some("37"));
    assert!(dee.readout.lines.iter().all(|l| l.derived));
    let l = link(&g, "Ada – Ben");
    assert_eq!(line(l, "Messages"), Some("12"));
    assert!(!l.readout.lines[0].derived);
}

#[test]
fn connected_parts_take_their_own_colour() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 2);
    assert_eq!(node(&g, "Dee").series, node(&g, "Jo").series);
    assert_eq!(node(&g, "Kim").series, node(&g, "Lee").series);
    assert_ne!(node(&g, "Dee").series, node(&g, "Kim").series);
    assert!(g.legend.is_some());
    let hidden = layout(
        &ChartBlock::from_value(&spec).unwrap(),
        &rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            hidden: vec!["Group 2".into()],
            ..Options::default()
        },
    );
    assert!(hidden.failure.is_none());
    assert_eq!(observations(&hidden).len(), 10 + 14);
}

#[test]
fn without_a_value_a_link_reads_out_its_ends() {
    let spec = plain();
    let rows = [row("A", "B"), row("B", "C"), row("C", "A")];
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 3 + 3);
    let l = link(&g, "A – B");
    assert_eq!(line(l, "From"), Some("A"));
    assert_eq!(line(l, "To"), Some("B"));
    assert_eq!(node(&g, "A").readout.lines.len(), 1, "no value, no total");
    assert_eq!(line(node(&g, "A"), "Links"), Some("2"));
}

#[test]
fn a_link_to_the_same_node_and_unusable_rows_are_left_out_and_reported() {
    let spec = json!({"type":"network","title":"T","encoding":{"source":"a","target":"b","y":"v"}});
    let rows = [
        json!({"a":"A","b":"B","v":1}),
        json!({"a":"A","b":"A","v":1}),
        json!({"a":"A","b":"C","v":-2}),
        json!({"a":"B","v":2}),
    ];
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 2 + 1);
    let checks: Vec<_> = g.checks.iter().filter(|c| c.kind == "excluded-observations").collect();
    assert_eq!(checks.len(), 2);
    assert!(checks.iter().any(|c| c.message.contains("itself")));
    // A missing value is only a link without a weight.
    let g = draw(
        &spec,
        &[json!({"a":"A","b":"B","v":null}), json!({"a":"B","b":"C","v":4})],
    );
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 3 + 2);
    assert_eq!(line(link(&g, "A – B"), "From"), Some("A"));
}

#[test]
fn bad_input_fails_the_chart_with_the_cause_named() {
    let spec = json!({"type":"network","title":"T","encoding":{"source":"a","target":"b","y":"v"}});
    let g = draw(&spec, &[json!({"a":"A","b":"B","v":"heavy"})]);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
    let g = draw(&spec, &[json!({"a":"A","b":"B","v":1}), json!({"a":"A","b":"B","v":2})]);
    assert!(g.failure.is_some() && check(&g, "source-error").is_some());
    let g = draw(
        &spec,
        &[json!({"a":"A","b":"B","v":1e308}), json!({"a":"B","b":"C","v":1e308})],
    );
    assert!(g.failure.is_some() && check(&g, "value-overflow").is_some());
    let g = draw(
        &json!({"type":"network","title":"T","encoding":{"source":"a"}}),
        &[row("A", "B")],
    );
    assert!(g.failure.is_some());
}

#[test]
fn a_name_that_fits_nowhere_is_reported_never_clipped() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"from": "A person whose name is far too long to sit beside any node", "to": "Dee", "msgs": 1}));
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(check(&g, "label-collision").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A person")));
}

#[test]
fn facets_draw_one_network_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("year");
    let rows: Vec<Value> = ["2025", "2026"]
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
    assert_eq!(g.panels[1].observations.len(), 27);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let spec = plain();
    stays_visible(&spec, &[]);
    stays_visible(&spec, &[row("A", "A")]);
    contract(&spec, &[row("A", "B")]);
    let weighted = json!({"type":"network","title":"T","encoding":{"source":"a","target":"b","y":"v"}});
    let g = draw(
        &weighted,
        &[json!({"a":"A","b":"B","v":1e300}), json!({"a":"B","b":"C","v":1e-300})],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let chain: Vec<Value> = (0..300)
        .map(|i| row(&format!("Node {i}"), &format!("Node {}", i + 1)))
        .collect();
    let g = draw_at(&spec, &chain, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 301 + 300);
    assert!(!g.dropped.is_empty(), "301 names do not fit and are reported");
    assert!(check(&g, "clipping").is_none() && check(&g, "label-collision").is_none());
    let star: Vec<Value> = (0..40).map(|i| row("Hub", &format!("Leaf {i}"))).collect();
    let g = draw_at(&spec, &star, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(line(node(&g, "Hub"), "Links"), Some("40"));
}

#[test]
fn a_network_over_a_limit_is_not_drawn_and_names_the_limit_and_at_it_draws() {
    let rows: Vec<Value> = (0..400)
        .map(|i| row(&format!("n{i}"), &format!("n{}", i + 1)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "nodes");
    assert_eq!(c.details["count"], 401);
    assert_eq!(c.details["limit"], 400);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..46)
        .flat_map(|i| (0..46).map(move |j| row(&format!("a{i}"), &format!("b{j}"))))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "links");
    assert_eq!(c.details["count"], 2116);
    assert_eq!(c.details["limit"], 2000);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..399)
        .map(|i| row(&format!("n{i}"), &format!("n{}", i + 1)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(check(&g, "limit-exceeded").is_none());
    assert!(nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("network", &spec, &rows);
}

#[test]
fn the_runtime_draws_the_positions_the_exact_rows_found() {
    let rows: Vec<Value> = vec![
        json!({"a": "A", "b": "B", "v": 1000}),
        json!({"a": "B", "b": "C", "v": 0.05}),
        json!({"a": "C", "b": "A", "v": 100.04}),
        json!({"a": "C", "b": "D", "v": 100.01}),
        json!({"a": "D", "b": "E", "v": 3}),
    ];
    let weighted = json!({"type":"network","title":"T","encoding":{"source":"a","target":"b","y":"v"}});
    runtime_matches_native(&weighted, &rows);
}
