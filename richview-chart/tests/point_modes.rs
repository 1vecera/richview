//! Every-point geometry and explicit reductions (FEA-CH-17). Readout counts alone cannot prove
//! this contract: a reduced path already keeps a readout for every observation.
use super::common;

use common::*;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use richview_chart::layout::{ChartTheme, Options, Prepared, layout, layout_prepared};
use serde_json::{Value, json};

fn spec(kind: &str, points: Option<&str>) -> Value {
    let mut block = json!({"type":kind,"title":"Every observation matters",
        "encoding":{"x":"x","y":"y"},"style":{"curve":"linear"}});
    if let Some(points) = points {
        block["style"]["points"] = json!(points);
    }
    block
}

fn rows(n: usize) -> Vec<Value> {
    (0..n).map(|i| json!({"x":i,"y":(i * 37 % 101) + 1})).collect()
}

#[test]
fn reused_scatter_workspace_keeps_no_pixel_selection_from_another_view() {
    let count = richview_chart::limits::max("scatter", "all-points observations") + 1;
    let mut source = rows(count);
    for (i, row) in source.iter_mut().enumerate() {
        row["group"] = json!(format!("g{}", i % 3));
    }
    let mut value = spec("scatter", None);
    value["encoding"]["color"] = json!("group");
    let block = ChartBlock::from_value(&value).unwrap();
    let theme = ChartTheme::default();
    for hidden in [vec![], vec!["g0".into()]] {
        let prepared = Prepared::default();
        for (width, zoom) in [
            (640.0, None),
            (390.0, Some([2500.0, 2510.0])),
            (1440.0, Some([7500.0, 7600.0])),
            (320.0, None),
        ] {
            let opts = Options {
                width,
                zoom,
                hidden: hidden.clone(),
                ..Options::default()
            };
            assert_eq!(
                layout_prepared(&block, &source, &theme, &opts, &prepared),
                layout(&block, &source, &theme, &opts),
                "width {width}, zoom {zoom:?}, hidden {hidden:?}"
            );
        }
    }
}

fn vertices(g: &Geometry) -> Vec<Vec<[f64; 2]>> {
    marks(g)
        .into_iter()
        .filter_map(|item| match item {
            Item::Line { path, .. } => Some(
                path.cmds
                    .iter()
                    .filter_map(|cmd| match cmd {
                        Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => Some(*p),
                        Cmd::Z => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn all_and_the_default_connect_every_observation_instead_of_only_keeping_readouts() {
    let rows = rows(4_000);
    for kind in ["line", "area"] {
        for points in [None, Some("all")] {
            let g = draw_at(&spec(kind, points), &rows, 390.0);
            assert!(g.failure.is_none(), "{:?}", g.failure);
            let paths = vertices(&g);
            assert_eq!(paths.len(), 1);
            let observations = observations(&g);
            let expected: Vec<_> = observations.iter().map(|o| o.dots[0].at).collect();
            assert_eq!(paths[0].len(), rows.len());
            assert!(paths[0] == expected, "{kind} {points:?} must draw every vertex");
            assert!(check(&g, "aggregation").is_none());
            assert!(check(&g, "decimation").is_none());
            // Flattening straight paths for WebGL retains even subpixel vertices.
            let Item::Line { path, .. } = marks(&g).into_iter().find(|i| matches!(i, Item::Line { .. })).unwrap()
            else {
                unreachable!()
            };
            assert_eq!(path.flatten(1.0)[0], expected);
        }
    }
}

#[test]
fn each_reduction_reports_what_it_replaces_without_changing_readouts() {
    let rows = rows(4_000);
    for kind in ["line", "area"] {
        let full = draw_at(&spec(kind, Some("all")), &rows, 390.0);
        for mode in ["m4", "band"] {
            let g = draw_at(&spec(kind, Some(mode)), &rows, 390.0);
            assert_eq!(observations(&g), observations(&full), "{kind} {mode}");
            let kept: usize = vertices(&g).iter().map(Vec::len).sum();
            assert!(kept < rows.len(), "the fixture must exercise {mode}");
            let reduction = check(&g, "decimation").expect("every reduction is disclosed");
            assert_eq!(reduction.level, richview_chart::Level::Info);
            assert_eq!(reduction.details["kept"], kept);
            assert_eq!(reduction.details["total"], rows.len());
            assert!(reduction.details["perPixel"].as_f64().unwrap() > 6.0);
            assert_eq!(check(&g, "aggregation").is_some(), mode == "band");
            assert_eq!(g.items().any(|item| item.role() == Role::Band), mode == "band");
        }
    }
    for mode in ["all", "m4", "band"] {
        let g = draw(&spec("line", Some(mode)), &rows[..3]);
        assert!(check(&g, "decimation").is_none(), "unreduced {mode}");
    }
}

#[test]
fn all_keeps_missing_runs_and_zoom_boundary_neighbours() {
    let mut rows = rows(1_000);
    rows[500]["y"] = Value::Null;
    for kind in ["line", "area"] {
        let block = ChartBlock::from_value(&spec(kind, Some("all"))).unwrap();
        let g = draw(&spec(kind, Some("all")), &rows);
        let paths = vertices(&g);
        assert_eq!(paths.iter().map(Vec::len).collect::<Vec<_>>(), [500, 499]);
        let zoom = layout(
            &block,
            &rows,
            &ChartTheme::default(),
            &Options {
                width: 390.0,
                zoom: Some([200.5, 300.5]),
                ..Options::default()
            },
        );
        assert_eq!(vertices(&zoom)[0].len(), 102, "x=200..301 continue to both edges");
        assert_eq!(observations(&zoom).len(), 100, "readouts stay inside the viewport");
    }
}

#[test]
fn stacked_areas_keep_every_vertex_on_both_sides_of_each_layer() {
    let rows: Vec<_> = rows(1_000)
        .into_iter()
        .flat_map(|mut a| {
            a["group"] = json!("A");
            let mut b = a.clone();
            b["group"] = json!("B");
            [a, b]
        })
        .collect();
    let mut block = spec("area", Some("all"));
    block["encoding"]["color"] = json!("group");
    block["stack"] = json!(true);
    let g = draw_at(&block, &rows, 390.0);
    assert_eq!(vertices(&g).iter().map(Vec::len).collect::<Vec<_>>(), [1_000, 1_000]);
    let fills: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|item| match item {
            Item::Area { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 2);
    assert!(fills.iter().all(|path| path.cmds.len() == 2_001));
}

#[test]
fn scatter_keeps_one_mark_per_row_even_when_points_coincide() {
    let rows = vec![json!({"x":1,"y":2}); 1_000];
    let g = draw_at(&spec("scatter", Some("all")), &rows, 390.0);
    assert_eq!(marks(&g).len(), rows.len());
    assert_eq!(observations(&g).len(), rows.len());
    assert!(check(&g, "decimation").is_none());
}

#[test]
fn calendar_axes_and_facets_keep_the_same_every_point_policy() {
    let rows: Vec<_> = rows(1_000)
        .into_iter()
        .enumerate()
        .map(|(i, mut row)| {
            row["x"] = json!(format!("2026-01-01T{:02}:{:02}", i / 60, i % 60));
            row["facet"] = json!(i / 500);
            row
        })
        .collect();
    for kind in ["line", "area"] {
        let mut block = spec(kind, Some("all"));
        block["encoding"]["facet"] = json!("facet");
        let g = draw_at(&block, &rows, 390.0);
        assert!(g.failure.is_none(), "{:?}", g.failure);
        assert_eq!(g.panels.len(), 2);
        assert_eq!(vertices(&g).iter().map(Vec::len).sum::<usize>(), rows.len());
        assert!(check(&g, "decimation").is_none());
    }
}

#[test]
fn a_ceiling_fallback_is_disclosed_even_in_a_narrow_zoom() {
    for kind in ["line", "area", "scatter"] {
        let max = richview_chart::limits::max(kind, "all-points observations");
        let rows = rows(max + 1);
        let block = ChartBlock::from_value(&spec(kind, None)).unwrap();
        let options = Options {
            width: 390.0,
            zoom: Some([10.0, 20.0]),
            ..Options::default()
        };
        let g = layout(&block, &rows, &ChartTheme::default(), &options);
        if kind == "scatter" {
            assert!(g.failure.is_none());
            let reduction = check(&g, "decimation").expect("scatter policy is disclosed after zoom");
            assert_eq!(reduction.details["automatic"], true);
            assert_eq!(reduction.details["total"], 11);
            assert_eq!(reduction.details["kept"], 11);
        } else {
            assert!(g.failure.is_none(), "an automatic M4 fallback keeps the chart usable");
            let reduction = check(&g, "decimation").expect("fallback is always disclosed");
            assert_eq!(reduction.details["automatic"], true);
            assert_eq!(
                reduction.details["total"], 13,
                "the drawn run includes two boundary neighbours"
            );
            assert_eq!(reduction.details["kept"], 13);
            assert_eq!(observations(&g).len(), 11);
        }
        let g = layout(&block, &rows[..max], &ChartTheme::default(), &options);
        assert!(g.failure.is_none(), "{kind} accepts the declared boundary");
        assert!(check(&g, "decimation").is_none(), "the boundary retains every point");
        let explicit = ChartBlock::from_value(&spec(kind, Some("all"))).unwrap();
        let g = layout(&explicit, &rows, &ChartTheme::default(), &Options::default());
        assert!(g.failure.is_none());
        assert!(check(&g, "decimation").is_none(), "explicit all has no ceiling");
        let positions = if kind == "scatter" {
            marks(&g).len()
        } else {
            vertices(&g)[0].len()
        };
        assert_eq!(positions, max + 1);
    }
}

#[test]
fn automatic_scatter_keeps_each_occupied_cell_per_series_and_extremes_without_losing_readouts() {
    let max = richview_chart::limits::max("scatter", "all-points observations");
    let mut rows: Vec<_> = (0..max + 1)
        .map(|i| json!({"x":i % 3,"y":i % 3,"group":i % 2}))
        .collect();
    rows.extend([json!({"x":-100,"y":1,"group":0}), json!({"x":1,"y":100,"group":1})]);
    let mut full = spec("scatter", Some("all"));
    full["encoding"]["color"] = json!("group");
    let g = draw_at(&full, &rows, 390.0);
    full["style"].as_object_mut().unwrap().remove("points");
    let reduced = draw_at(&full, &rows, 390.0);
    assert_eq!(observations(&g), observations(&reduced));
    let point = |i: &Item| match i {
        Item::Point { center, series, .. } => Some((*series, *center)),
        _ => None,
    };
    let original: Vec<_> = marks(&g).into_iter().filter_map(point).collect();
    let kept: Vec<_> = marks(&reduced).into_iter().filter_map(point).collect();
    assert!(kept.len() < 10, "coincident points share their own series' cell");
    for (series, center) in &original {
        assert!(
            kept.iter()
                .any(|(s, c)| s == series && c.map(f64::floor) == center.map(f64::floor))
        );
    }
    assert!(kept.contains(original.last().unwrap()));
    assert!(kept.contains(&original[original.len() - 2]));
    assert_eq!(
        kept,
        marks(&draw_at(&full, &rows, 390.0))
            .into_iter()
            .filter_map(point)
            .collect::<Vec<_>>(),
        "deterministic source order"
    );
    let check = check(&reduced, "decimation").unwrap();
    assert_eq!(check.details["total"], rows.len());
    assert_eq!(check.details["kept"], kept.len());
    assert!(check.details["perPixel"].is_number());
}

#[test]
fn reduced_bubbles_keep_first_largest_cell_winner_and_the_final_occupied_cell() {
    let max = richview_chart::limits::max("scatter", "all-points observations");
    let mut rows = vec![
        json!({"x":0,"y":0,"n":1}),
        json!({"x":10,"y":10,"n":100}),
        json!({"x":2,"y":2,"n":4}),
        json!({"x":2,"y":2,"n":9}),
        json!({"x":4,"y":4,"n":16}),
        json!({"x":2,"y":2,"n":25}),
        json!({"x":2,"y":2,"n":25}),
    ];
    rows.extend((0..max).map(|_| json!({"x":2,"y":2,"n":1})));
    let last = rows.len();
    rows.push(json!({"x":6,"y":4,"n":9}));
    let mut value = spec("scatter", None);
    value["encoding"]["size"] = json!("n");
    for width in [390.0, 1440.0] {
        let g = draw_at(&value, &rows, width);
        assert_eq!(observations(&g).len(), rows.len());
        let kept: Vec<_> = marks(&g)
            .into_iter()
            .filter_map(|item| match item {
                Item::Point { obs: Some(index), .. } => g.panels[0].observations[*index].row,
                _ => None,
            })
            .collect();
        // Rows0/1 supply every global extreme. Interior cells must retain rows4/last;
        // the revisited (2,2) cell keeps row5, the first of its two largest bubbles.
        assert_eq!(kept, [0, 1, 4, 5, last]);
    }
}

#[test]
fn reduced_bubbles_keep_actual_radii_and_complete_observation_indexes() {
    let max = richview_chart::limits::max("scatter", "all-points observations");
    let mut rows = vec![
        json!({"x":0,"y":0,"n":0}),
        json!({"x":1,"y":1,"n":1e-8}),
        json!({"x":2,"y":2,"n":4}),
        json!({"x":3,"y":3,"n":16}),
    ];
    rows.extend((0..=max).map(|_| json!({"x":2,"y":2,"n":1})));
    let mut value = spec("scatter", None);
    value["encoding"]["size"] = json!("n");
    let block = ChartBlock::from_value(&value).unwrap();
    for zoom in [None, Some([0.5, 2.5])] {
        let opts = Options {
            width: 390.0,
            zoom,
            ..Options::default()
        };
        let theme = ChartTheme::default();
        let g = layout(&block, &rows, &theme, &opts);
        let kept: Vec<_> = marks(&g)
            .into_iter()
            .filter_map(|item| match item {
                Item::Point {
                    radius,
                    obs: Some(index),
                    ..
                } => Some((*index, *radius)),
                _ => None,
            })
            .collect();
        assert!(kept.len() < 10, "coincident bubbles share representatives");
        assert!(
            kept.iter()
                .all(|(index, _)| g.panels[0].observations[*index].row != Some(0))
        );
        let expected = if zoom.is_some() {
            vec![(0, 1), (1, 2)]
        } else {
            vec![(1, 1), (2, 2), (3, 3)]
        };
        let radii: Vec<_> = expected
            .into_iter()
            .map(|(index, row)| {
                let (_, radius) = kept
                    .iter()
                    .find(|(at, _)| *at == index)
                    .expect("required representative");
                assert_eq!(
                    g.panels[0].observations[index].row,
                    Some(row),
                    "a mark links to its source row"
                );
                *radius
            })
            .collect();
        // Areas follow n: sqrt(1e-8 / 4) = 1 / 20,000, independently of the radius cap.
        assert!(
            (radii[0] / radii[1] - 1.0 / 20_000.0).abs() < 1e-12,
            "bubble areas keep the authored size ratio"
        );
    }
}
