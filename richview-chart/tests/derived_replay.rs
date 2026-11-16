//! Reader interactions reuse the selected author state's exact-derived structure (TYP-36).

mod common;

use common::{draw, line, observations};
use richview_chart::block::ChartBlock;
use richview_chart::geometry::Geometry;
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn replay(spec: &Value, shown: &[Value], native: &Geometry, hidden: &[&str], zoom: Option<[f64; 2]>) -> Geometry {
    layout(
        &ChartBlock::from_value(spec).unwrap(),
        shown,
        &ChartTheme::default(),
        &Options {
            width: 360.0,
            hidden: hidden.iter().map(|s| (*s).to_owned()).collect(),
            zoom,
            derived: native.derived.clone(),
            ..Options::default()
        },
    )
}

#[test]
fn automatic_values_follow_the_exact_visible_readout_across_replay_and_zoom() {
    use richview_chart::geometry::{Face, Item, Role};
    for kind in ["line", "area"] {
        // Give decimal text a clear band: this contract tests replay, not a cramped fit.
        let spec = json!({"type":kind,"title":"Output","style":{"ratio":1},"axes":{"y":{"min":-100,"max":100}},"encoding":{"x":"x","y":"v","color":"s"}});
        let sign = if kind == "area" { -1.0 } else { 1.0 };
        let second = if kind == "area" { 28.2 } else { 42.2 };
        let rows: Vec<_> = (0..5)
            .flat_map(|x| {
                [
                    json!({"x":x,"v":12.1+f64::from(x),"s":"A"}),
                    json!({"x":x,"v":(second-f64::from(x))*sign,"s":"B"}),
                ]
            })
            .collect();
        let native = draw(&spec, &rows);
        for hidden in [vec![], vec!["B"]] {
            // The zoom keeps the series' own last values in view: a window labels a series' own
            // first, last, lowest and highest values that it shows, never the window's (FEA-CH-28).
            for zoom in [None, Some([1.0, 4.0])] {
                let g = replay(&spec, &rows, &native, &hidden, zoom);
                for p in &g.panels {
                    let labels: Vec<_> = p
                        .items
                        .iter()
                        .filter_map(|i| match i {
                            Item::Text(t) if t.role == Role::Label && t.face == Face::Figures => Some(t),
                            _ => None,
                        })
                        .collect();
                    assert!(
                        !labels.is_empty(),
                        "{kind}/{hidden:?}/{zoom:?}: visible last values fit; plot {:?}; drops {:?}",
                        p.plot,
                        g.dropped
                    );
                    for t in labels {
                        assert!(p.observations.iter().any(|o| {
                            o.readout
                                .lines
                                .iter()
                                .any(|l| l.series == t.series && l.value == t.text)
                        }));
                        assert!(t.mark.as_ref().is_some_and(|m| g.mark_ids().contains(m)));
                        assert!(!g.series[t.series.unwrap()].hidden);
                    }
                }
            }
        }
    }
}

#[test]
fn hiding_a_histogram_series_keeps_the_exact_bin_membership() {
    let spec = json!({"type":"histogram","title":"T","encoding":{"x":"v","color":"g"},
        "binning":{"edges":[0,10,20,30]}});
    let mut rows = vec![
        json!({"v":1,"g":"A"}),
        json!({"v":2,"g":"A"}),
        json!({"v":9.996,"g":"A"}),
        json!({"v":11,"g":"B"}),
        json!({"v":12,"g":"B"}),
        json!({"v":25,"g":"B"}),
    ];
    let native = draw(&spec, &rows);
    rows[2]["v"] = json!(10);
    for zoom in [None, Some([0.0, 20.0])] {
        let g = replay(&spec, &rows, &native, &["B"], zoom);
        let counts: Vec<_> = observations(&g)
            .into_iter()
            .map(|o| line(o, "Count").unwrap())
            .collect();
        assert_eq!(counts, ["3", "0", "0"], "zoom {zoom:?}");
    }
}

#[test]
fn histogram_summary_counts_the_same_visible_replayed_tallies_as_its_bars() {
    let spec = json!({"type":"histogram","title":"T","encoding":{"x":"v","color":"g"},
        "binning":{"edges":[0,10,20,30]}});
    let mut rows = vec![
        json!({"g":"A","v":11}),
        json!({"g":"A","v":12}),
        json!({"g":"A","v":25}),
        json!({"g":"B","v":1}),
        json!({"g":"B","v":2}),
        json!({"g":"B","v":9.996}),
    ];
    let native = draw(&spec, &rows);
    rows[5]["v"] = json!(10);
    for (hidden, counts, summary) in [
        (
            vec!["B"],
            ["0", "2", "1"],
            "3 observations from 0 to 30; the fullest interval, 10 to 20, holds 2.",
        ),
        (
            vec!["A"],
            ["3", "0", "0"],
            "3 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.",
        ),
        (vec!["A", "B"], ["0", "0", "0"], "No observations."),
        (
            vec![],
            ["3", "2", "1"],
            "6 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.",
        ),
    ] {
        let g = replay(&spec, &rows, &native, &hidden, None);
        assert_eq!(
            observations(&g)
                .iter()
                .map(|o| line(o, "Count").unwrap())
                .collect::<Vec<_>>(),
            counts
        );
        assert_eq!(g.summary, summary, "hidden {hidden:?}");
    }
}

#[test]
fn zoom_does_not_rerank_near_equal_pareto_values() {
    let spec = json!({"type":"pareto","title":"T","encoding":{"x":"name","y":"v"}});
    let mut rows = vec![
        json!({"name":"Alpha","v":1004}),
        json!({"name":"Beta","v":1006}),
        json!({"name":"Gamma","v":20}),
    ];
    let native = draw(&spec, &rows);
    rows[0]["v"] = json!(1000);
    rows[1]["v"] = json!(1000);
    let g = replay(&spec, &rows, &native, &[], Some([0.0, 2.0]));
    let names: Vec<_> = observations(&g)
        .into_iter()
        .map(|o| o.readout.heading.as_str())
        .collect();
    assert_eq!(names, ["Beta", "Alpha", "Gamma"]);
}

#[test]
fn hiding_a_bar_series_refits_even_when_replaying_the_full_state() {
    let spec = json!({"type":"bar","title":"T","encoding":{"x":"name","y":"v","color":"g"}});
    let rows = vec![
        json!({"name":"one","v":1,"g":"A"}),
        json!({"name":"one","v":100,"g":"B"}),
    ];
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["B"], None);
    assert_eq!(g.fit.y, Some([1.0, 1.0]));
    let mut no_records = native.clone();
    no_records.derived.clear();
    assert_eq!(replay(&spec, &rows, &no_records, &["B"], None).fit.y, Some([1.0, 1.0]));
}

#[test]
fn hidden_qq_groups_keep_the_recorded_axes_quantiles_and_lines() {
    let spec = json!({"type":"qq","title":"T","encoding":{"y":"v","color":"g"}});
    let rows: Vec<_> = [("A", 1, 4), ("B", 1000, 8)]
        .into_iter()
        .flat_map(|(g, unit, n)| (1..=n).map(move |v| json!({"g":g,"v":v*unit})))
        .collect();
    let native = replay(&spec, &rows, &draw(&spec, &rows), &[], None);
    let mut shown = rows.clone();
    // A replay must not compute the line or extent from these quantized/shown values.
    for row in &mut shown {
        row["v"] = json!(10);
    }
    for hidden in [vec!["A"], vec!["B"], vec!["A", "B"], vec![]] {
        let g = replay(&spec, &shown, &native, &hidden, None);
        assert_eq!(g.derived, native.derived);
        assert_eq!(g.fit, native.fit, "hidden {hidden:?}");
        let count = if hidden.contains(&"A") { 0 } else { 4 } + if hidden.contains(&"B") { 0 } else { 8 };
        assert_eq!(
            g.summary,
            format!(
                "Normal quantile plot of {count} observations{}.",
                if hidden.is_empty() { " in 2 groups" } else { "" }
            )
        );
        let reference_lines = |g: &Geometry| {
            g.panels[0]
                .items
                .iter()
                .filter_map(|item| match item {
                    richview_chart::geometry::Item::Line {
                        path,
                        role: richview_chart::geometry::Role::Annotation,
                        series,
                        ..
                    } if !hidden.contains(&if *series == Some(0) { "A" } else { "B" }) => Some(path.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(reference_lines(&g), reference_lines(&native));
        let expected: Vec<_> = observations(&native)
            .into_iter()
            .filter(|o| !hidden.contains(&if o.series == Some(0) { "A" } else { "B" }))
            .map(|o| (o.row, line(o, "Theoretical quantile"), line(o, "Rank")))
            .collect();
        assert_eq!(
            observations(&g)
                .into_iter()
                .map(|o| (o.row, line(o, "Theoretical quantile"), line(o, "Rank")))
                .collect::<Vec<_>>(),
            expected
        );
    }
    let mut no_records = native.clone();
    no_records.derived.clear();
    assert_eq!(replay(&spec, &rows, &no_records, &["B"], None).fit, native.fit);
}

#[test]
fn venn_hides_keep_the_recorded_areas_and_overlap_solution() {
    use richview_chart::geometry::{Item, Role};
    let spec = json!({"type":"venn","title":"T","encoding":{"sets":"s","y":"v"}});
    let rows = vec![
        json!({"s":"A","v":123.456789}),
        json!({"s":"B","v":98.7654321}),
        json!({"s":"A&B","v":45.6789123}),
    ];
    let native = replay(&spec, &rows, &draw(&spec, &rows), &[], None);
    let shown: Vec<_> = rows.iter().map(|r| json!({"s":r["s"],"v":100})).collect();
    for hidden in [vec!["A"], vec!["B"], vec!["A", "B"], vec![]] {
        let g = replay(&spec, &shown, &native, &hidden, None);
        assert_eq!(g.derived, native.derived);
        let circles = |g: &Geometry| {
            g.panels[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Area {
                        path,
                        role: Role::Mark,
                        series: Some(series),
                        ..
                    } if !hidden.contains(&if *series == 0 { "A" } else { "B" }) => Some(path.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            circles(&g),
            circles(&native),
            "hidden {hidden:?}: the solver and sizes replay together"
        );
    }
}

#[test]
fn density_hides_and_zoom_clip_exact_recorded_curves_and_readouts() {
    let spec = json!({"type":"density","title":"T","encoding":{"x":"v","color":"g"},"style":{"bandwidth":0.5}});
    let rows = vec![
        json!({"v":0,"g":"A"}),
        json!({"v":1,"g":"A"}),
        json!({"v":10,"g":"B"}),
        json!({"v":11,"g":"B"}),
    ];
    let native = replay(&spec, &rows, &draw(&spec, &rows), &[], None);
    let shown: Vec<_> = rows.iter().map(|r| json!({"v":0.5,"g":r["g"]})).collect();
    for hidden in [vec![], vec!["A"], vec!["B"], vec!["A", "B"]] {
        for zoom in [None, Some([0.0, 1.0]), Some([0.4, 0.6])] {
            let expected = replay(&spec, &rows, &native, &hidden, zoom);
            let actual = replay(&spec, &shown, &native, &hidden, zoom);
            assert_eq!(actual.derived, native.derived);
            assert_eq!(actual.fit, native.fit, "hidden {hidden:?}, zoom {zoom:?}");
            assert_eq!(actual.panels[0].items, expected.panels[0].items);
            assert_eq!(
                observations(&actual),
                observations(&expected),
                "readouts and dots come from the exact recorded estimate"
            );
            if let Some([a, b]) = zoom {
                let x = actual.panels[0].x.as_ref().unwrap();
                assert_eq!(x.domain, [a, b]);
                assert!(
                    observations(&actual)
                        .iter()
                        .all(|o| o.x >= x.range[0] && o.x <= x.range[1])
                );
            }
        }
    }
}

#[test]
fn density_zoom_retains_readouts_for_a_narrow_group_on_a_wide_shared_scale() {
    let spec = json!({"type":"density","title":"T","digits":6,"encoding":{"x":"v","color":"g"}});
    let mut rows: Vec<_> = [100.01, 100.04]
        .into_iter()
        .map(|v| json!({"v":v,"g":"close"}))
        .collect();
    rows.extend(
        [0.0, 250.0, 500.0, 750.0, 1000.0]
            .into_iter()
            .map(|v| json!({"v":v,"g":"wide"})),
    );
    let native = draw(&spec, &rows);
    let zoomed = replay(&spec, &rows, &native, &[], Some([100.0, 100.05]));
    assert!(
        observations(&zoomed)
            .iter()
            .any(|o| line(o, "close · Density").is_some()),
        "the fine anchors cover the narrow curve as well as the wide one"
    );
}

#[test]
fn constant_ungridded_channels_keep_the_same_positions_from_shown_rows() {
    let fixtures = [
        (
            json!({"type":"scatter3d","title":"T","encoding":{"x":"x","y":"y","z":"z"}}),
            vec![json!({"x":100.04,"y":0,"z":1}), json!({"x":100.04,"y":1,"z":2})],
            vec!["x"],
        ),
        (
            json!({"type":"vector","title":"T","encoding":{"x":"x","y":"y","angle":"a"}}),
            vec![json!({"x":100.04,"y":0,"a":45}), json!({"x":100.04,"y":1,"a":90})],
            vec!["x"],
        ),
        (
            json!({"type":"gantt","title":"T","encoding":{"label":"t","start":"s","end":"e"}}),
            vec![json!({"t":"A","s":100.04,"e":100.04})],
            vec!["s", "e"],
        ),
    ];
    for (spec, rows, fields) in fixtures {
        let native = replay(&spec, &rows, &draw(&spec, &rows), &[], None);
        let mut shown = rows.clone();
        for row in &mut shown {
            for field in &fields {
                row[*field] = json!(100);
            }
        }
        let g = replay(&spec, &shown, &native, &[], None);
        let positions = |g: &Geometry| observations(g).iter().map(|o| o.at).collect::<Vec<_>>();
        assert_eq!(
            positions(&g),
            positions(&native),
            "{}: no-grid numeric positions use shown values",
            spec["type"]
        );
        assert_eq!(g.fit, native.fit);
    }
}

#[test]
fn gantt_replay_keeps_an_exact_reversed_task_invalid_when_the_grid_ties_its_ends() {
    let spec = json!({"type":"gantt","title":"T","encoding":{"label":"t","start":"s","end":"e"}});
    let rows = vec![
        json!({"t":"reversed","s":100.04,"e":100.01}),
        json!({"t":"range","s":0,"e":1000}),
    ];
    let native = draw(&spec, &rows);
    assert!(native.failure.is_some());
    let mut shown = rows.clone();
    shown[0]["s"] = json!(100);
    shown[0]["e"] = json!(100);
    assert_eq!(
        replay(&spec, &shown, &native, &[], None).failure,
        native.failure,
        "a grid tie must not turn a reversed interval into a valid milestone"
    );
}

#[test]
fn gantt_reversed_task_validity_follows_rows_across_interleaved_facets() {
    let spec = json!({"type":"gantt","title":"T","encoding":{"label":"t","start":"s","end":"e","facet":"f"}});
    let rows = vec![
        json!({"t":"rangeA","s":0,"e":1000,"f":"A"}),
        json!({"t":"badB","s":100.04,"e":100.01,"f":"B"}),
        json!({"t":"badA","s":200.04,"e":200.01,"f":"A"}),
        json!({"t":"rangeB","s":0,"e":1000,"f":"B"}),
    ];
    let native = draw(&spec, &rows);
    assert!(
        native.panels.iter().all(|p| p.failure.is_some()),
        "both facets contain a reversed task, irrespective of grouped traversal order"
    );
    let mut shown = rows.clone();
    for (i, v) in [(1, 100), (2, 200)] {
        shown[i]["s"] = json!(v);
        shown[i]["e"] = json!(v);
    }
    let actual = replay(&spec, &shown, &native, &[], None);
    assert_eq!(actual.failure, native.failure);
    assert!(actual.panels.iter().all(|p| p.failure.is_some()));
}

#[test]
fn hidden_stream_and_ribbon_groups_keep_the_full_state_scale() {
    let rows = vec![
        json!({"x":1,"v":100,"g":"A"}),
        json!({"x":1,"v":1,"g":"B"}),
        json!({"x":2,"v":80,"g":"A"}),
        json!({"x":2,"v":2,"g":"B"}),
    ];
    for kind in ["streamgraph", "ribbon"] {
        let spec = json!({"type":kind,"title":"T","encoding":{"x":"x","y":"v","color":"g"}});
        let native = draw(&spec, &rows);
        assert!(native.failure.is_none(), "{kind}: {:?}", native.failure);
        let g = replay(&spec, &rows, &native, &["A"], None);
        assert_eq!(g.fit.y, native.fit.y, "{kind} replay");
        let mut no_records = native.clone();
        no_records.derived.clear();
        assert_eq!(
            replay(&spec, &rows, &no_records, &["A"], None).fit.y,
            native.fit.y,
            "{kind} fallback"
        );
    }
}

#[test]
fn shares_of_visible_hierarchy_parts_follow_the_shown_total() {
    let spec = json!({"type":"treemap","title":"T","encoding":{"label":"name","parent":"parent","y":"v"}});
    let rows = vec![json!({"name":"A","v":3}), json!({"name":"B","v":7})];
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["A"], None);
    let obs = observations(&g);
    let b = obs.iter().find(|o| o.readout.heading == "B").unwrap();
    assert_eq!(line(b, "Share of the visible"), Some("100%"));
    let tile = b.area.unwrap();
    let plot = g.panels[0].plot;
    assert!((tile.w - plot.w).abs() < 0.01 && (tile.h - plot.h).abs() < 0.01 && tile.inside(&plot, 0.01));
    let restored = replay(&spec, &rows, &native, &[], None);
    let original_tiles = native.derived.iter().find(|d| d.key == "treemap.tiles").unwrap();
    assert_eq!(
        restored.derived.iter().find(|d| d.key == "treemap.tiles").unwrap(),
        original_tiles
    );
}

#[test]
fn ribbon_summary_counts_only_changes_in_visible_rank() {
    let spec = json!({"type":"ribbon","title":"T","encoding":{"x":"x","y":"v","color":"g"}});
    let rows = vec![
        json!({"x":"one","v":3,"g":"A"}),
        json!({"x":"one","v":2,"g":"B"}),
        json!({"x":"one","v":1,"g":"C"}),
        json!({"x":"two","v":0.5,"g":"A"}),
        json!({"x":"two","v":2,"g":"B"}),
        json!({"x":"two","v":1,"g":"C"}),
    ];
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["A"], None);
    assert!(g.summary.contains("ranks change 0 times"), "{}", g.summary);
    assert!(
        observations(&g)
            .iter()
            .all(|o| matches!(line(o, "Rank"), Some("1 of 2" | "2 of 2")))
    );
}

#[test]
fn hidden_treemap_areas_use_the_same_values_as_the_readouts() {
    let spec = json!({"type":"treemap","title":"T","encoding":{"label":"name","parent":"parent","y":"v"}});
    let rows = vec![
        json!({"name":"H","v":10}),
        json!({"name":"A","v":1001.3}),
        json!({"name":"B","v":1008.7}),
    ];
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["H"], None);
    let obs = observations(&g);
    let a = obs.iter().find(|o| o.readout.heading == "A").unwrap();
    let b = obs.iter().find(|o| o.readout.heading == "B").unwrap();
    assert_eq!(line(a, "V"), Some("1,001"));
    assert_eq!(line(b, "V"), Some("1,009"));
    let area = |o: &richview_chart::geometry::Observation| o.area.map(|r| r.w * r.h).unwrap();
    let grid: richview_chart::payload::Quant = serde_json::from_value(
        g.derived
            .iter()
            .find(|d| d.key.starts_with("treemap.tiles.hidden."))
            .unwrap()
            .value["grid"]
            .clone(),
    )
    .unwrap();
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    let whole = area(a) + area(b);
    assert!((area(a) / whole - 1001.0 / 2010.0).abs() <= step);
    assert!((area(b) / whole - 1009.0 / 2010.0).abs() <= step);
    assert_eq!(line(a, "Share of the visible"), Some("49.8%"));
    assert_eq!(line(b, "Share of the visible"), Some("50.2%"));
}

#[test]
fn pie_wedges_replay_the_embedded_boundaries_and_keep_dense_targets_after_hide() {
    let spec = json!({"type":"pie","title":"T","encoding":{"x":"name","y":"v"}});
    let rows = vec![
        json!({"name":"A","v":1.23456789}),
        json!({"name":"B","v":2.34567891}),
        json!({"name":"C","v":3.45678912}),
    ];
    let native = draw(&spec, &rows);
    let shown = common::quantized(&spec, &rows);
    let g = replay(&spec, &shown, &native, &[], None);
    for (a, b) in observations(&native).iter().zip(observations(&g)) {
        let a = a.wedge.unwrap();
        let b = b.wedge.unwrap();
        assert_eq!((a.start, a.end), (b.start, b.end));
    }
    let hidden = replay(&spec, &shown, &native, &["A"], None);
    for panel in &hidden.panels {
        for item in &panel.items {
            if let richview_chart::geometry::Item::Area {
                obs: Some(index),
                series: Some(slot),
                ..
            } = item
            {
                let o = panel
                    .observations
                    .get(*index)
                    .expect("a mark points into its panel's dense observations");
                assert_eq!(o.series, Some(*slot));
            }
        }
    }
}

#[test]
fn hidden_pie_totals_sum_shown_rows_and_summarize_only_visible_slices() {
    let spec = json!({"type":"pie","title":"T","encoding":{"x":"name","y":"v"},"style":{"ratio":2}});
    // The visible names are too long to stand beside their slices of this narrow disk (names
    // that fit are drawn there, S27-1017), so the chart takes a key, which also lists the hidden one.
    let rows = vec![
        json!({"name":"Hidden","v":100}),
        json!({"name":"Alpha channel","v":1.234}),
        json!({"name":"Alpha channel","v":1.234}),
        json!({"name":"Beta channel","v":2.5}),
    ];
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["Hidden"], None);
    let obs = observations(&g);
    let a = obs.iter().find(|o| o.readout.heading == "Alpha channel").unwrap();
    assert_eq!(line(a, "V"), Some("2.46"), "sum two shown 1.23 values");
    assert_eq!(line(a, "Share"), Some("49.6%"));
    assert!(g.summary.contains("2 slices"), "{}", g.summary);
    assert!(
        g.summary.contains("the largest is Beta channel at 50.4%"),
        "{}",
        g.summary
    );
    let key_labels: Vec<_> = g
        .figure_items
        .iter()
        .filter_map(|item| match item {
            richview_chart::geometry::Item::Text(t) if t.series == Some(0) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!key_labels.is_empty(), "names this long take the key");
    assert!(
        key_labels.iter().all(|t| *t == "Hidden"),
        "a hidden slice has no share of the visible total"
    );
}

#[test]
fn hidden_violin_groups_keep_the_embedded_value_domain_and_visible_counts() {
    let spec = json!({"type":"violin","title":"T","encoding":{"x":"name","y":"v","color":"group"}});
    let rows: Vec<_> = [("A", 1.0), ("B", 1000.0)]
        .into_iter()
        .flat_map(|(group, unit)| (1..=4).map(move |v| json!({"name":"X","group":group,"v":v as f64*unit})))
        .collect();
    let native = draw(&spec, &rows);
    let g = replay(&spec, &rows, &native, &["B"], None);
    assert_eq!(
        g.fit.y, native.fit.y,
        "hiding a shape does not rescale its value domain"
    );
    assert!(g.summary.contains("1 distribution of 4 observations"), "{}", g.summary);
    assert_eq!(
        g.derived.iter().find(|d| d.key == "violin.forms"),
        native.derived.iter().find(|d| d.key == "violin.forms")
    );
}

#[test]
fn hidden_cartesian_series_refit_the_value_range_without_following_x_zoom() {
    for kind in ["line", "area", "scatter", "bar"] {
        for sign in [-1.0, 1.0] {
            let spec = json!({"type":kind,"title":"T","encoding":{"x":"x","y":"y","color":"g"}});
            let rows: Vec<_> = (0..8)
                .flat_map(|i| {
                    [
                        json!({"x":i,"y":sign * (10 + i) as f64,"g":"Low"}),
                        json!({"x":i,"y":sign * (1000 + i * 37) as f64,"g":"High"}),
                    ]
                })
                .collect();
            let native = draw(&spec, &rows);
            let low: Vec<_> = rows.iter().filter(|r| r["g"] == "Low").cloned().collect();
            let expected = draw(&spec, &low).fit.y;
            assert_ne!(
                native.fit.y, expected,
                "the fixture has a series that stretches the range"
            );
            for zoom in [None, Some([0.0, 2.0])] {
                let hidden = replay(&spec, &rows, &native, &["High"], zoom);
                assert_eq!(hidden.fit.y, expected, "{kind}, sign {sign}, zoom {zoom:?}");
                assert_eq!(
                    hidden.series[0].color, native.series[0].color,
                    "the visible colour slot stays"
                );
                assert_eq!(
                    hidden.series[1].color, native.series[1].color,
                    "the hidden colour slot stays"
                );
                assert!(hidden.series[1].hidden);
                let restored = replay(&spec, &rows, &native, &[], zoom);
                assert_eq!(restored.fit.y, native.fit.y, "restoring the series restores its range");
            }
        }
    }
}

#[test]
fn hidden_stack_layers_leave_both_positive_and_negative_value_extents() {
    for kind in ["area", "bar"] {
        let spec = json!({"type":kind,"title":"T","stack":true,"encoding":{"x":"x","y":"y","color":"g"}});
        let rows = vec![
            json!({"x":0,"y":4,"g":"Low"}),
            json!({"x":0,"y":6,"g":"Second"}),
            json!({"x":0,"y":100,"g":"High"}),
            json!({"x":1,"y":-3,"g":"Low"}),
            json!({"x":1,"y":-7,"g":"Second"}),
            json!({"x":1,"y":-100,"g":"High"}),
            json!({"x":2,"y":2,"g":"Low"}),
            json!({"x":2,"y":5,"g":"Second"}),
            json!({"x":2,"y":200,"g":"High"}),
        ];
        let native = draw(&spec, &rows);
        assert_eq!(
            replay(&spec, &rows, &native, &["High"], None).fit.y,
            Some([-10.0, 10.0]),
            "{kind}: remaining stack totals"
        );
        assert_eq!(
            replay(&spec, &rows, &native, &["High", "Second"], None).fit.y,
            Some([-3.0, 4.0]),
            "{kind}: last layer"
        );
    }
}

#[test]
fn hidden_series_refit_shared_and_free_facet_ranges() {
    let rows: Vec<_> = ["A", "B"]
        .into_iter()
        .enumerate()
        .flat_map(|(facet, name)| {
            (0..8).flat_map(move |i| {
                [
                    json!({"x":i,"y":10 + facet * 90 + i,"g":"Low","f":name}),
                    json!({"x":i,"y":1000 + facet * 1000 + i * 37,"g":"High","f":name}),
                ]
            })
        })
        .collect();
    for free in [false, true] {
        let mut spec = json!({"type":"line","title":"T","encoding":{"x":"x","y":"y","color":"g","facet":"f"}});
        if free {
            spec["style"] = json!({"facet_scale":"free"});
        }
        let native = draw(&spec, &rows);
        let hidden = replay(&spec, &rows, &native, &["High"], None);
        let low: Vec<_> = rows.iter().filter(|r| r["g"] == "Low").cloned().collect();
        assert_eq!(
            hidden.fit.y,
            draw(&spec, &low).fit.y,
            "free scales {free}: visible shared extent"
        );
        assert_eq!(hidden.panels.len(), 2);
        if free {
            for panel in &hidden.panels {
                let ys: Vec<_> = panel
                    .observations
                    .iter()
                    .flat_map(|o| o.dots.iter().filter(|d| d.series == 0).map(|d| d.at[1]))
                    .collect();
                let lo = ys.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                assert!(
                    (hi - lo) / panel.plot.h > 0.25,
                    "each free panel uses its visible range"
                );
            }
        }
    }
}

#[test]
fn hidden_series_still_respect_authored_value_axis_bounds() {
    let spec =
        json!({"type":"line","title":"T","encoding":{"x":"x","y":"y","color":"g"},"axes":{"y":{"min":0,"max":2000}}});
    let rows = vec![
        json!({"x":0,"y":10,"g":"Low"}),
        json!({"x":1,"y":17,"g":"Low"}),
        json!({"x":0,"y":1000,"g":"High"}),
        json!({"x":1,"y":1259,"g":"High"}),
    ];
    let whole = draw(&spec, &rows);
    let native = replay(&spec, &rows, &whole, &[], None);
    let hidden = replay(&spec, &rows, &native, &["High"], None);
    let ticks = |g: &Geometry| {
        g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                richview_chart::geometry::Item::Text(t)
                    if t.role == richview_chart::geometry::Role::AxisLabel && t.x < g.panels[0].plot.x =>
                {
                    Some(t.text.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(ticks(&hidden), ticks(&native), "authored bounds retain the value scale");
}
