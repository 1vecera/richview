//! Pie and donut names beside their slices (FEA-CH-08, S27-1017): each complete name and share
//! stands at its slice's middle just outside the disk, without a leader, a constant 12 px from the
//! rim; when any name would meet the disk, another name or the panel's edge, or would have to leave
//! its slice, the whole chart takes the key instead, never a mixed view.

mod common;

use common::*;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Panel, Rect, Role, Text};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};
use std::f64::consts::TAU;

/// The gap between a rim and the nearest edge of a name.
const GAP: f64 = 12.0;

fn spec(inner: f64) -> Value {
    json!({"type":"pie","title":"Where the money goes","encoding":{"x":"item","y":"amount"},"style":{"inner":inner}})
}

/// Four well-separated slices with short names.
fn short() -> Vec<Value> {
    vec![
        json!({"item":"Ads","amount":40}),
        json!({"item":"SEO","amount":30}),
        json!({"item":"Mail","amount":20}),
        json!({"item":"Pay","amount":10}),
    ]
}

/// Four narrow neighbours that start at 12 o'clock, and one slice for the rest.
fn crowded() -> Vec<Value> {
    let mut rows: Vec<Value> = ["North", "South", "East", "West", "Up", "Down", "Left", "Right"]
        .iter()
        .map(|n| json!({"item":n,"amount":1}))
        .collect();
    rows.push(json!({"item":"Everything else","amount":92}));
    rows
}

fn at(spec: &Value, rows: &[Value], width: f64) -> Geometry {
    draw_at(spec, rows, width)
}

/// The names drawn beside the slices.
fn labels(g: &Geometry) -> Vec<&Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect()
}

/// The names in the key.
fn keyed(g: &Geometry) -> Vec<&Text> {
    g.figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Key => Some(t),
            _ => None,
        })
        .collect()
}

fn leaders(g: &Geometry) -> usize {
    g.items().filter(|i| i.role() == Role::Leader).count()
}

/// The disk of a panel: its centre and outer radius.
fn disk(p: &Panel) -> ([f64; 2], f64) {
    let w = p.observations[0].wedge.expect("a pie slice has a wedge");
    (w.center, w.radius)
}

/// How far `rect` stands from the rim of the disk at `centre`: negative inside the disk.
fn rim_gap(rect: &Rect, centre: [f64; 2], radius: f64) -> f64 {
    let dx = (rect.x - centre[0]).max(centre[0] - rect.right()).max(0.0);
    let dy = (rect.y - centre[1]).max(centre[1] - rect.bottom()).max(0.0);
    dx.hypot(dy) - radius
}

/// Whether the ray from the centre through the middle of slice `k` meets `rect`.
fn ray_meets(p: &Panel, k: usize, rect: &Rect) -> bool {
    let w = p.observations[k].wedge.unwrap();
    let mid = (w.start + w.end) / 2.0;
    let (dx, dy) = (mid.sin(), -mid.cos());
    (0..4000).any(|step| {
        let t = w.radius + f64::from(step) * 0.25;
        rect.inflate(1.0).contains([w.center[0] + t * dx, w.center[1] + t * dy])
    })
}

/// The shared promises of names beside the slices of a panel.
fn assert_beside(g: &Geometry, texts: &[&str], at_width: f64) {
    let name = format!("{} at {at_width} px", g.kind);
    assert_eq!(leaders(g), 0, "{name}: no leader lines");
    assert!(keyed(g).is_empty(), "{name}: no key beside names: {:?}", keyed(g));
    let found = labels(g);
    assert_eq!(
        found.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
        texts,
        "{name}: every name and share, complete, in the author's order"
    );
    let figure = Rect::new(0.0, 0.0, g.width, g.height);
    for panel in &g.panels {
        let (centre, radius) = disk(panel);
        let mine: Vec<&&Text> = found
            .iter()
            .filter(|t| panel.plot.contains(t.bounds.center()))
            .collect();
        assert_eq!(mine.len(), texts.len(), "{name}: a name for each slice in the panel");
        for (k, t) in mine.iter().enumerate() {
            let gap = rim_gap(&t.bounds, centre, radius);
            assert!(
                (GAP - 1e-6..=GAP + 2.0).contains(&gap),
                "{name}: {:?} stands {gap} px from the rim",
                t.text
            );
            assert!(
                t.bounds.inside(&panel.plot, 0.0) && t.bounds.inside(&figure, 0.0),
                "{name}: {:?} {:?} leaves the panel {:?}",
                t.text,
                t.bounds,
                panel.plot
            );
            assert!(
                ray_meets(panel, k, &t.bounds),
                "{name}: {:?} is not on its slice's ray",
                t.text
            );
            for u in &mine[k + 1..] {
                assert!(
                    !t.bounds.overlaps(&u.bounds, 0.0),
                    "{name}: {:?} meets {:?}",
                    t.text,
                    u.text
                );
            }
        }
    }
    for kind in ["clipping", "label-collision"] {
        assert!(check(g, kind).is_none(), "{name}: {kind}: {:?}", check(g, kind));
    }
}

#[test]
fn four_separated_slices_are_named_beside_their_slices_without_leaders() {
    for width in [390.0, 720.0, 1280.0] {
        let g = at(&spec(0.0), &short(), width);
        assert_beside(&g, &["Ads 40%", "SEO 30%", "Mail 20%", "Pay 10%"], width);
    }
}

#[test]
fn a_donut_is_named_beside_its_outer_rim() {
    for width in [390.0, 720.0, 1280.0] {
        let g = at(&spec(0.55), &short(), width);
        assert_beside(&g, &["Ads 40%", "SEO 30%", "Mail 20%", "Pay 10%"], width);
        let w = g.panels[0].observations[0].wedge.unwrap();
        assert!(w.inner > 0.0, "a donut keeps its hole");
    }
}

#[test]
fn a_single_slice_is_named_below_its_disk() {
    for inner in [0.0, 0.6] {
        for width in [390.0, 720.0] {
            let rows = [json!({"item":"All","amount":7})];
            let g = at(&spec(inner), &rows, width);
            assert_beside(&g, &["All 100%"], width);
            let (centre, radius) = disk(&g.panels[0]);
            let t = labels(&g)[0];
            assert!(
                t.bounds.y >= centre[1] + radius + GAP - 1e-6,
                "{:?} stands under the disk",
                t.bounds
            );
            assert!(
                (t.bounds.center()[0] - centre[0]).abs() < 0.5,
                "{:?} stands under the middle of the disk",
                t.bounds
            );
        }
    }
}

#[test]
fn several_narrow_neighbours_take_the_key_for_the_whole_chart() {
    let names: Vec<String> = crowded()
        .iter()
        .map(|r| {
            // A share under 10 % keeps its decimal: 1.0%.
            let share = if r["amount"] == 1 { "1.0%" } else { "92%" };
            format!("{} {share}", r["item"].as_str().unwrap())
        })
        .collect();
    for inner in [0.0, 0.55] {
        for width in [390.0, 720.0, 1280.0] {
            let g = at(&spec(inner), &crowded(), width);
            let name = format!("inner {inner} at {width} px");
            assert!(
                labels(&g).is_empty(),
                "{name}: no name beside a slice: {:?}",
                labels(&g)
            );
            assert_eq!(leaders(&g), 0, "{name}: no leader");
            assert_eq!(
                keyed(&g).iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
                names,
                "{name}: every complete name and share in the key"
            );
            // The key stands under the disk, as before.
            let disk_bottom = g.panels[0].plot.bottom();
            assert!(
                keyed(&g).iter().all(|t| t.bounds.y >= disk_bottom),
                "{name}: the key is under the disk"
            );
            assert!(check(&g, "label-collision").is_none() && check(&g, "clipping").is_none());
        }
    }
}

#[test]
fn the_key_stands_beside_the_disk_where_the_room_is_wider_than_tall() {
    let block = ChartBlock::from_value(&spec(0.0)).unwrap();
    let g = layout(
        &block,
        &crowded(),
        &ChartTheme::default(),
        &Options {
            width: 480.0,
            aside: true,
            ..Options::default()
        },
    );
    assert!(labels(&g).is_empty());
    let (centre, radius) = disk(&g.panels[0]);
    let entries = keyed(&g);
    assert_eq!(entries.len(), 9);
    assert!(
        entries.iter().all(|t| t.bounds.x >= centre[0] + radius),
        "the key is beside the disk"
    );
}

#[test]
fn long_names_take_the_key_complete_and_unabbreviated() {
    let rows = [
        json!({"item":"Search engines and aggregators","amount":6}),
        json!({"item":"Referral partners and affiliates","amount":3}),
        json!({"item":"Email campaigns and newsletters","amount":1}),
    ];
    for width in [390.0, 720.0] {
        let g = at(&spec(0.0), &rows, width);
        assert!(labels(&g).is_empty(), "{width}: {:?}", labels(&g));
        let texts: Vec<_> = keyed(&g).iter().map(|t| t.text.clone()).collect();
        assert_eq!(
            texts,
            [
                "Search engines and aggregators 60%",
                "Referral partners and affiliates 30%",
                "Email campaigns and newsletters 10%"
            ]
        );
        assert!(texts.iter().all(|t| !t.contains('…') && !t.contains("...")));
    }
}

#[test]
fn the_disk_is_solved_with_the_names_beside_it_and_the_slices_stay_where_the_shares_put_them() {
    let rows = short();
    let total = 100.0;
    let mut start = 0.0;
    for width in [390.0, 720.0] {
        let g = at(&spec(0.0), &rows, width);
        let plot = g.panels[0].plot;
        let (centre, radius) = disk(&g.panels[0]);
        assert_eq!(centre, plot.center(), "the disk stands in the middle of its panel");
        assert!(radius > 0.45 * plot.w.min(plot.h) / 2.0 && radius <= plot.w.min(plot.h) / 2.0 - 4.0);
        // Names take room: the disk is smaller than the key layout's.
        let crowd = at(&spec(0.0), &crowded(), width);
        let (_, key_radius) = disk(&crowd.panels[0]);
        assert!(radius < key_radius, "{width}: names beside the disk take room from it");
        start = 0.0;
        for (o, amount) in g.panels[0].observations.iter().zip([40.0, 30.0, 20.0, 10.0]) {
            let w = o.wedge.unwrap();
            assert!((w.start - start).abs() < 1e-9, "slices touch");
            // Edges sit on the share grid of TYP-36, so a span is within a step or two of its share.
            assert!(((w.end - w.start) / TAU - amount / total).abs() < 2.0 / 4095.0);
            start = w.end;
        }
    }
    assert!((start - TAU).abs() < 1e-9);
}

#[test]
fn every_style_face_decides_alike_and_draws_the_same_disk() {
    // The built-in styles' data faces: Geist Mono (sleek, air), RichView Code (scientific),
    // Space Mono (brutalist) and Patrick Hand (hand-drawn).
    let faces = [
        "Geist Mono, ui-monospace, monospace",
        "RichView Code, ui-monospace, monospace",
        "Space Mono, ui-monospace, monospace",
        "Patrick Hand, cursive",
    ];
    for chars in 2..16usize {
        let name = |n: &str| format!("{n}{}", "x".repeat(chars.saturating_sub(n.len())));
        let rows: Vec<Value> = [("A", 45), ("B", 25), ("C", 20), ("D", 10)]
            .iter()
            .map(|(n, v)| json!({"item": name(n), "amount": v}))
            .collect();
        for inner in [0.0, 0.5] {
            for width in (300..=900).step_by(20) {
                let width = f64::from(width);
                let block = ChartBlock::from_value(&spec(inner)).unwrap();
                let outcome: Vec<(bool, f64, f64)> = faces
                    .iter()
                    .map(|face| {
                        let g = layout(
                            &block,
                            &rows,
                            &ChartTheme {
                                data_font: (*face).into(),
                                ..ChartTheme::default()
                            },
                            &Options {
                                width,
                                ..Options::default()
                            },
                        );
                        assert!(check(&g, "label-collision").is_none() && check(&g, "clipping").is_none());
                        assert!(g.figure_items.iter().all(|i| i.role() != Role::Leader));
                        let (_, radius) = disk(&g.panels[0]);
                        // A key's rows follow the face's line height, so only the figure of names
                        // beside the slices has a height the faces must agree on.
                        let beside = keyed(&g).is_empty();
                        (beside, radius, if beside { g.height } else { 0.0 })
                    })
                    .collect();
                for (face, found) in faces.iter().zip(&outcome) {
                    assert_eq!(
                        found.0, outcome[0].0,
                        "{face} decides differently at {width} px with {chars}-character names"
                    );
                    assert!(
                        (found.1 - outcome[0].1).abs() < 1e-9 && (found.2 - outcome[0].2).abs() < 1e-9,
                        "{face} draws another disk at {width} px with {chars}-character names: {outcome:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn hiding_a_slice_renames_the_rest_and_showing_it_again_restores_the_first_view() {
    let spec = spec(0.0);
    let rows = short();
    let block = ChartBlock::from_value(&spec).unwrap();
    let with = |hidden: &[&str]| {
        layout(
            &block,
            &rows,
            &ChartTheme::default(),
            &Options {
                width: 720.0,
                hidden: hidden.iter().map(|s| (*s).to_owned()).collect(),
                ..Options::default()
            },
        )
    };
    let first = with(&[]);
    let hidden = with(&["Ads"]);
    let names = |g: &Geometry| labels(g).iter().map(|t| t.text.clone()).collect::<Vec<_>>();
    assert_eq!(names(&hidden), ["SEO 50%", "Mail 33.3%", "Pay 16.7%"]);
    assert_eq!(leaders(&hidden), 0);
    let again = with(&[]);
    assert_eq!(names(&first), names(&again));
    let boxes = |g: &Geometry| labels(g).iter().map(|t| t.bounds).collect::<Vec<_>>();
    assert_eq!(
        boxes(&first),
        boxes(&again),
        "showing the slice again restores every name's place"
    );
}

#[test]
fn facets_share_one_disk_size_and_the_key_is_all_or_none() {
    let mut facet = spec(0.0);
    facet["encoding"]["facet"] = json!("quarter");
    let mut rows = Vec::new();
    for q in ["Q1", "Q2"] {
        for r in short() {
            let mut r = r;
            r["quarter"] = json!(q);
            rows.push(r);
        }
    }
    let g = at(&facet, &rows, 720.0);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(labels(&g).len(), 8, "every panel names its slices");
    assert_eq!(leaders(&g), 0);
    assert!(keyed(&g).is_empty());
    let radii: Vec<f64> = g.panels.iter().map(|p| disk(p).1).collect();
    assert!(
        (radii[0] - radii[1]).abs() < 1e-9,
        "small multiples share a disk size: {radii:?}"
    );
    for panel in &g.panels {
        let (centre, radius) = disk(panel);
        for t in labels(&g).iter().filter(|t| panel.plot.contains(t.bounds.center())) {
            assert!(rim_gap(&t.bounds, centre, radius) >= GAP - 1e-6);
            assert!(t.bounds.inside(&panel.plot, 0.0));
        }
    }
    // One crowded panel takes the key for the chart: no panel keeps its names.
    let mut mixed = rows[..4].to_vec();
    for r in crowded() {
        let mut r = r;
        r["quarter"] = json!("Q2");
        mixed.push(r);
    }
    let g = at(&facet, &mixed, 720.0);
    assert!(labels(&g).is_empty(), "never a mixed view: {:?}", labels(&g));
    assert!(!keyed(&g).is_empty());
}

#[test]
fn the_contract_holds_for_names_beside_the_slices() {
    contract(&spec(0.0), &short());
    contract(&spec(0.55), &short());
    contract(&spec(0.0), &[json!({"item":"All","amount":7})]);
    contract(&spec(0.0), &crowded());
}
