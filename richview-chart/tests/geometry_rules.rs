//! Chart rules measured on the geometry model: facets, annotations, missing values, series
//! identity, zoom and motion (FEA-CH-09, FEA-CH-35, FEA-CH-38, FEA-CH-49, FEA-CH-63, STY-40).

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Item, Role};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};

fn draw(spec: Value, rows: &[Value], opts: Options) -> Geometry {
    let block = ChartBlock::from_value(&spec).expect("block");
    layout(
        &block,
        rows,
        &ChartTheme::default(),
        &Options {
            width: if opts.width > 0.0 { opts.width } else { 640.0 },
            ..opts
        },
    )
}

fn months(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("2024-{:02}", i + 1)).collect()
}

/// The names a chart draws for its series: the label texts that spell a series' name.
fn direct_names(g: &Geometry) -> Vec<&richview_chart::geometry::Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label && t.series.is_some_and(|s| g.series[s].name == t.text) => Some(t),
            _ => None,
        })
        .collect()
}

/// The box a name's button covers: at least 24 px each way, centred on the name.
fn name_target(t: &richview_chart::geometry::Text) -> richview_chart::geometry::Rect {
    let (w, h) = ((t.bounds.w + 4.0).max(24.0), (t.bounds.h + 4.0).max(24.0));
    let c = t.bounds.center();
    richview_chart::geometry::Rect::new(c[0] - w / 2.0, c[1] - h / 2.0, w, h)
}

/// An area's names stand inside the plot and never in the value axis' margin, where they read
/// as stray axis labels; no name column takes width on either side (FEA-CH-27).
#[test]
fn area_names_stand_inside_the_plot_and_leave_the_margins_to_the_axis() {
    let rows: Vec<_> = (0..4)
        .flat_map(|x| {
            [
                json!({"x":x,"v":820+x*40,"s":"free"}),
                json!({"x":x,"v":240+x*20,"s":"paid"}),
            ]
        })
        .collect();
    let spec = json!({"type":"area","title":"T","encoding":{"x":"x","y":"v","color":"s"}});
    for width in [358.0, 640.0] {
        let all = draw(
            spec.clone(),
            &rows,
            Options {
                width,
                ..Options::default()
            },
        );
        assert!(all.legend.is_none(), "{width}: two separate areas are named inside");
        for hidden in [vec![], vec!["free".to_owned()], vec!["paid".to_owned()]] {
            let g = draw(
                spec.clone(),
                &rows,
                Options {
                    width,
                    hidden: hidden.clone(),
                    ..Options::default()
                },
            );
            assert!(g.legend.is_none(), "{width}: hiding {hidden:?} keeps the names inside");
            let panel = &g.panels[0];
            assert!(
                (panel.frame.right() - panel.plot.right() - 9.0).abs() <= 1e-6,
                "area keeps only the endpoint halo inset, not a name gutter, at {width}"
            );
            let names = direct_names(&g);
            assert_eq!(names.len(), 2, "complete names include the hidden series");
            let targets: Vec<_> = names
                .iter()
                .map(|t| {
                    assert!(
                        t.bounds.inside(&panel.plot, 0.0),
                        "{} stands inside the plot at {width}, hidden {hidden:?}: {:?}",
                        t.text,
                        t.bounds
                    );
                    name_target(t)
                })
                .collect();
            for (i, target) in targets.iter().enumerate() {
                assert!(target.inside(&panel.frame, 0.0), "whole name target fits");
                assert!(targets.iter().skip(i + 1).all(|other| !target.overlaps(other, 0.0)));
            }
        }
    }
}

/// Lines and areas are named the same way: as far right as a free place exists, so a name
/// stands where its own series ends, not where it began.
#[test]
fn line_and_area_names_stand_by_the_right_ends_of_their_series() {
    let rows = vec![
        json!({"x":0,"v":90,"s":"A"}),
        json!({"x":0,"v":10,"s":"B"}),
        json!({"x":1,"v":10,"s":"A"}),
        json!({"x":1,"v":90,"s":"B"}),
    ];
    for kind in ["area", "line"] {
        let g = draw(
            json!({"type":kind,"title":"T","encoding":{"x":"x","y":"v","color":"s"}}),
            &rows,
            Options {
                width: 800.0,
                ..Options::default()
            },
        );
        assert!(g.legend.is_none(), "the wide fixture has room for direct names");
        let plot = g.panels[0].plot;
        let at = |name: &str| {
            direct_names(&g)
                .into_iter()
                .find(|t| t.text == name)
                .map(|t| t.bounds.center())
                .unwrap()
        };
        for name in ["A", "B"] {
            assert!(
                at(name)[0] > plot.x + plot.w / 2.0,
                "{kind}: {name} stands in the right half"
            );
        }
        assert!(at("A")[1] > at("B")[1], "{kind}: A ends low and B ends high");
    }
}

/// Hiding an area lets the value axis follow the rest; the names stay inside the plot, and
/// showing the series again returns every name and mark to its place (FEA-CH-27, FEA-CH-33).
#[test]
fn hiding_an_area_keeps_the_names_inside_and_restoring_returns_them() {
    let rows: Vec<_> = ["Mon", "Tue", "Wed", "Thu"]
        .iter()
        .enumerate()
        .flat_map(|(i, x)| {
            [
                json!({"x":x,"v":([820,910,870,940][i]),"s":"free"}),
                json!({"x":x,"v":([240,265,290,305][i]),"s":"paid"}),
            ]
        })
        .collect();
    let spec = json!({"type":"area","title":"T","encoding":{"x":"x","y":"v","color":"s"}});
    let all = draw(
        spec.clone(),
        &rows,
        Options {
            width: 576.0,
            ..Options::default()
        },
    );
    assert!(all.legend.is_none(), "two separate areas are named inside");
    assert_eq!(direct_names(&all).len(), 2);
    let hidden = draw(
        spec.clone(),
        &rows,
        Options {
            width: 576.0,
            hidden: vec!["free".into()],
            derived: all.derived.clone(),
            ..Options::default()
        },
    );
    assert!(hidden.legend.is_none(), "hiding a series brings no legend");
    assert!(hidden.series[0].hidden);
    let names = direct_names(&hidden);
    assert_eq!(names.len(), 2, "the hidden series keeps its name as the way back");
    assert!(names.iter().all(|t| t.bounds.inside(&hidden.panels[0].plot, 0.0)));
    let restored = draw(
        spec,
        &rows,
        Options {
            width: 576.0,
            derived: hidden.derived,
            ..Options::default()
        },
    );
    assert!(restored.legend.is_none());
    assert_eq!(
        restored.panels, all.panels,
        "restoring also restores original name positions and data"
    );
    assert_eq!(
        restored.series, all.series,
        "the same series controls remain restorable"
    );
}

/// A series with no rows inside the zoom window has no line to be named beside. The chart then
/// names its series in the legend, so every series stays controllable while zoomed (FEA-CH-27).
#[test]
fn zoom_keeps_every_area_controllable_without_rows_inside_the_window() {
    let mut rows: Vec<_> = (0..5).map(|x| json!({"x":x,"v":10+x,"s":"A"})).collect();
    rows.extend([json!({"x":3,"v":100,"s":"LateB"}), json!({"x":4,"v":110,"s":"LateB"})]);
    let spec = json!({"type":"area","title":"T","encoding":{"x":"x","y":"v","color":"s"}});
    for hidden in [vec![], vec!["LateB".to_owned()]] {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                width: 800.0,
                zoom: Some([1.0, 2.0]),
                hidden,
                ..Options::default()
            },
        );
        let names: Vec<_> = direct_names(&g).into_iter().map(|t| t.text.as_str()).collect();
        match &g.legend {
            Some(legend) => {
                assert_eq!(legend.entries.len(), 2, "the legend holds every series");
                assert!(names.is_empty(), "a legend does not duplicate direct names");
            }
            None => assert!(
                names.contains(&"A") && names.contains(&"LateB"),
                "every series remains controllable while zoomed: {names:?}"
            ),
        }
    }
}

/// Each part of a stack is named inside its own band, between the parts below it and its own
/// top line, also after a part below it is hidden (FEA-CH-27).
#[test]
fn stacked_area_names_stand_inside_their_own_bands() {
    let rows: Vec<_> = (0..6)
        .flat_map(|x| [json!({"x":x,"v":10+x,"s":"A"}), json!({"x":x,"v":12,"s":"B"})])
        .collect();
    let spec = json!({"type":"area","title":"T","stack":true,"encoding":{"x":"x","y":"v","color":"s"},"axes":{"y":{"min":0,"max":30}}});
    for hidden in [vec![], vec!["A".to_owned()]] {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                width: 800.0,
                hidden: hidden.clone(),
                ..Options::default()
            },
        );
        assert!(g.legend.is_none(), "two wide bands have room for their names");
        let panel = &g.panels[0];
        let axis_y = |label: &str| {
            panel
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text(t) if t.role == Role::AxisLabel && t.text == label => Some(t.bounds.center()[1]),
                    _ => None,
                })
                .unwrap()
        };
        let y_of = |v: f64| axis_y("0") + (axis_y("30") - axis_y("0")) * v / 30.0;
        for t in direct_names(&g) {
            if hidden.contains(&t.text) {
                continue;
            }
            let c = t.bounds.center();
            // The area runs from edge to edge of its plot over x = 0..5.
            let x = (c[0] - panel.plot.x) / panel.plot.w * 5.0;
            let a = if hidden.is_empty() { 10.0 + x } else { 0.0 };
            let (floor, top) = if t.text == "A" { (0.0, a) } else { (a, a + 12.0) };
            assert!(
                c[1] > y_of(top) && c[1] < y_of(floor),
                "{} stands inside its band ({floor}..{top}) with {hidden:?} hidden: y {} of {}..{}",
                t.text,
                c[1],
                y_of(top),
                y_of(floor)
            );
        }
    }
}

#[test]
fn series_names_fall_back_when_wrapped_axes_leave_too_little_room() {
    let rows: Vec<_> = (0..5)
        .flat_map(|x| (0..7).map(move |s| json!({"x":x,"v":100+s+x*2,"s":format!("S{}",s+1)})))
        .collect();
    for kind in ["line", "area"] {
        let spec = json!({"type":kind,"title":"T","encoding":{"x":"x","y":"v","color":"s"},
            "axes":{"x":{"label":"Week since commissioning of each individual production line"},"y":{"label":"Output"}}});
        let all = draw(
            spec.clone(),
            &rows,
            Options {
                width: 358.0,
                ..Options::default()
            },
        );
        assert!(
            all.legend.is_some(),
            "{kind}: complete buttons do not fit the real phone plot after axis wrapping"
        );
        assert_eq!(all.legend.as_ref().unwrap().entries.len(), 7);
        for hidden in [vec!["S1".to_owned()], (1..7).map(|s| format!("S{s}")).collect()] {
            let g = draw(
                spec.clone(),
                &rows,
                Options {
                    width: 358.0,
                    hidden,
                    ..Options::default()
                },
            );
            assert_eq!(
                g.legend.as_ref().unwrap().entries.len(),
                7,
                "all restore controls stay in the original legend"
            );
            assert!(g.panels.iter().all(|p| {
                p.items
                    .iter()
                    .all(|i| !matches!(i, Item::Text(t) if t.role==Role::Label && t.series.is_some() && t.face != richview_chart::geometry::Face::Figures))
            }));
        }
        assert_eq!(
            draw(
                spec.clone(),
                &rows,
                Options {
                    width: 358.0,
                    ..Options::default()
                }
            ),
            all,
            "restoration is exact"
        );
        if kind == "line" {
            let wide = draw(
                spec,
                &rows,
                Options {
                    width: 800.0,
                    ..Options::default()
                },
            );
            assert!(wide.legend.is_none(), "the same complete names still fit on desktop");
            let names: Vec<_> = wide.panels[0]
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t)
                        if t.role == Role::Label
                            && t.series.is_some()
                            && t.face != richview_chart::geometry::Face::Figures =>
                    {
                        Some(t)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(names.len(), 7);
            let targets: Vec<_> = names
                .iter()
                .map(|t| {
                    assert!(
                        t.bounds.x > wide.panels[0].plot.right(),
                        "line names remain on the right"
                    );
                    let [x, y] = t.bounds.center();
                    let (w, h) = ((t.bounds.w + 4.0).max(24.0), (t.bounds.h + 4.0).max(24.0));
                    richview_chart::geometry::Rect::new(x - w / 2.0, y - h / 2.0, w, h)
                })
                .collect();
            for (i, target) in targets.iter().enumerate() {
                assert!(target.inside(&wide.panels[0].frame, 0.0));
                assert!(
                    targets.iter().skip(i + 1).all(|other| !target.overlaps(other, 0.0)),
                    "whole targets stay apart"
                );
            }
        }
    }
}

fn kinds(g: &Geometry) -> Vec<&str> {
    g.checks.iter().map(|c| c.kind.as_str()).collect()
}

#[test]
fn facets_share_slots_and_take_two_columns_from_600_px() {
    let rows: Vec<Value> = ["East", "West"]
        .iter()
        .flat_map(|r| {
            months(4)
                .into_iter()
                .flat_map(move |m| ["A", "B"].map(|s| json!({"m": m, "v": 3, "s": s, "region": r})))
        })
        .collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s", "facet": "region"}});
    let wide = draw(
        spec.clone(),
        &rows,
        Options {
            width: 720.0,
            ..Options::default()
        },
    );
    assert_eq!(wide.panels.len(), 2);
    assert_eq!(
        wide.panels[0].frame.y, wide.panels[1].frame.y,
        "two columns side by side"
    );
    assert_eq!(
        wide.panels.iter().map(|p| p.facet.clone().unwrap()).collect::<Vec<_>>(),
        ["East", "West"]
    );
    let narrow = draw(
        spec,
        &rows,
        Options {
            width: 390.0,
            ..Options::default()
        },
    );
    assert!(
        narrow.panels[1].frame.y > narrow.panels[0].frame.bottom() - 1.0,
        "one column below 600 px"
    );
    assert!(narrow.height > wide.height, "each extra facet row grows the chart");
    // One series identity across panels.
    assert_eq!(wide.series.len(), 2);
}

#[test]
fn hidden_series_keep_their_slot_and_entity_colours_pin_slots() {
    let rows: Vec<Value> = months(3)
        .into_iter()
        .flat_map(|m| ["A", "B", "C"].map(|s| json!({"m": m, "v": 1, "s": s})))
        .collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}});
    let all = draw(spec.clone(), &rows, Options::default());
    let hidden = draw(
        spec.clone(),
        &rows,
        Options {
            hidden: vec!["A".into()],
            ..Options::default()
        },
    );
    assert_eq!(all.series[2].slot, hidden.series[2].slot);
    assert!(hidden.series[0].hidden);
    assert!(
        hidden.panels[0]
            .items
            .iter()
            .all(|i| i.series() != Some(0) || i.role() != Role::Mark)
    );
    let pinned = draw(
        spec,
        &rows,
        Options {
            colors: vec![("C".into(), 1)],
            ..Options::default()
        },
    );
    assert_eq!(pinned.series[2].slot, 0, "C takes slot 1");
    assert_eq!(pinned.series[0].slot, 1, "A moves to the next free slot");
}

#[test]
fn missing_values_break_lines_and_never_become_observations() {
    let rows: Vec<Value> = months(6)
        .into_iter()
        .enumerate()
        .map(|(i, m)| json!({"m": m, "v": if i == 3 { Value::Null } else { json!(i + 1) }}))
        .collect();
    let g = draw(
        json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"}}),
        &rows,
        Options::default(),
    );
    let pieces: usize = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => Some(path.flatten(0.5).len()),
            _ => None,
        })
        .sum();
    assert_eq!(pieces, 2, "the line breaks at the missing month");
    let readouts: Vec<String> = g.panels[0]
        .observations
        .iter()
        .flat_map(|o| o.readout.lines.iter().map(|l| l.value.clone()))
        .collect();
    assert!(
        readouts.iter().all(|v| v != "0"),
        "missing never reads as zero: {readouts:?}"
    );
    assert!(readouts.iter().any(|v| v == "No observation"), "{readouts:?}");
}

/// Each labelled band of a chart with its label's box, in the order the bands stand on x.
fn band_labels(
    g: &Geometry,
    texts: &[&str],
) -> Vec<(richview_chart::geometry::Rect, Option<richview_chart::geometry::Rect>)> {
    let mut bands: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect {
                rect,
                role: Role::Annotation,
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    bands.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(bands.len(), texts.len(), "one band per label");
    bands
        .into_iter()
        .zip(texts)
        .map(|(band, text)| {
            let label = g.panels[0].items.iter().find_map(|i| match i {
                Item::Text(t) if t.role == Role::Annotation && t.text == *text => Some(t.bounds),
                _ => None,
            });
            (band, label)
        })
        .collect()
}

fn banded(bands: &[(&str, &str, &str)], width: f64) -> Geometry {
    // Low values under a y axis to 100, so the top rows are clear of the line.
    banded_with(bands, width, |i| Some(i % 7))
}

/// A line over 42 months under a y axis to 100, with the given bands and a value for each month
/// that has a row.
fn banded_with(bands: &[(&str, &str, &str)], width: f64, value: impl Fn(usize) -> Option<usize>) -> Geometry {
    let rows: Vec<Value> = (0..42)
        .filter_map(|i| Some(json!({"m": format!("{}-{:02}", 2023 + i / 12, i % 12 + 1), "v": value(i)?})))
        .collect();
    let annotations: Vec<Value> = bands
        .iter()
        .map(|(from, to, text)| json!({"kind": "band", "from": {"x": from}, "to": {"x": to}, "text": text}))
        .collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"},
        "axes": {"y": {"max": 100}}, "annotations": annotations});
    draw(
        spec,
        &rows,
        Options {
            width,
            ..Options::default()
        },
    )
}

/// A band's label reads as that band's: it lies over its band, keeps 12 px from its neighbours
/// in a row and uses at most two rows (chart design, 15 Nov 2026). Here every label has a place
/// ending at its band, the last one only there, so all three end 4 px inside their bands.
#[test]
fn a_bands_label_lies_over_its_band_and_keeps_its_distance() {
    let texts = [
        "First long event label",
        "Second long event label",
        "Third event, at the right edge",
    ];
    let g = banded(
        &[
            ("2023-11", "2023-12", texts[0]),
            ("2024-05", "2024-06", texts[1]),
            ("2026-04", "2026-05", texts[2]),
        ],
        1100.0,
    );
    let placed = band_labels(&g, &texts);
    let boxes: Vec<_> = placed
        .iter()
        .map(|(_, l)| l.unwrap_or_else(|| panic!("every label is placed: {:?}", g.dropped)))
        .collect();
    let apart = placed[1].0.x - placed[0].0.x;
    assert!(
        (100.0..boxes[0].w + 12.0).contains(&apart),
        "the first two bands are too near for one row: {apart} px, label {} px",
        boxes[0].w
    );
    for ((band, _), label) in placed.iter().zip(&boxes) {
        assert!(
            label.x < band.right() && band.x < label.right(),
            "a label lies over its band: {label:?} and {band:?}"
        );
    }
    let plot = g.panels[0].plot;
    for ((band, _), label) in placed.iter().zip(&boxes) {
        assert!(
            (band.right() - label.right() - 4.0).abs() < 1.0 && label.right() <= plot.right(),
            "every label ends 4 px inside its band: {label:?} and {band:?}"
        );
    }
    let mut rows: Vec<f64> = boxes.iter().map(|b| b.y).collect();
    rows.sort_by(f64::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(rows.len(), 2, "the second label takes the second row: {boxes:?}");
    assert!(boxes[1].y > boxes[0].y);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            if (a.y - b.y).abs() < 0.01 {
                let gap = (b.x - a.right()).max(a.x - b.right());
                assert!(gap >= 12.0 - 0.01, "row neighbours keep 12 px: {gap}");
            }
        }
    }
    assert!(g.dropped.iter().all(|d| d.kind.as_deref() != Some("annotation")));
}

/// Two rows are all there are: a third label that fits in neither is dropped and reported, and
/// the two that stay still lie over their bands. On this narrow chart a label ending at one of
/// the first two bands would leave the plot, so starting prints more and both labels start
/// 4 px inside their bands.
#[test]
fn a_band_label_with_no_row_left_is_dropped_and_reported() {
    let texts = [
        "First long event label",
        "Second long event label",
        "Third long event label",
    ];
    let g = banded(
        &[
            ("2024-05", "2024-06", texts[0]),
            ("2024-07", "2024-08", texts[1]),
            ("2024-09", "2024-10", texts[2]),
        ],
        390.0,
    );
    let placed = band_labels(&g, &texts);
    assert!(placed[0].1.is_some() && placed[1].1.is_some(), "{placed:?}");
    assert!(placed[2].1.is_none(), "no third row: {placed:?}");
    for (band, label) in placed.iter().filter_map(|(b, l)| l.map(|l| (b, l))) {
        assert!(label.x < band.right() && band.x < label.right());
        assert!(
            (label.x - band.x - 4.0).abs() < 0.01,
            "starts 4 px inside: {label:?} and {band:?}"
        );
    }
    assert!(
        g.dropped
            .iter()
            .any(|d| d.kind.as_deref() == Some("annotation") && d.text == texts[2]),
        "{:?}",
        g.dropped
    );
    assert!(kinds(&g).contains(&"label-dropped"), "{:?}", kinds(&g));
}

/// All band labels of a chart hang on the same side of their bands, so no label reaches from
/// its own band to the band the next label ends at (chart design, 16 Nov 2026). Three bands
/// 39 px apart print two labels on either side; on that tie the labels end at their bands.
#[test]
fn band_labels_of_one_chart_hang_on_the_same_side() {
    let texts = [
        "First long event label",
        "Second long event label",
        "Third long event label",
    ];
    let g = banded(
        &[
            ("2024-05", "2024-06", texts[0]),
            ("2024-08", "2024-09", texts[1]),
            ("2024-11", "2024-12", texts[2]),
        ],
        560.0,
    );
    let placed = band_labels(&g, &texts);
    let printed: Vec<_> = placed.iter().filter_map(|(b, l)| l.map(|l| (*b, l))).collect();
    assert_eq!(printed.len(), 2, "two rows hold two of the three: {placed:?}");
    for (band, label) in &printed {
        assert!(
            (band.right() - label.right() - 4.0).abs() < 1.0 && label.x < band.x,
            "on a tie every label ends 4 px inside its band: {label:?} and {band:?}"
        );
    }
    assert!(
        (printed[0].1.y - printed[1].1.y).abs() > 1.0,
        "one label a row: {printed:?}"
    );
    assert_eq!(
        g.dropped
            .iter()
            .filter(|d| d.kind.as_deref() == Some("annotation"))
            .count(),
        1,
        "{:?}",
        g.dropped
    );
}

/// A drawn line is a mark between its points too. Three rows leave a long stretch of line with
/// no point on it; where that stretch runs through the first row, the label takes the second.
#[test]
fn a_band_label_keeps_clear_of_the_line_between_the_points() {
    let texts = ["An event label"];
    let band = [("2024-05", "2024-06", texts[0])];
    let sparse = |high: usize| {
        banded_with(&band, 560.0, move |i| match i {
            0 => Some(0),
            1 | 41 => Some(high),
            _ => None,
        })
    };
    let clear = band_labels(&sparse(3), &texts)[0].1.expect("placed");
    let g = sparse(97);
    let label = band_labels(&g, &texts)[0].1.expect("placed in the second row");
    assert!(label.y > clear.y + 1.0, "second row: {label:?}, first row {clear:?}");
    let line_y = g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                path, role: Role::Mark, ..
            } => path.bounds().map(|b| b.y),
            _ => None,
        })
        .expect("the series line");
    assert!(
        (clear.y..clear.bottom()).contains(&line_y),
        "the line runs through the first row: {line_y} and {clear:?}"
    );
    assert!(
        line_y < label.y - 2.0,
        "the label is off the stroke: {line_y} and {label:?}"
    );
}

#[test]
fn annotations_draw_and_unmatched_ones_warn() {
    let rows: Vec<Value> = months(6)
        .into_iter()
        .enumerate()
        .map(|(i, m)| json!({"m": m, "v": i * 10}))
        .collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"}, "annotations": [
        {"kind": "line", "at": {"y": 25}, "text": "Target"},
        {"kind": "point", "at": {"x": "2024-04", "y": 30}, "text": "Launch"},
        {"kind": "band", "from": {"x": "2024-02"}, "to": {"x": "2024-03"}},
        {"kind": "point", "at": {"x": "2031-01", "y": 5}, "text": "Nowhere"}
    ]});
    let g = draw(spec, &rows, Options::default());
    let annotations = g.panels[0]
        .items
        .iter()
        .filter(|i| i.role() == Role::Annotation)
        .count();
    assert!(annotations >= 4, "rule, point, band and texts: {annotations}");
    assert_eq!(kinds(&g).iter().filter(|k| **k == "unresolved-annotation").count(), 1);
    let texts: Vec<String> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Text(t) = i {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(
        texts.contains(&"Target".to_string()) && texts.contains(&"Launch".to_string()),
        "{texts:?}"
    );
}

#[test]
fn zoom_narrows_the_visible_window() {
    let rows: Vec<Value> = (0..20).map(|i| json!({"x": i, "v": i})).collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "v"}});
    let all = draw(spec.clone(), &rows, Options::default());
    let zoomed = draw(
        spec,
        &rows,
        Options {
            zoom: Some([5.0, 10.0]),
            ..Options::default()
        },
    );
    let domain = |g: &Geometry| g.panels[0].x.as_ref().expect("x scale").domain;
    assert!(domain(&zoomed)[1] - domain(&zoomed)[0] < domain(&all)[1] - domain(&all)[0]);
    assert_eq!(domain(&all), all.panels[0].x.as_ref().unwrap().extent);
}

#[test]
fn a_zoomed_line_runs_to_the_nearest_point_beyond_each_edge() {
    // The line continues to the plot edge through the nearest x on each side of the window and
    // no farther (FEA-CH-49); a window that holds no point keeps only the x after it.
    let rows: Vec<Value> = (0..20).map(|i| json!({"x": i, "v": i})).collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "x", "y": "v"}});
    let points = |zoom: [f64; 2]| -> Vec<usize> {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                zoom: Some(zoom),
                ..Options::default()
            },
        );
        g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line {
                    path, role: Role::Mark, ..
                } => Some(path.cmds.len()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(points([5.5, 9.5]), [6], "x = 5 to 10: four inside and one on each side");
    assert!(
        points([5.2, 5.8]).iter().all(|&n| n <= 1),
        "an empty window draws no segment"
    );
}

#[test]
fn a_style_without_motion_has_no_entrance() {
    let rows = vec![json!({"c": "A", "v": 1})];
    let spec = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}});
    let block = ChartBlock::from_value(&spec).unwrap();
    let still = layout(
        &block,
        &rows,
        &ChartTheme {
            motion: false,
            ..ChartTheme::default()
        },
        &Options {
            width: 640.0,
            ..Options::default()
        },
    );
    assert!(still.entrance.is_none());
    assert!(draw(spec, &rows, Options::default()).entrance.is_some());
}

#[test]
fn every_chart_names_itself_for_screen_readers() {
    let rows: Vec<Value> = months(3)
        .into_iter()
        .enumerate()
        .map(|(i, m)| json!({"m": m, "v": i + 1}))
        .collect();
    let g = draw(
        json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"}}),
        &rows,
        Options::default(),
    );
    assert!(!g.summary.is_empty() && g.summary.ends_with('.'), "{}", g.summary);
}

fn readouts(g: &Geometry, panel: usize) -> Vec<(String, String)> {
    g.panels[panel]
        .observations
        .iter()
        .flat_map(|o| o.readout.lines.iter().map(|l| (l.name.clone(), l.value.clone())))
        .collect()
}

#[test]
fn each_facet_panel_draws_from_its_own_rows() {
    // Box plots: panel Two's values must not pull panel One's median (FEA-CH-35).
    let rows: Vec<Value> = [("One", [1, 2, 3]), ("Two", [100, 101, 102])]
        .iter()
        .flat_map(|(f, vs)| vs.map(|v| json!({"c": "a", "v": v, "f": f})))
        .collect();
    let g = draw(
        json!({"type": "boxplot", "title": "T", "encoding": {"x": "c", "y": "v", "facet": "f"}}),
        &rows,
        Options::default(),
    );
    assert_eq!(g.panels.len(), 2);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    let median = |p: usize| readouts(&g, p).into_iter().find(|(n, _)| n == "Median").map(|(_, v)| v);
    assert_eq!(median(0).as_deref(), Some("2"));
    assert_eq!(median(1).as_deref(), Some("101"));
    // Intervals: the same category in two panels is two rows, not a repeat.
    let rows = [
        json!({"c": "a", "lo": 1, "hi": 2, "f": "One"}),
        json!({"c": "a", "lo": 3, "hi": 4, "f": "Two"}),
    ];
    let g = draw(
        json!({"type": "interval", "title": "T", "encoding": {"x": "c", "lower": "lo", "upper": "hi", "facet": "f"}}),
        &rows,
        Options::default(),
    );
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(
        g.panels
            .iter()
            .all(|p| p.failure.is_none() && !p.observations.is_empty())
    );
}

#[test]
fn a_bad_facet_panel_fails_alone() {
    let cases: Vec<(Value, Vec<Value>)> = vec![
        (
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "facet": "f"}}),
            vec![
                json!({"c": "a", "v": 1, "f": "One"}),
                json!({"c": "a", "v": 2, "f": "Two"}),
                json!({"c": "a", "v": 3, "f": "Two"}),
            ],
        ),
        (
            json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "facet": "f"}}),
            vec![
                json!({"m": "2024-01", "v": 1, "f": "One"}),
                json!({"m": "2024-02", "v": 2, "f": "One"}),
                json!({"m": "2024-01", "v": 1, "f": "Two"}),
                json!({"m": "2024-01", "v": 2, "f": "Two"}),
            ],
        ),
        (
            json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "v", "facet": "f"}}),
            vec![
                json!({"x": 1, "v": 1, "f": "One"}),
                json!({"x": 2, "v": 2, "f": "One"}),
                json!({"x": 1, "v": "abc", "f": "Two"}),
            ],
        ),
        (
            json!({"type": "histogram", "title": "T", "encoding": {"x": "v", "facet": "f"}}),
            vec![
                json!({"v": 1, "f": "One"}),
                json!({"v": 2, "f": "One"}),
                json!({"v": "abc", "f": "Two"}),
            ],
        ),
        (
            json!({"type": "boxplot", "title": "T", "encoding": {"y": "v", "facet": "f"}}),
            vec![
                json!({"v": 1, "f": "One"}),
                json!({"v": 2, "f": "One"}),
                json!({"v": "abc", "f": "Two"}),
            ],
        ),
        (
            json!({"type": "interval", "title": "T", "encoding": {"x": "c", "lower": "lo", "upper": "hi", "facet": "f"}}),
            vec![
                json!({"c": "a", "lo": 1, "hi": 2, "f": "One"}),
                json!({"c": "a", "lo": 5, "hi": 4, "f": "Two"}),
            ],
        ),
    ];
    for (spec, rows) in cases {
        let kind = spec["type"].clone();
        let g = draw(spec, &rows, Options::default());
        assert!(g.failure.is_none(), "{kind}: the chart still draws");
        assert_eq!(g.panels.len(), 2, "{kind}");
        assert!(g.panels[0].failure.is_none(), "{kind}: the valid panel draws");
        assert!(
            g.panels[0].items.iter().any(|i| i.role() == Role::Mark),
            "{kind}: the valid panel has marks"
        );
        assert!(g.panels[1].failure.is_some(), "{kind}: the bad panel fails");
        assert_eq!(g.panels[1].facet.as_deref(), Some("Two"), "{kind}: under its own name");
        let named = g.panels[1].items.iter().any(|i| match i {
            Item::Text(t) => t.role == Role::PanelTitle && t.text == "Two",
            _ => false,
        });
        assert!(named, "{kind}: the failed panel keeps its title");
        assert!(
            g.checks.iter().any(|c| c.level == richview_chart::Level::Error),
            "{kind}"
        );
    }
}

#[test]
fn charts_never_disappear_or_drop_rows_silently() {
    let bar = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "facet": "f"}});
    // No rows, and a facet field no row has: a failed frame, never a zero-height chart.
    for rows in [vec![], vec![json!({"c": "a", "v": 1})]] {
        let g = draw(bar.clone(), &rows, Options::default());
        assert!(g.height > 100.0, "{}", g.height);
        assert_eq!(g.panels.len(), 1);
        assert!(g.failure.as_deref().is_some_and(|f| f.starts_with("Nothing to draw")));
        assert!(kinds(&g).contains(&"empty-data"));
    }
    // A row without a facet value is disclosed.
    let rows = [json!({"c": "a", "v": 1, "f": "One"}), json!({"c": "b", "v": 2})];
    let g = draw(bar, &rows, Options::default());
    let excluded = g
        .checks
        .iter()
        .find(|c| c.kind == "excluded-observations")
        .expect("disclosed");
    assert_eq!(excluded.details["count"], 1);
    // A pie whose total overflows names no share.
    let pie = json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}});
    let g = draw(
        pie,
        &[json!({"c": "a", "v": 1.7e308}), json!({"c": "a", "v": 1.7e308})],
        Options::default(),
    );
    assert!(kinds(&g).contains(&"value-overflow"));
    assert!(!g.summary.contains("NaN"), "{}", g.summary);
    let svg = richview_chart::vector::svg(&g, &ChartTheme::default());
    assert!(!svg.contains("NaN"));
    // Stack totals follow the same rule (FEA-CH-70).
    let stack = json!({"type": "bar", "title": "T", "stack": true, "encoding": {"x": "c", "y": "v", "color": "s"}});
    let g = draw(
        stack,
        &[
            json!({"c": "a", "v": 1.7e308, "s": "x"}),
            json!({"c": "a", "v": 1.7e308, "s": "y"}),
        ],
        Options::default(),
    );
    assert!(kinds(&g).contains(&"value-overflow"));
    assert!(g.failure.is_some());
}

#[test]
fn many_labels_are_checked_in_bounded_time_with_one_finding() {
    let rows: Vec<Value> = (0..5000)
        .map(|i| json!({"c": format!("Category {i}"), "v": i % 97}))
        .collect();
    let started = std::time::Instant::now();
    let g = draw(
        json!({"type": "bar", "title": "T", "style": {"orientation": "vertical"}, "encoding": {"x": "c", "y": "v"}}),
        &rows,
        Options::default(),
    );
    assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
    for kind in ["label-collision", "clipping", "label-dropped"] {
        assert!(kinds(&g).iter().filter(|k| **k == kind).count() <= 8, "{kind}");
    }
}

#[test]
fn summaries_read_cleanly_when_nothing_is_visible() {
    let rows: Vec<Value> = months(3)
        .into_iter()
        .flat_map(|m| ["A", "B"].map(|s| json!({"m": m, "v": 1, "s": s})))
        .collect();
    let g = draw(
        json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}}),
        &rows,
        Options {
            hidden: vec!["A".into(), "B".into()],
            ..Options::default()
        },
    );
    assert_eq!(g.summary, "No values to show.");
}

#[test]
fn summaries_describe_only_the_panels_that_draw() {
    let bar = json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v", "facet": "f"}});
    let rows = [
        json!({"c": "a", "v": 1, "f": "One"}),
        json!({"c": "a", "v": 1, "f": "Two"}),
        json!({"c": "a", "v": 3, "f": "Two"}),
        json!({"c": "b", "v": 9}),
    ];
    let g = draw(bar, &rows, Options::default());
    assert!(
        !g.summary.contains('3') || g.summary.contains("panels"),
        "{}",
        g.summary
    );
    assert!(
        !g.summary.contains("at 9") && !g.summary.contains(" b "),
        "{}",
        g.summary
    );
    assert!(g.summary.starts_with("1 category;"), "{}", g.summary);
    assert!(
        g.summary.ends_with("1 of 2 panels could not be drawn."),
        "{}",
        g.summary
    );
    let failed = g.checks.iter().find(|c| c.kind == "ambiguous-bar-slot").unwrap();
    assert!(failed.details.to_string().contains("Two"));
    let pie = json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v", "facet": "f"}});
    let rows = [
        json!({"c": "a", "v": 1.7e308, "f": "One"}),
        json!({"c": "a", "v": 1.7e308, "f": "One"}),
        json!({"c": "a", "v": 2, "f": "Two"}),
        json!({"c": "b", "v": 6, "f": "Two"}),
    ];
    let g = draw(pie, &rows, Options::default());
    assert!(g.summary.contains("the largest is b at 75%"), "{}", g.summary);
    assert!(
        g.summary.ends_with("1 of 2 panels could not be drawn."),
        "{}",
        g.summary
    );
    // Row-level findings name their panel.
    let scatter = json!({"type": "scatter", "title": "T", "encoding": {"x": "x", "y": "v", "facet": "f"}});
    let g = draw(
        scatter,
        &[
            json!({"x": 1, "v": 1, "f": "One"}),
            json!({"x": 1, "v": "abc", "f": "Two"}),
        ],
        Options::default(),
    );
    let bad = g.checks.iter().find(|c| c.kind == "non-numeric").unwrap();
    assert_eq!(bad.facet.as_deref(), Some("Two"));
}

#[test]
fn annotation_text_keeps_clear_of_bars_and_its_own_ring() {
    let rows: Vec<Value> = [("Jul", 280, 110), ("Aug", 340, 160), ("Sep", 420, 210)]
        .iter()
        .flat_map(|(m, e, s)| {
            [
                json!({"month": m, "mrr": e * 1000, "segment": "enterprise"}),
                json!({"month": m, "mrr": s * 1000, "segment": "mid-market"}),
            ]
        })
        .collect();
    let spec = json!({"type": "bar", "title": "MRR", "encoding": {"x": "month", "y": "mrr", "color": "segment"}, "annotations": [
        {"kind": "point", "at": {"x": "Aug", "y": 340000}, "text": "Enterprise reached $340K"},
        {"kind": "point", "at": {"x": "Sep", "y": 420000}, "text": "Enterprise reached $420K"}
    ]});
    for width in [358.0, 512.0, 720.0] {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                width,
                ..Options::default()
            },
        );
        let items = &g.panels[0].items;
        let mut blocked: Vec<richview_chart::geometry::Rect> = Vec::new();
        for item in items {
            match item {
                Item::Rect {
                    rect, role: Role::Mark, ..
                } => blocked.push(*rect),
                Item::Point {
                    center,
                    radius,
                    stroke,
                    role: Role::Annotation,
                    ..
                } => {
                    let r = radius + stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
                    blocked.push(richview_chart::geometry::Rect::new(
                        center[0] - r,
                        center[1] - r,
                        2.0 * r,
                        2.0 * r,
                    ));
                }
                _ => {}
            }
        }
        let mut accounted = 0;
        for item in items {
            if let Item::Text(t) = item
                && t.role == Role::Annotation
            {
                accounted += 1;
                for b in &blocked {
                    assert!(!t.bounds.overlaps(b, 0.0), "{width}: '{}' overlaps {b:?}", t.text);
                }
            }
        }
        accounted += g
            .dropped
            .iter()
            .filter(|d| d.kind.as_deref() == Some("annotation"))
            .count();
        assert_eq!(accounted, 2, "{width}: each note is drawn or reported");
    }
}

/// Hiding a series never trades its name for a legend or the legend for names, the hidden name
/// stays as the way back, and series-coloured names read at 4.5:1 on a dark surface (FEA-CH-27,
/// FEA-CH-33, STY-38).
#[test]
fn hiding_a_series_keeps_the_naming_and_names_stay_legible() {
    let rows: Vec<Value> = months(4)
        .into_iter()
        .enumerate()
        .flat_map(|(i, m)| {
            [("Long series name one", 1), ("Two", 3)].map(|(s, k)| json!({"m": m, "v": i * k + 2, "s": s}))
        })
        .collect();
    let spec = json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v", "color": "s"}});
    let names = |g: &Geometry| -> Vec<(String, Option<usize>)> {
        g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label && t.face != richview_chart::geometry::Face::Figures => {
                    Some((t.text.clone(), t.series))
                }
                _ => None,
            })
            .collect()
    };
    for width in [358.0, 640.0, 1100.0] {
        let all = draw(
            spec.clone(),
            &rows,
            Options {
                width,
                ..Options::default()
            },
        );
        for hide in ["Long series name one", "Two"] {
            let one = draw(
                spec.clone(),
                &rows,
                Options {
                    width,
                    hidden: vec![hide.into()],
                    ..Options::default()
                },
            );
            assert_eq!(
                all.legend.is_some(),
                one.legend.is_some(),
                "{width}: hiding {hide} keeps the naming"
            );
            if all.legend.is_none() {
                assert_eq!(names(&one).len(), 2, "{width}: the hidden name stays");
            }
        }
    }
    let dark = ChartTheme {
        surface: "#0b0d12".into(),
        palette: vec!["#195bff".into(), "#ff7a1a".into()],
        ..ChartTheme::default()
    };
    let block = ChartBlock::from_value(&spec).unwrap();
    let g = layout(
        &block,
        &rows,
        &dark,
        &Options {
            width: 1100.0,
            ..Options::default()
        },
    );
    let surface = richview_chart::color::parse("#0b0d12").unwrap();
    for item in &g.panels[0].items {
        if let Item::Text(t) = item
            && t.role == Role::Label
        {
            let [r, gr, b, _] = t.color.0;
            let c = richview_chart::color::Oklch::from_srgb([r as f64, gr as f64, b as f64]);
            assert!(
                richview_chart::color::contrast(c, surface) >= 4.4,
                "{} reads at {:.2}",
                t.text,
                richview_chart::color::contrast(c, surface)
            );
        }
    }
}

/// The text of the authored note `text`, the ring of the chart's first point annotation, and
/// the texts a chart adds by itself: automatic point names and values.
fn note<'a>(g: &'a Geometry, text: &str) -> Option<&'a richview_chart::geometry::Text> {
    g.panels[0].items.iter().find_map(|i| match i {
        Item::Text(t) if t.role == Role::Annotation && t.text == text => Some(t),
        _ => None,
    })
}

fn ring(g: &Geometry) -> [f64; 2] {
    g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Point {
                center,
                role: Role::Annotation,
                ..
            } => Some(*center),
            _ => None,
        })
        .expect("the point annotation's ring")
}

fn automatic_texts(g: &Geometry) -> Vec<&richview_chart::geometry::Text> {
    g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect()
}

/// Whether any drawn line of the panel comes within `gap` of `r`, its stroke width included.
fn touches_a_drawn_line(g: &Geometry, r: richview_chart::geometry::Rect, gap: f64) -> bool {
    g.panels[0].items.iter().any(|i| match i {
        Item::Line {
            path,
            stroke,
            role: Role::Mark,
            ..
        } => {
            let reach = r.inflate(gap + stroke.width / 2.0);
            path.flatten(0.25).iter().any(|line| {
                line.windows(2).any(|s| {
                    let steps = ((s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]) * 4.0).ceil().max(1.0) as usize;
                    (0..=steps).any(|k| {
                        let t = k as f64 / steps as f64;
                        let (x, y) = (s[0][0] + t * (s[1][0] - s[0][0]), s[0][1] + t * (s[1][1] - s[0][1]));
                        x >= reach.x && x <= reach.right() && y >= reach.y && y <= reach.bottom()
                    })
                })
            })
        }
        _ => false,
    })
}

/// An author's note is placed before a scatter's point names and keeps the place it has on a
/// chart that names no point at all; a name whose place the note took moves or is dropped and
/// recorded, and the author's highlight is still named (FEA-CH-29).
#[test]
fn an_authors_note_takes_its_place_before_the_point_names() {
    let rows = vec![
        json!({"clinic":"Northgate","wait":12,"score":71}),
        json!({"clinic":"Riverside","wait":18,"score":64}),
        json!({"clinic":"Hillcrest","wait":22,"score":69}),
        json!({"clinic":"Old Town","wait":27,"score":58}),
        json!({"clinic":"Harbour","wait":31,"score":62}),
        json!({"clinic":"Eastfield","wait":36,"score":55}),
        json!({"clinic":"Marsh Lane","wait":41,"score":49}),
        json!({"clinic":"Southbank","wait":44,"score":88}),
    ];
    let named = json!({"type":"scatter","title":"T",
        "encoding":{"x":"wait","y":"score","label":"clinic"},
        "highlight":{"by":"clinic","include":["Old Town"]}});
    let notes = json!([
        {"kind":"point","at":{"x":44,"y":88},"text":"New triage desk"},
        {"kind":"point","at":{"x":12,"y":71},"text":"Shortest wait"}
    ]);
    for width in [358.0, 681.0] {
        let opts = Options {
            width,
            ..Options::default()
        };
        let mut annotated = named.clone();
        annotated["annotations"] = notes.clone();
        let mut alone = json!({"type":"scatter","title":"T","encoding":{"x":"wait","y":"score"}});
        alone["annotations"] = notes.clone();
        let (control, g, alone) = (
            draw(named.clone(), &rows, opts.clone()),
            draw(annotated, &rows, opts.clone()),
            draw(alone, &rows, opts),
        );
        for text in ["New triage desk", "Shortest wait"] {
            let placed = note(&g, text).unwrap_or_else(|| panic!("{width}: '{text}' is drawn"));
            assert_eq!(
                Some(placed.bounds),
                note(&alone, text).map(|t| t.bounds),
                "{width}: '{text}' stands where it does on a chart that names no point"
            );
            for name in automatic_texts(&g) {
                assert!(
                    !name.bounds.inflate(2.0).overlaps(&placed.bounds, 0.0),
                    "{width}: the name '{}' keeps off the note '{text}'",
                    name.text
                );
            }
        }
        let name = |g: &Geometry, text: &str| {
            automatic_texts(g)
                .into_iter()
                .find(|t| t.text == text)
                .map(|t| t.bounds)
        };
        assert!(name(&control, "Old Town").is_some(), "{width}: the highlight is named");
        assert!(
            name(&g, "Old Town").is_some(),
            "{width}: the author's highlight is still named beside the notes"
        );
        // Each name the chart chose is drawn clear of the author's rings, or recorded.
        let rings: Vec<_> = g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Point {
                    center,
                    radius,
                    stroke,
                    role: Role::Annotation,
                    ..
                } => {
                    let r = radius + stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
                    Some(richview_chart::geometry::Rect::new(
                        center[0] - r,
                        center[1] - r,
                        2.0 * r,
                        2.0 * r,
                    ))
                }
                _ => None,
            })
            .collect();
        assert_eq!(rings.len(), 2);
        for chosen in automatic_texts(&control) {
            match name(&g, &chosen.text) {
                Some(bounds) => assert!(
                    rings.iter().all(|ring| !bounds.overlaps(ring, 0.0)),
                    "{width}: '{}' keeps off the author's rings",
                    chosen.text
                ),
                None => assert!(
                    g.dropped
                        .iter()
                        .any(|d| d.kind.as_deref() == Some("point") && d.text == chosen.text),
                    "{width}: the name '{}' that lost its place is recorded",
                    chosen.text
                ),
            }
        }
        // Placement moves no observation (FEA-CH-29).
        assert_eq!(g.panels[0].observations, control.panels[0].observations);
        let marks = |g: &Geometry| -> Vec<Item> {
            g.panels[0]
                .items
                .iter()
                .filter(|i| i.role() == Role::Mark)
                .cloned()
                .collect()
        };
        assert_eq!(marks(&g), marks(&control));
    }
}

/// The 40 synthetic stations of `examples/scatter-highlights`: 34 in one crowd, Jarrow in its
/// middle, where no name has room beside its point.
fn stations() -> Vec<Value> {
    [
        ("Aldmoor", 50.8, 58.6),
        ("Brightwell", 53.1, 58.0),
        ("Carrow", 52.3, 60.1),
        ("Dunmere", 48.9, 61.1),
        ("Eastvale", 48.8, 60.5),
        ("Farnley", 49.0, 58.1),
        ("Gresham", 51.5, 63.3),
        ("Hollin", 49.4, 59.1),
        ("Ivesley", 52.9, 64.1),
        ("Jarrow", 52.5, 60.3),
        ("Kelby", 55.3, 57.8),
        ("Lowick", 54.5, 59.5),
        ("Marden", 49.5, 58.3),
        ("Norbury", 50.7, 63.2),
        ("Oakham", 49.8, 61.6),
        ("Pendle", 53.0, 60.1),
        ("Quorn", 52.3, 57.9),
        ("Redmere", 48.9, 58.9),
        ("Saltby", 53.3, 60.5),
        ("Tarvin", 50.7, 61.6),
        ("Ulverby", 51.7, 59.6),
        ("Vexford", 54.1, 62.4),
        ("Wickham", 50.2, 61.5),
        ("Yarrow", 52.2, 63.6),
        ("Ashgrove", 53.6, 59.5),
        ("Bramley", 55.4, 58.3),
        ("Corbridge", 51.4, 62.8),
        ("Denholm", 49.6, 60.9),
        ("Elmstead", 48.8, 62.2),
        ("Fenwick", 53.9, 61.5),
        ("Gransden", 54.6, 59.7),
        ("Hartley", 53.4, 61.7),
        ("Inkpen", 52.6, 60.7),
        ("Kirkby", 54.4, 64.1),
        ("Langdon", 14.2, 22.5),
        ("Melbury", 88.4, 83.1),
        ("Nunton", 21.7, 81.9),
        ("Ormsby", 79.3, 18.6),
        ("Poulton", 33.0, 40.2),
        ("Radley", 67.5, 70.4),
    ]
    .iter()
    .map(|(station, rain, sun)| json!({"station": station, "rainfall_mm": rain, "sunshine_h": sun}))
    .collect()
}

/// The leader of the panel's one name on a leader: where it starts and ends.
fn leader(g: &Geometry) -> Option<[[f64; 2]; 2]> {
    g.panels[0].items.iter().find_map(|i| match i {
        Item::Line {
            path,
            role: Role::Leader,
            ..
        } => {
            let line = path.flatten(0.25).into_iter().next()?;
            Some([*line.first()?, *line.last()?])
        }
        _ => None,
    })
}

/// Whether the straight line from `a` to `b` passes through `r`.
fn crosses([a, b]: [[f64; 2]; 2], r: richview_chart::geometry::Rect) -> bool {
    let steps = ((b[0] - a[0]).hypot(b[1] - a[1]) * 4.0).ceil().max(1.0) as usize;
    (0..=steps).any(|k| {
        let t = k as f64 / steps as f64;
        r.contains([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])])
    })
}

/// A highlighted point the author also rings keeps its name: in a crowd the name stands off on
/// a leader, which starts inside the point's own ring. The leader may cross an author's rule
/// but never the rule's label, and the name's box keeps off every ring and rule (FEA-CH-29).
#[test]
fn a_ringed_highlight_keeps_its_name_on_a_leader_that_may_cross_a_rule() {
    let rows = stations();
    let named = json!({"type":"scatter","title":"T",
        "encoding":{"x":"rainfall_mm","y":"sunshine_h"},
        "highlight":{"by":"station","include":["Jarrow"]}});
    let ringed = json!([{"kind":"point","at":{"x":52.5,"y":60.3},"text":"Moved in March"}]);
    // Four rules close round the crowd, so every place for a name on a leader lies past one.
    let fenced = json!([
        {"kind":"point","at":{"x":52.5,"y":60.3},"text":"Moved in March"},
        {"kind":"line","at":{"y":66},"text":"Target"},
        {"kind":"line","at":{"y":56}},
        {"kind":"line","at":{"x":47}},
        {"kind":"line","at":{"x":57}}
    ]);
    let opts = Options {
        width: 632.0,
        ..Options::default()
    };
    let name = |g: &Geometry| {
        automatic_texts(g)
            .into_iter()
            .find(|t| t.text == "Jarrow")
            .map(|t| t.bounds)
    };
    let control = draw(named.clone(), &rows, opts.clone());
    assert!(name(&control).is_some(), "the highlight is named");
    assert!(
        leader(&control).is_some(),
        "the crowd leaves the name a place only on a leader"
    );
    for (case, annotations) in [("ringed", ringed), ("fenced", fenced)] {
        let mut spec = named.clone();
        spec["annotations"] = annotations;
        let g = draw(spec, &rows, opts.clone());
        let at = ring(&g);
        let bounds = name(&g).unwrap_or_else(|| panic!("{case}: the ringed highlight keeps its name: {:?}", g.dropped));
        let line = leader(&g).unwrap_or_else(|| panic!("{case}: the name stands on a leader"));
        assert!(
            (line[0][0] - at[0]).abs() < 0.01 && (line[0][1] - at[1]).abs() < 0.01,
            "{case}: the leader starts at the ringed point: {line:?}, ring at {at:?}"
        );
        let mut crossed = 0;
        for item in &g.panels[0].items {
            match item {
                Item::Point {
                    center,
                    radius,
                    role: Role::Annotation,
                    ..
                } => assert!(
                    !bounds.overlaps(
                        &richview_chart::geometry::Rect::new(
                            center[0] - radius,
                            center[1] - radius,
                            2.0 * radius,
                            2.0 * radius
                        ),
                        0.0
                    ),
                    "{case}: the name keeps off the ring"
                ),
                Item::Line {
                    path,
                    role: Role::Annotation,
                    ..
                } => {
                    let rule = path.bounds().expect("the rule's bounds");
                    assert!(
                        !bounds.inflate(1.0).overlaps(&rule, 0.0),
                        "{case}: the name keeps off the author's rule: {bounds:?}, rule {rule:?}"
                    );
                    crossed += usize::from(crosses(line, rule.inflate(0.5)));
                }
                Item::Text(t) if t.role == Role::Annotation => {
                    assert!(
                        !bounds.overlaps(&t.bounds, 0.0),
                        "{case}: the name keeps off '{}'",
                        t.text
                    );
                    assert!(
                        !crosses(line, t.bounds),
                        "{case}: the leader never crosses the author's text '{}'",
                        t.text
                    );
                }
                _ => {}
            }
        }
        if case == "fenced" {
            assert!(crossed > 0, "the leader crosses a rule to reach the name's place");
        }
    }
}

/// A note keeps off every rule and ring the author drew, whichever is written first: a rule or
/// a second ring that stands where the note would go moves the note, and never strikes through
/// it (FEA-CH-32).
#[test]
fn a_note_keeps_off_the_authors_other_rules_and_rings_in_either_order() {
    let rows = vec![json!({"x": 0, "y": 0}), json!({"x": 100, "y": 100})];
    let note = json!({"kind":"point","at":{"x":50,"y":50},"text":"Checked twice"});
    // Each stands 4 units right of the note's point, inside the place the note takes alone.
    let rule = json!({"kind":"line","at":{"x":54}});
    let second = json!({"kind":"point","at":{"x":54,"y":50},"text":"Second"});
    let draw_with = |annotations: Value| {
        draw(
            json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"},"annotations":annotations}),
            &rows,
            Options {
                width: 632.0,
                ..Options::default()
            },
        )
    };
    let alone = draw_with(json!([note.clone()]));
    // The note's own ring is the one its text stands beside; every other mark it keeps off.
    let (own, alone) = (ring(&alone), note_bounds(&alone, "Checked twice"));
    for (case, annotations) in [
        ("the rule after the note", json!([note.clone(), rule.clone()])),
        ("the rule before the note", json!([rule, note.clone()])),
        ("a second ring after the note", json!([note.clone(), second.clone()])),
        ("a second ring before the note", json!([second, note])),
    ] {
        let g = draw_with(annotations);
        let placed = note_bounds(&g, "Checked twice");
        let mut others = 0;
        for item in &g.panels[0].items {
            let other = match item {
                Item::Line {
                    path,
                    role: Role::Annotation,
                    ..
                } => path.bounds().expect("the rule's bounds"),
                Item::Point {
                    center,
                    radius,
                    role: Role::Annotation,
                    ..
                } if (center[0] - own[0]).abs() > 1.0 => richview_chart::geometry::Rect::new(
                    center[0] - radius,
                    center[1] - radius,
                    2.0 * radius,
                    2.0 * radius,
                ),
                _ => continue,
            };
            if other.overlaps(&alone.inflate(1.0), 0.0) {
                others += 1;
                assert!(
                    !placed.inflate(1.0).overlaps(&other, 0.0),
                    "{case}: the note keeps off it: note {placed:?}, mark {other:?}"
                );
            }
        }
        assert!(
            others > 0,
            "{case}: the other mark stands where the note would go alone"
        );
    }
}

/// Keeping off the author's rules is a preference, not a ban: three labelled rules standing
/// close together leave the middle label no place clear of its neighbours, and it is drawn
/// across one of them, never left out and never over another label (FEA-CH-32).
#[test]
fn a_label_with_no_place_clear_of_the_other_rules_stands_across_one() {
    let rows = vec![json!({"x": 0, "y": 0}), json!({"x": 100, "y": 100})];
    let g = draw(
        json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"},"annotations":[
            {"kind":"line","at":{"x":46},"text":"Alpha"},
            {"kind":"line","at":{"x":53},"text":"Bravo"},
            {"kind":"line","at":{"x":60},"text":"Charlie"}
        ]}),
        &rows,
        Options {
            width: 358.0,
            ..Options::default()
        },
    );
    let labels: Vec<_> = ["Alpha", "Bravo", "Charlie"]
        .iter()
        .map(|text| note_bounds(&g, text))
        .collect();
    for (i, a) in labels.iter().enumerate() {
        for b in &labels[i + 1..] {
            assert!(!a.inflate(1.0).overlaps(b, 0.0), "no label covers another: {a:?} {b:?}");
        }
    }
    let rules: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line {
                path,
                role: Role::Annotation,
                ..
            } => path.bounds(),
            _ => None,
        })
        .collect();
    assert_eq!(rules.len(), 3);
    let crossed = |label: &richview_chart::geometry::Rect| {
        rules
            .iter()
            .filter(|rule| rule.x > label.x && rule.x < label.right())
            .count()
    };
    assert_eq!(
        crossed(&labels[0]),
        0,
        "the first label has a clear place: {:?}",
        labels[0]
    );
    assert!(
        crossed(&labels[1]) > 0,
        "the middle label has none, which is why it stands across a rule: {:?}, rules {rules:?}",
        labels[1]
    );
}

fn note_bounds(g: &Geometry, text: &str) -> richview_chart::geometry::Rect {
    note(g, text)
        .unwrap_or_else(|| panic!("'{text}' is drawn: {:?}", g.dropped))
        .bounds
}

/// The note takes the nearest place beside its ring, which the point's own name had before:
/// the name moves to another side (FEA-CH-29).
#[test]
fn the_automatic_name_gives_its_place_beside_the_point_to_the_note() {
    let rows = vec![
        json!({"name":"Alpha","x":2,"y":5}),
        json!({"name":"Bravo","x":5,"y":2}),
        json!({"name":"Charlie","x":8,"y":8}),
    ];
    let spec = json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y","label":"name"},
        "axes":{"x":{"min":0,"max":10},"y":{"min":0,"max":10}}});
    let opts = Options {
        width: 640.0,
        ..Options::default()
    };
    let control = draw(spec.clone(), &rows, opts.clone());
    let before = automatic_texts(&control)
        .into_iter()
        .find(|t| t.text == "Alpha")
        .expect("the name without a note")
        .bounds;
    let mut annotated = spec;
    annotated["annotations"] = json!([{"kind":"point","at":{"x":2,"y":5},"text":"The author's note"}]);
    let g = draw(annotated, &rows, opts);
    let placed = note(&g, "The author's note").expect("the note").bounds;
    let at = ring(&g);
    assert!(
        placed.x > at[0] && (placed.center()[1] - at[1]).abs() < 0.5,
        "the note stands right of its ring, on its level: {placed:?} at {at:?}"
    );
    assert!(placed.overlaps(&before, 0.0), "the note takes the place the name had");
    let after = automatic_texts(&g)
        .into_iter()
        .find(|t| t.text == "Alpha")
        .expect("the name moves, it is not dropped")
        .bounds;
    assert!(!after.inflate(2.0).overlaps(&placed, 0.0));
}

/// A note keeps clear of the drawn lines wherever a place clear of them exists, and the
/// automatic values and series names placed after it keep off it; where the lines leave no
/// clear place the note still stands (FEA-CH-29, FEA-CH-32).
#[test]
fn a_note_keeps_clear_of_the_drawn_lines_and_automatic_labels_keep_off_it() {
    let retained = [
        ("New", [0.72, 0.74, 0.69, 0.78, 0.81, 0.83]),
        ("Existing", [0.82, 0.84, 0.80, 0.86, 0.87, 0.89]),
    ];
    let rows: Vec<Value> = retained
        .iter()
        .flat_map(|(cohort, values)| {
            values
                .iter()
                .enumerate()
                .map(move |(i, v)| json!({"month": format!("2025-{:02}", i + 1), "retained": v, "cohort": cohort}))
        })
        .collect();
    let spec = json!({"type":"line","title":"T","encoding":{"x":"month","y":"retained","color":"cohort"},
        "axes":{"y":{"format":"percent"}},
        "annotations":[{"kind":"point","at":{"x":"2025-06","y":0.89},"text":"Best month yet"}]});
    for width in [358.0, 681.0] {
        let opts = Options {
            width,
            ..Options::default()
        };
        let g = draw(spec.clone(), &rows, opts.clone());
        let Some(placed) = note(&g, "Best month yet") else {
            // The phone plot has no place for the note at all: it is recorded, as before.
            assert!(width < 480.0, "{width}: the note is drawn");
            assert!(
                g.dropped
                    .iter()
                    .any(|d| d.kind.as_deref() == Some("annotation") && d.text == "Best month yet")
            );
            continue;
        };
        assert!(
            !touches_a_drawn_line(&g, placed.bounds, 1.5),
            "{width}: the note stands clear of both lines: {:?}",
            placed.bounds
        );
        let mut alone = spec.clone();
        alone["auto_annotations"] = json!(false);
        let alone = draw(alone, &rows, opts);
        assert_eq!(
            Some(placed.bounds),
            note(&alone, "Best month yet").map(|t| t.bounds),
            "{width}: the note stands where it does with no automatic value"
        );
        for label in automatic_texts(&g) {
            assert!(
                !label.bounds.overlaps(&placed.bounds, 1.0),
                "{width}: '{}' keeps off the note",
                label.text
            );
        }
    }
    // A line that crosses every place round the note: the note stays, clear of the points.
    let zigzag: Vec<Value> = (0..=200)
        .map(|i| json!({"x": i, "y": if i % 2 == 0 { 0 } else { 100 }}))
        .collect();
    let dense = json!({"type":"line","title":"T","encoding":{"x":"x","y":"y"},"style":{"curve":"linear"},
        "annotations":[{"kind":"text","at":{"x":100,"y":50},"text":"Still said"}]});
    let g = draw(
        dense,
        &zigzag,
        Options {
            width: 640.0,
            ..Options::default()
        },
    );
    assert!(
        note(&g, "Still said").is_some(),
        "no clear place among the lines never costs the author the note: {:?}",
        g.dropped
    );
}

/// A horizontal rule's label never sits on its rule: it stands above it at the plot's right
/// edge, and where a bar is in the way there it moves along the rule before it is dropped
/// (FEA-CH-32).
#[test]
fn a_horizontal_rules_label_stands_beside_its_rule_never_on_it() {
    let rule = |g: &Geometry| -> f64 {
        g.panels[0]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Line {
                    path,
                    role: Role::Annotation,
                    ..
                } => path.bounds().map(|b| b.center()[1]),
                _ => None,
            })
            .expect("the rule")
    };
    let weeks: Vec<Value> = [120, 135, 128, 190, 162, 150, 141, 158]
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"week": i + 1, "signups": v}))
        .collect();
    let line = json!({"type":"line","title":"T","encoding":{"x":"week","y":"signups"},
        "annotations":[{"kind":"line","at":{"y":175},"text":"Target"}]});
    let quarters: Vec<Value> = [("Q1", 42), ("Q2", 58), ("Q3", 61), ("Q4", 73)]
        .iter()
        .map(|(q, v)| json!({"quarter": q, "units": v}))
        .collect();
    let bars = json!({"type":"bar","title":"T","encoding":{"x":"quarter","y":"units"},
        "annotations":[{"kind":"line","at":{"y":60},"text":"Target 60"}]});
    for width in [358.0, 681.0] {
        let opts = Options {
            width,
            ..Options::default()
        };
        let g = draw(line.clone(), &weeks, opts.clone());
        let (y, plot) = (rule(&g), g.panels[0].plot);
        let label = note(&g, "Target")
            .unwrap_or_else(|| panic!("{width}: the label is drawn"))
            .bounds;
        assert!(
            label.bottom() < y,
            "{width}: the label stands above its rule: {label:?}, rule at {y}"
        );
        assert!(y - label.bottom() <= 4.0, "{width}: and reads as that rule's label");
        assert!(
            (plot.right() - label.right() - 4.0).abs() < 0.01,
            "{width}: at the plot's right edge: {label:?} in {plot:?}"
        );

        let g = draw(bars.clone(), &quarters, opts);
        let (y, plot) = (rule(&g), g.panels[0].plot);
        let label = note(&g, "Target 60")
            .unwrap_or_else(|| panic!("{width}: a bar under the right end moves the label: {:?}", g.dropped))
            .bounds;
        assert!(
            label.bottom() < y || label.y > y,
            "{width}: the label is not struck through by its rule: {label:?}, rule at {y}"
        );
        assert!(label.inside(&plot, 0.0));
        for item in &g.panels[0].items {
            match item {
                Item::Rect {
                    rect, role: Role::Mark, ..
                } => assert!(!label.overlaps(rect, 0.0), "{width}: the label keeps off the bars"),
                Item::Text(t) if t.role == Role::Label => {
                    assert!(
                        !t.bounds.overlaps(&label, 1.0),
                        "{width}: the value '{}' keeps off the label",
                        t.text
                    )
                }
                _ => {}
            }
        }
    }
}

/// Where bars pass the rule at both ends of the plot, so that no place beside the rule at
/// either edge is free, the label still stands near the rule's right end, off the rule and the
/// bars, as it did before the label got its places beside the rule (FEA-CH-32).
#[test]
fn a_rules_label_with_both_ends_taken_stands_near_its_right_end() {
    let quarters: Vec<Value> = [("Q1", 62), ("Q2", 30), ("Q3", 30), ("Q4", 62)]
        .iter()
        .map(|(q, v)| json!({"quarter": q, "units": v}))
        .collect();
    let bars = json!({"type":"bar","title":"T","encoding":{"x":"quarter","y":"units"},
        "annotations":[{"kind":"line","at":{"y":60},"text":"Target 60"}]});
    let g = draw(
        bars,
        &quarters,
        Options {
            width: 632.0,
            ..Options::default()
        },
    );
    let plot = g.panels[0].plot;
    let y = g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                path,
                role: Role::Annotation,
                ..
            } => path.bounds().map(|b| b.center()[1]),
            _ => None,
        })
        .expect("the rule");
    let label = note(&g, "Target 60")
        .unwrap_or_else(|| panic!("the goal line keeps its label: {:?}", g.dropped))
        .bounds;
    assert!(
        label.bottom() <= y - 3.0 || label.y >= y + 3.0,
        "the label is off its rule: {label:?}, rule at {y}"
    );
    assert!(label.inside(&plot, 0.0));
    for item in &g.panels[0].items {
        match item {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => assert!(!label.overlaps(rect, 0.0), "the label keeps off the bars"),
            Item::Text(t) if t.role == Role::Label => {
                assert!(
                    !t.bounds.overlaps(&label, 1.0),
                    "the value '{}' keeps off the label",
                    t.text
                )
            }
            _ => {}
        }
    }
}
