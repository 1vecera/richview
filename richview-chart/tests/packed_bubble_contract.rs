//! Packed bubbles: circles whose area is the value, packed without overlap, names inside the
//! circles that fit, and the contract every type shares (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/packed-bubble.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"packed-bubble","title":"T","encoding":{"label":"n","y":"v"}})
}

fn parts(vs: &[f64]) -> Vec<Value> {
    vs.iter()
        .enumerate()
        .map(|(i, v)| json!({"n": format!("P{i}"), "v": v}))
        .collect()
}

/// The circles as (centre, radius, observation index).
fn circles(g: &Geometry) -> Vec<([f64; 2], f64, usize)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Point {
                center,
                radius,
                role: Role::Mark,
                obs: Some(o),
                ..
            } => Some((*center, *radius, *o)),
            _ => None,
        })
        .collect()
}

fn labels(g: &Geometry) -> Vec<&richview_chart::geometry::Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("packed-bubble", &spec, &rows);
}

#[test]
fn area_is_proportional_to_the_value_and_circles_never_overlap() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let cs = circles(&g);
        assert_eq!(cs.len(), rows.len());
        let panel = &g.panels[0];
        let k: Vec<f64> = cs
            .iter()
            .map(|(_, r, o)| {
                let row = panel.observations[*o].row.unwrap();
                r * r / rows[row]["spend"].as_f64().unwrap()
            })
            .collect();
        for v in &k {
            // A derived radius keeps six significant digits.
            assert!((v - k[0]).abs() < k[0] * 1e-4, "{k:?}");
        }
        for (i, a) in cs.iter().enumerate() {
            for b in &cs[i + 1..] {
                let d = (a.0[0] - b.0[0]).hypot(a.0[1] - b.0[1]);
                assert!(d >= a.1 + b.1 - 1e-6, "{width}: circles overlap");
            }
        }
    }
}

#[test]
fn a_name_is_written_inside_its_circle_or_reported() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let drawn = labels(&g);
        for t in &drawn {
            let inside = circles(&g).iter().any(|(c, r, _)| {
                t.bounds.inside(
                    &richview_chart::geometry::Rect::new(c[0] - r, c[1] - r, 2.0 * r, 2.0 * r),
                    0.5,
                )
            });
            assert!(inside, "{width}: '{}' is not inside a circle", t.text);
        }
        assert_eq!(
            drawn.len() + g.dropped.iter().filter(|d| d.kind.as_deref() == Some("bubble")).count(),
            rows.len(),
            "{width}: every name is drawn or reported"
        );
        if !g.dropped.is_empty() {
            assert!(check(&g, "label-dropped").is_some());
        }
    }
    let wide = draw_at(&spec, &rows, 1440.0);
    assert!(labels(&wide).iter().any(|t| t.text == "Cloud hosting"));
    let narrow = draw_at(&spec, &rows, 320.0);
    assert!(labels(&narrow).len() < labels(&wide).len());
}

#[test]
fn groups_get_colours_and_a_legend_and_a_hidden_group_leaves() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 5);
    assert!(g.legend.is_some());
    let block = ChartBlock::from_value(&spec).unwrap();
    let hidden = layout(
        &block,
        &rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            hidden: vec!["Operations".into()],
            ..Options::default()
        },
    );
    assert_eq!(circles(&hidden).len(), rows.len() - 4);
    assert!(hidden.failure.is_none());
}

#[test]
fn readouts_name_the_part_its_group_its_value_and_a_derived_share() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Cloud hosting")
        .unwrap();
    assert_eq!(line(o, "Area"), Some("Operations"));
    assert_eq!(line(o, "Spend (k EUR)"), Some("420"));
    let share = o.readout.lines.iter().find(|l| l.name == "Share").unwrap();
    assert!(share.derived && share.value.ends_with('%'));
}

#[test]
fn one_part_is_one_circle_and_equal_parts_are_equal_circles() {
    let g = draw(&spec(), &parts(&[5.0]));
    assert_eq!(circles(&g).len(), 1);
    contract(&spec(), &parts(&[5.0]));
    let g = draw(&spec(), &parts(&[3.0, 3.0, 3.0, 3.0, 3.0]));
    let cs = circles(&g);
    assert!(cs.iter().all(|c| (c.1 - cs[0].1).abs() < 1e-9));
}

#[test]
fn a_panel_over_the_bubble_limit_is_not_drawn_and_names_the_limit() {
    let vs: Vec<f64> = (0..201).map(|i| 1.0 + (i * 7 % 40) as f64).collect();
    let g = draw(&spec(), &parts(&vs));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 201);
    assert_eq!(c.details["limit"], 200);
    assert_eq!(c.details["what"], "bubbles per panel");
    assert!(observations(&g).is_empty());
}

#[test]
fn a_panel_at_the_bubble_limit_draws_every_part() {
    let vs: Vec<f64> = (0..200).map(|i| 1.0 + (i * 7 % 40) as f64).collect();
    let rows = parts(&vs);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 200);
    assert!(nonfinite(&g).is_none());
    contract(&spec(), &parts(&vs[..120]));
}

#[test]
fn zero_negative_and_missing_values_are_left_out_and_bad_text_fails() {
    let mut rows = parts(&[4.0, 2.0, 0.0, -3.0]);
    rows.push(json!({"n": "Gap", "v": null}));
    rows.push(json!({"v": 5.0}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(circles(&g).len(), 2);
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"n": "Bad", "v": "lots"}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &parts(&[0.0, -1.0]));
}

#[test]
fn a_missing_channel_fails_with_a_source_error() {
    let s = json!({"type":"packed-bubble","title":"T","encoding":{"label":"n"}});
    let g = draw(&s, &parts(&[1.0, 2.0]));
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn extreme_ranges_stay_finite_and_tiny_parts_are_reported() {
    let g = draw(&spec(), &parts(&[1e300, 1e-300, 5e299]));
    assert!(nonfinite(&g).is_none());
    assert_eq!(circles(&g).len(), 2);
    assert_eq!(observations(&g).len(), 3);
    assert!(g.note.as_deref().unwrap_or_default().contains("too small"));
    let g = draw(&spec(), &parts(&[1e-300, 3e-300]));
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &parts(&[1.0, 1e6, 3.0, 40.0]));
    assert!(nonfinite(&g).is_none());
}

#[test]
fn an_omitted_part_uses_a_packed_edge_with_room_for_its_selection_ring() {
    let rows = parts(&[1e6, 1.0]);
    for width in WIDTHS {
        let g = draw_at(&spec(), &rows, width);
        let panel = &g.panels[0];
        let (center, radius, _) = circles(&g)[0];
        let omitted = &panel.observations[1];
        assert_eq!(omitted.radius, 0.0);
        let [x, y] = omitted.at;
        assert!(((x - center[0]).hypot(y - center[1]) - radius).abs() < 1e-6);
        assert!(panel.plot.inflate(-19.0).contains(omitted.at));
        assert_eq!(omitted.x, x);
    }
}

#[test]
fn facets_share_one_scale_so_equal_values_are_equal_areas() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for (f, vs) in [("p", [9.0, 4.0, 1.0]), ("q", [4.0, 4.0, 2.0])] {
        for (i, v) in vs.iter().enumerate() {
            rows.push(json!({"n": format!("{f}{i}"), "v": v, "f": f}));
        }
    }
    let g = draw_at(&s, &rows, 1440.0);
    assert_eq!(g.panels.len(), 2);
    let radius = |panel: usize, name: &str| {
        let p = &g.panels[panel];
        let o = p.observations.iter().position(|o| o.readout.heading == name).unwrap();
        p.observations[o].radius
    };
    assert!((radius(0, "p1") - radius(1, "q0")).abs() < 1e-9);
    contract(&s, &rows);
}

#[test]
fn the_runtime_packs_the_parts_the_exact_rows_packed() {
    // 0.05 beside 1000 is zero on the grid, and 100.01 against 100.04 swap places in the order.
    let rows = parts(&[1000.0, 0.05, 100.01, 100.04, 300.5, 7.0]);
    runtime_matches_native(&spec(), &rows);
}
