//! Chart rules measured on the geometry model: facets, annotations, missing values, series
//! identity, zoom and motion (FEA-CH-09, FEA-CH-35, FEA-CH-38, FEA-CH-49, FEA-CH-63, STY-40).

use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
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

#[test]
fn area_names_use_the_left_margin_and_leave_the_right_for_data() {
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
        for hidden in [vec![], vec!["free".to_owned()], vec!["paid".to_owned()]] {
            let g = draw(
                spec.clone(),
                &rows,
                Options {
                    width,
                    hidden,
                    ..Options::default()
                },
            );
            if all.legend.is_some() {
                assert!(g.legend.is_some(), "hiding never promotes an original legend to names");
            }
            for panel in &g.panels {
                assert!(
                    (panel.frame.right() - panel.plot.right() - 9.0).abs() <= 1e-6,
                    "area keeps only the endpoint halo inset, not a name gutter, at {width}"
                );
                let names: Vec<_> = panel
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
                if g.legend.is_some() {
                    assert!(names.is_empty(), "legend fallback does not duplicate direct names");
                    continue;
                }
                assert_eq!(names.len(), 2, "complete names include the hidden series");
                let targets: Vec<_> = names
                    .iter()
                    .map(|t| {
                        assert!(t.bounds.right() < panel.plot.x, "{} sits left of the area", t.text);
                        let w = (t.bounds.w + 4.0).max(24.0);
                        let h = (t.bounds.h + 4.0).max(24.0);
                        let c = t.bounds.center();
                        richview_chart::geometry::Rect::new(c[0] - w / 2.0, c[1] - h / 2.0, w, h)
                    })
                    .collect();
                for (i, target) in targets.iter().enumerate() {
                    assert!(target.inside(&panel.frame, 0.0), "whole name target fits");
                    assert!(targets.iter().skip(i + 1).all(|other| !target.overlaps(other, 0.0)));
                    assert!(
                        panel.items.iter().all(|item| match item {
                            Item::Text(t) if matches!(t.role, Role::AxisLabel | Role::AxisTitle | Role::PanelTitle) =>
                                !target.inflate(2.0).overlaps(&t.bounds, 0.0),
                            _ => true,
                        }),
                        "{} clears axis ink at {width}: {target:?}; axis: {:?}",
                        names[i].text,
                        panel
                            .items
                            .iter()
                            .filter_map(|item| match item {
                                Item::Text(t)
                                    if matches!(t.role, Role::AxisLabel | Role::AxisTitle | Role::PanelTitle)
                                        && target.inflate(2.0).overlaps(&t.bounds, 0.0) =>
                                    Some((t.text.as_str(), t.bounds)),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn area_names_follow_the_first_values_while_lines_follow_the_last() {
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
        let y = |name: &str| {
            g.panels[0]
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text(t) if t.role == Role::Label && t.text == name => Some(t.bounds.center()[1]),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            y("A") < y("B"),
            kind == "area",
            "names follow the corresponding end: {kind}"
        );
    }
}

#[test]
fn area_names_fall_back_when_the_refitted_axis_has_no_clear_band() {
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
    assert!(all.legend.is_none(), "all-shown axis leaves clear bands for two names");
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
    assert!(
        hidden.legend.is_some(),
        "refitted ticks leave no room for complete name targets"
    );
    assert!(hidden.series[0].hidden);
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

#[test]
fn area_name_leaders_stay_inside_authored_value_bounds() {
    let rows = vec![
        json!({"x":0,"v":100,"s":"A"}),
        json!({"x":1,"v":10,"s":"A"}),
        json!({"x":0,"v":6,"s":"B"}),
        json!({"x":1,"v":8,"s":"B"}),
    ];
    let g = draw(
        json!({"type":"area","title":"T","encoding":{"x":"x","y":"v","color":"s"},"axes":{"y":{"min":0,"max":20}}}),
        &rows,
        Options {
            width: 800.0,
            ..Options::default()
        },
    );
    assert!(g.legend.is_none());
    let panel = &g.panels[0];
    assert!(
        panel
            .observations
            .iter()
            .flat_map(|o| &o.readout.lines)
            .any(|l| l.value == "100"),
        "the authored source value remains available"
    );
    for item in &panel.items {
        if let Item::Line {
            path,
            role: Role::Leader,
            ..
        } = item
        {
            for cmd in &path.cmds {
                if let Cmd::M { p } | Cmd::L { p } = cmd {
                    assert!(
                        panel.frame.contains(*p),
                        "a clipped first value must not send its name leader outside the figure: {p:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn zoom_keeps_an_area_name_restorable_without_rows_inside_the_window() {
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
        assert!(g.legend.is_none(), "the wide plot has safe name bands");
        let names: Vec<_> = g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            names.contains(&"A") && names.contains(&"LateB"),
            "every series remains controllable while zoomed: {names:?}"
        );
    }
}

#[test]
fn crowded_area_names_do_not_point_toward_a_different_value_tick() {
    let rows: Vec<_> = (0..6)
        .flat_map(|x| (0..3).map(move |s| json!({"x":x,"v":100+s*2+x*(s+1)*3,"s":(["North","South","West"][s])})))
        .collect();
    let spec = json!({"type":"area","title":"T","encoding":{"x":"x","y":"v","color":"s"}});
    for width in [358.0, 576.0] {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                width,
                ..Options::default()
            },
        );
        if width == 358.0 {
            assert!(
                g.legend.is_some(),
                "crowded phone anchors cannot fit complete names within half a tick spacing"
            );
        }
        if g.legend.is_some() {
            continue;
        }
        let panel = &g.panels[0];
        let y = |label: &str| {
            panel
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Text(t) if t.role == Role::AxisLabel && t.text == label => Some(t.bounds.center()[1]),
                    _ => None,
                })
                .unwrap()
        };
        let limit = (y("50") - y("100")).abs() / 2.0;
        for item in &panel.items {
            if let Item::Line {
                path,
                role: Role::Leader,
                ..
            } = item
            {
                let points: Vec<_> = path
                    .cmds
                    .iter()
                    .filter_map(|c| match c {
                        Cmd::M { p } | Cmd::L { p } => Some(p),
                        _ => None,
                    })
                    .collect();
                assert!(
                    (points[0][1] - points[1][1]).abs() <= limit + 1e-8,
                    "an area name stays within half the y-tick spacing"
                );
            }
        }
    }
}

#[test]
fn area_names_use_the_visible_stack_boundary_and_restart_after_calendar_gaps() {
    let numeric = vec![
        json!({"x":0,"v":10,"s":"A"}),
        json!({"x":1,"v":20,"s":"A"}),
        json!({"x":0,"v":20,"s":"B"}),
        json!({"x":1,"v":5,"s":"B"}),
    ];
    let calendar = vec![
        json!({"x":"2025-01-01","v":10,"s":"A"}),
        json!({"x":"2025-04-01","v":20,"s":"A"}),
        json!({"x":"2025-01-01","v":5,"s":"B"}),
        json!({"x":"2025-02-01","v":5,"s":"B"}),
        json!({"x":"2025-04-01","v":5,"s":"B"}),
    ];
    let spec = json!({"type":"area","title":"T","stack":true,"encoding":{"x":"x","y":"v","color":"s"},"axes":{"y":{"min":0,"max":30}}});
    for (rows, hidden, zoom, expected) in [
        (numeric.clone(), vec![], None, 30.0),
        (numeric, vec!["A".to_owned()], None, 20.0),
        (calendar, vec![], Some([1743379200000.0, 1743552000000.0]), 5.0),
    ] {
        let g = draw(
            spec.clone(),
            &rows,
            Options {
                width: 800.0,
                hidden,
                zoom,
                ..Options::default()
            },
        );
        assert!(
            g.legend.is_none(),
            "these separated stack boundaries have clear name positions"
        );
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
        let wanted = axis_y("0") + (axis_y("30") - axis_y("0")) * expected / 30.0;
        let slot = g.series.iter().position(|s| s.name == "B").unwrap();
        let actual = panel
            .items
            .iter()
            .find_map(|i| match i {
                Item::Line {
                    path,
                    role: Role::Leader,
                    series: Some(s),
                    ..
                } if *s == slot => path.cmds.iter().find_map(|c| match c {
                    Cmd::M { p } => Some(p[1]),
                    _ => None,
                }),
                _ => None,
            })
            .expect("a name anchored on a tick is displaced with a leader");
        assert!(
            (actual - wanted).abs() < 1e-8,
            "B starts at the visible boundary {expected}, not a last value or an unpainted stack sum"
        );
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
