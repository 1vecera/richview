//! Automatic values of the charts that printed none (FEA-CH-28 as chart design ruled on
//! 16 Nov 2026): a dumbbell's two ends, a bullet's measure, a box plot's medians, a treemap's
//! largest tiles, a combo's bars and last line value, and a financial chart's last close. Rows
//! share the budget of five, three under 480 px of plot width, and every value is printed or
//! none.

mod common;

use common::*;
use richview_chart::geometry::{DropReason, Face, Geometry, Item, Rect, Role, Text};
use serde_json::{Value, json};

fn fixture(name: &str) -> (Value, Vec<Value>) {
    let path = format!("{}/tests/fixtures/types/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    (v["spec"].clone(), v["rows"].as_array().expect("rows").clone())
}

fn values(g: &Geometry) -> Vec<&Text> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label && t.face == Face::Figures => Some(t),
            _ => None,
        })
        .collect()
}

fn texts(g: &Geometry) -> Vec<&str> {
    values(g).iter().map(|t| t.text.as_str()).collect()
}

fn value_drops(g: &Geometry) -> Vec<(&str, DropReason)> {
    g.dropped
        .iter()
        .filter(|d| d.kind.as_deref() == Some("value"))
        .map(|d| (d.text.as_str(), d.reason))
        .collect()
}

/// Every value keeps inside the plot, 2 px clear of every dot, bar and stem, and no gridline
/// runs through one: the gridline stops short of it.
fn stands_clear(g: &Geometry) {
    let panel = &g.panels[0];
    for t in values(g) {
        assert!(
            t.bounds.inside(&panel.plot, 0.5),
            "{} inside the plot: {:?}",
            t.text,
            t.bounds
        );
        let near = t.bounds.inflate(2.0);
        for item in &panel.items {
            match item {
                Item::Point {
                    center,
                    radius,
                    role: Role::Mark,
                    ..
                } => {
                    let dot = Rect::new(center[0] - radius, center[1] - radius, 2.0 * radius, 2.0 * radius);
                    assert!(!near.overlaps(&dot, 0.0), "{} clear of a dot", t.text);
                }
                Item::Rect {
                    rect, role: Role::Mark, ..
                } => {
                    assert!(!near.overlaps(rect, 0.0), "{} clear of a bar", t.text);
                }
                Item::Line {
                    path,
                    role: Role::Mark | Role::Grid,
                    ..
                } => {
                    for line in path.flatten(0.5) {
                        for s in line.windows(2) {
                            let (x0, x1) = (s[0][0].min(s[1][0]), s[0][0].max(s[1][0]));
                            let (y0, y1) = (s[0][1].min(s[1][1]), s[0][1].max(s[1][1]));
                            let stroke = Rect::new(x0, y0, x1 - x0, y1 - y0);
                            assert!(
                                !t.bounds.overlaps(&stroke.inflate(0.5), 0.0),
                                "no line through {}: {stroke:?} and {:?}",
                                t.text,
                                t.bounds
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[test]
fn a_dumbbell_prints_both_ends_of_every_row_outside_its_dots() {
    let (spec, rows) = fixture("dumbbell");
    for orientation in ["vertical", "horizontal"] {
        let mut spec = spec.clone();
        spec["style"]["orientation"] = json!(orientation);
        let g = draw_at(&spec, &rows, 1100.0);
        let mut printed = texts(&g);
        printed.sort();
        assert_eq!(
            printed,
            ["48", "51", "55", "62", "66", "70", "78", "88"],
            "{orientation}"
        );
        assert!(g.dropped.is_empty(), "{orientation}: {:?}", g.dropped);
        stands_clear(&g);
        // Each row's two values stand beyond its two dots, never between them.
        let panel = &g.panels[0];
        let axis = usize::from(orientation == "vertical");
        for (n, _) in panel.observations.iter().enumerate() {
            let dots: Vec<f64> = panel
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Point {
                        center,
                        obs: Some(o),
                        role: Role::Mark,
                        ..
                    } if *o == n => Some(center[axis]),
                    _ => None,
                })
                .collect();
            let (lo, hi) = (dots[0].min(dots[1]), dots[0].max(dots[1]));
            let all = values(&g);
            let mine: Vec<&Text> = all
                .iter()
                .copied()
                .filter(|t| (t.bounds.center()[1 - axis] - panel.observations[n].x).abs() < 1.0)
                .collect();
            assert_eq!(mine.len(), 2, "{orientation}: row {n} has two values");
            for t in mine {
                let c = t.bounds.center()[axis];
                assert!(c < lo || c > hi, "{orientation}: {} stands outside its dots", t.text);
            }
        }
        contract(&spec, &rows);
    }
}

/// The numbers a reader can get from a chart's values, printed or recorded as dropped, sorted.
fn numbers(g: &Geometry) -> Vec<String> {
    let mut all: Vec<String> = texts(g).into_iter().map(str::to_owned).collect();
    all.extend(value_drops(g).into_iter().map(|(text, _)| text.to_owned()));
    all.sort();
    all
}

#[test]
fn rows_over_the_budget_print_no_value_and_record_each() {
    let (spec, rows) = fixture("dumbbell");
    // Four rows are over the three a plot under 480 px may label.
    let narrow = draw_at(&spec, &rows, 390.0);
    assert!(narrow.panels[0].plot.w < 480.0);
    assert!(texts(&narrow).is_empty());
    let drops = value_drops(&narrow);
    assert_eq!(drops.len(), 8, "{drops:?}");
    assert!(
        drops.iter().all(|(_, reason)| *reason == DropReason::Density),
        "{drops:?}"
    );
    // Each record is one value, with its observation, and together they are what a wide plot prints.
    assert!(
        narrow
            .dropped
            .iter()
            .all(|d| d.observation.is_some() && d.would_be_at.is_some()),
        "{:?}",
        narrow.dropped
    );
    assert_eq!(numbers(&narrow), numbers(&draw_at(&spec, &rows, 1100.0)));
    // Three rows fit it.
    let three = draw_at(&spec, &rows[..3], 390.0);
    assert_eq!(texts(&three).len(), 6, "{:?}", three.dropped);
    stands_clear(&three);
    // Six rows are over the five of a wide plot.
    let mut six = rows.clone();
    six.push(json!({"team":"Epsilon","before":58,"after":61}));
    six.push(json!({"team":"Zeta","before":44,"after":73}));
    let wide = draw_at(&spec, &six, 1100.0);
    assert!(texts(&wide).is_empty());
    let drops = value_drops(&wide);
    assert_eq!(drops.len(), 12, "{drops:?}");
    assert!(
        drops.iter().all(|(_, reason)| *reason == DropReason::Density),
        "{drops:?}"
    );
    contract(&spec, &six);
}

/// A dropped value gets its text back from its own readout line when the runtime replays the
/// shown values: a dumbbell's upper end from the upper line, not from the first line of its
/// series, and a treemap tile's value from its tile.
#[test]
fn a_dropped_value_is_restored_from_its_own_readout_line() {
    let (spec, rows) = fixture("dumbbell");
    let wide = numbers(&draw_at(&spec, &rows, 1100.0));
    let mut narrow = draw_at(&spec, &rows, 390.0);
    narrow.restore_value_drop_text();
    assert_eq!(numbers(&narrow), wide);
    // The replay gives every readout line another text; each record follows its own line.
    for o in &mut narrow.panels[0].observations {
        let readout = std::rc::Rc::make_mut(&mut o.readout);
        for (n, line) in readout.lines.iter_mut().enumerate() {
            line.value = format!("{}#{n}", line.value);
        }
    }
    narrow.restore_value_drop_text();
    let mut restored = numbers(&narrow);
    let mut expected: Vec<String> = narrow.panels[0]
        .observations
        .iter()
        .flat_map(|o| {
            o.readout
                .lines
                .iter()
                .filter(|line| !line.derived)
                .map(|line| line.value.clone())
        })
        .collect();
    restored.sort();
    expected.sort();
    assert_eq!(restored, expected);

    let (spec, rows) = fixture("treemap");
    let mut narrow = draw_at(&spec, &rows, 390.0);
    assert!(!value_drops(&narrow).is_empty());
    for o in &mut narrow.panels[0].observations {
        let readout = std::rc::Rc::make_mut(&mut o.readout);
        readout.lines[0].value = format!("{}#", readout.lines[0].value);
    }
    narrow.restore_value_drop_text();
    let drops = value_drops(&narrow);
    assert!(drops.iter().all(|(text, _)| text.ends_with('#')), "{drops:?}");
}

/// A reader gets the same numbers from a chart's automatic values at every width (DEL-52): a
/// value a narrow plot does not print is recorded as dropped, each on its own.
#[test]
fn a_chart_prints_or_records_the_same_values_at_every_width() {
    let boxes = (box_spec(), box_rows(4));
    for (name, (spec, rows)) in [
        ("dumbbell", fixture("dumbbell")),
        ("bullet", fixture("bullet")),
        ("treemap", fixture("treemap")),
        ("combo", fixture("combo")),
        ("box plot", boxes),
    ] {
        let wide = numbers(&draw_at(&spec, &rows, 1100.0));
        assert!(!wide.is_empty(), "{name}");
        for width in [760.0, 390.0, 320.0] {
            assert_eq!(numbers(&draw_at(&spec, &rows, width)), wide, "{name} at {width}");
        }
    }
}

#[test]
fn a_bullet_prints_its_measure_beyond_the_bar() {
    let (spec, rows) = fixture("bullet");
    let g = draw_at(&spec, &rows, 1100.0);
    assert_eq!(texts(&g), ["72", "91", "55", "64"]);
    assert!(g.dropped.is_empty(), "{:?}", g.dropped);
    stands_clear(&g);
    let panel = &g.panels[0];
    for (n, t) in values(&g).iter().enumerate() {
        let bar = panel
            .items
            .iter()
            .find_map(|i| match i {
                Item::Rect {
                    rect,
                    obs: Some(o),
                    role: Role::Mark,
                    ..
                } if *o == n => Some(*rect),
                _ => None,
            })
            .expect("the measure's bar");
        assert!(
            (t.bounds.x - bar.right() - 4.0).abs() < 0.01,
            "{} starts 4 px past its bar",
            t.text
        );
    }
    // The longest bar reaches past every target and band: the axis keeps room for its value.
    let long = [json!({"region":"North","sales":118,"goal":85})];
    let g = draw_at(&spec, &long, 1100.0);
    assert_eq!(texts(&g), ["118"]);
    stands_clear(&g);
    contract(&spec, &rows);
}

#[test]
fn a_bullet_value_never_stands_at_a_target_or_band_edge() {
    let (spec, _) = fixture("bullet");
    // The target stands just past the first bar, and the second bar ends just short of the 80
    // band edge: each value would stand at a mark. No place for one value means no value at all.
    for rows in [
        vec![
            json!({"region":"North","sales":72,"goal":73}),
            json!({"region":"South","sales":55,"goal":90}),
        ],
        vec![
            json!({"region":"North","sales":79,"goal":95}),
            json!({"region":"South","sales":55,"goal":90}),
        ],
    ] {
        let g = draw_at(&spec, &rows, 1100.0);
        assert!(texts(&g).is_empty(), "{:?}", texts(&g));
        let drops = value_drops(&g);
        assert_eq!(drops.len(), 2, "each value is recorded: {drops:?}");
        assert!(drops.iter().all(|(_, reason)| *reason == DropReason::Collision));
        contract(&spec, &rows);
    }
}

fn box_rows(groups: usize) -> Vec<Value> {
    (0..groups * 20)
        .map(|i| {
            let g = ["A", "B", "C", "D", "E", "F"][i % groups];
            json!({"g": g, "v": (i * 7 % 23) as f64 + (i % groups) as f64 * 3.0})
        })
        .collect()
}

fn box_spec() -> Value {
    json!({"type":"boxplot","title":"Spread by group","encoding":{"x":"g","y":"v"}})
}

#[test]
fn a_box_plot_prints_each_median_beside_its_box() {
    let rows = box_rows(3);
    for orientation in ["vertical", "horizontal"] {
        let mut spec = box_spec();
        spec["style"]["orientation"] = json!(orientation);
        for width in [1100.0, 390.0] {
            let g = draw_at(&spec, &rows, width);
            let panel = &g.panels[0];
            let printed = values(&g);
            if orientation == "horizontal" && width < 480.0 {
                // Rows this close leave no line of figures between two boxes: no median has a
                // place, so none is printed and each is recorded.
                assert!(printed.is_empty());
                let drops = value_drops(&g);
                assert_eq!(drops.len(), 3, "{drops:?}");
                assert!(drops.iter().all(|(_, reason)| *reason == DropReason::Collision));
                continue;
            }
            assert_eq!(printed.len(), 3, "{orientation} at {width}: {:?}", g.dropped);
            stands_clear(&g);
            // The value axis runs along 1 when vertical, the category axis along the other.
            let axis = usize::from(orientation == "vertical");
            for t in &printed {
                let (n, o) = panel
                    .observations
                    .iter()
                    .enumerate()
                    .find(|(_, o)| {
                        line(o, "Median") == Some(t.text.as_str()) && (o.at[axis] - t.bounds.center()[axis]).abs() < 1.0
                    })
                    .unwrap_or_else(|| panic!("{} is a box's median, at its height", t.text));
                let rect = panel
                    .items
                    .iter()
                    .find_map(|i| match i {
                        Item::Rect {
                            rect,
                            obs: Some(b),
                            role: Role::Mark,
                            ..
                        } if *b == n => Some(*rect),
                        _ => None,
                    })
                    .expect("the box");
                let (lo, hi) = if axis == 1 {
                    (rect.x, rect.right())
                } else {
                    (rect.y, rect.bottom())
                };
                let (a, b) = if axis == 1 {
                    (t.bounds.x, t.bounds.right())
                } else {
                    (t.bounds.y, t.bounds.bottom())
                };
                // Every median stands on the same side: right of an upright box, under a lying
                // one. Only a box whose value would leave the plot there takes the other side.
                let edge = if axis == 1 {
                    panel.plot.right()
                } else {
                    panel.plot.bottom()
                };
                let no_room = hi + 4.0 + (b - a) > edge + 0.5;
                assert!(
                    (a - hi - 4.0).abs() < 0.01 || (no_room && (lo - b - 4.0).abs() < 0.01),
                    "{orientation}: {} stands 4 px right of or under its box ({:?}, box {rect:?}, {:?})",
                    t.text,
                    t.bounds,
                    o.at
                );
            }
            contract(&spec, &rows);
        }
    }
    // Six boxes are over the budget of five.
    let six = box_rows(6);
    let g = draw_at(&box_spec(), &six, 1100.0);
    assert!(texts(&g).is_empty());
    let drops = value_drops(&g);
    assert_eq!(drops.len(), 6, "{drops:?}");
    assert!(
        drops.iter().all(|(_, reason)| *reason == DropReason::Density),
        "{drops:?}"
    );
}

/// A treemap's largest tiles print their value on a second line under the name, in the tile:
/// five of them, three under 480 px of plot width, where both lines fit.
#[test]
fn a_treemap_prints_the_value_of_its_largest_tiles_under_the_name() {
    let (mut spec, rows) = fixture("treemap");
    for (width, budget) in [(1100.0, 5), (390.0, 3)] {
        let g = draw_at(&spec, &rows, width);
        let panel = &g.panels[0];
        let mut tiles: Vec<(usize, Rect)> = panel
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Rect {
                    rect,
                    obs: Some(o),
                    role: Role::Mark,
                    ..
                } => Some((*o, *rect)),
                _ => None,
            })
            .collect();
        tiles.sort_by(|a, b| (b.1.w * b.1.h).total_cmp(&(a.1.w * a.1.h)));
        let printed = values(&g);
        let dropped = value_drops(&g).len();
        // The five largest tiles are printed or recorded at every width; the budget prints.
        assert!(
            !printed.is_empty() && printed.len() <= budget && printed.len() + dropped == tiles.len().min(5),
            "at {width}: {printed:?} {:?}",
            g.dropped
        );
        for t in &printed {
            let (o, tile) = tiles[..budget]
                .iter()
                .find(|(_, r)| t.bounds.inside(r, 0.0))
                .unwrap_or_else(|| panic!("{} stands in one of the {budget} largest tiles", t.text));
            let observation = &panel.observations[*o];
            assert_eq!(
                Some(t.text.as_str()),
                observation.readout.lines.first().map(|l| l.value.as_str())
            );
            let name = panel
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text(n) if n.role == Role::Label && n.face == Face::Data && n.bounds.inside(tile, 0.0) => {
                        Some(n)
                    }
                    _ => None,
                })
                .expect("the tile's name");
            assert!((t.bounds.x - name.bounds.x).abs() < 0.01 && t.bounds.y >= name.bounds.bottom() - 0.01);
            assert_eq!(
                (t.color, t.weight),
                (name.color, name.weight),
                "the name's ink at its weight"
            );
        }
        contract(&spec, &rows);
    }
    spec["auto_annotations"] = json!(false);
    let off = draw_at(&spec, &rows, 1100.0);
    assert!(texts(&off).is_empty());
    assert!(value_drops(&off).is_empty());
}

/// How many value texts a mark line runs through: the line is walked in half-pixel steps
/// (FEA-CH-29: no line through text).
fn lines_through_text(g: &Geometry) -> usize {
    let panel = &g.panels[0];
    values(g)
        .iter()
        .filter(|t| {
            panel.items.iter().any(|item| match item {
                Item::Line {
                    path, role: Role::Mark, ..
                } => path.flatten(0.5).iter().any(|line| {
                    line.windows(2).any(|s| {
                        let steps = ((s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]) * 2.0).ceil().max(1.0);
                        (0..=steps as usize).any(|i| {
                            let k = i as f64 / steps;
                            t.bounds
                                .contains([s[0][0] + (s[1][0] - s[0][0]) * k, s[0][1] + (s[1][1] - s[0][1]) * k])
                        })
                    })
                }),
                _ => false,
            })
        })
        .count()
}

fn bars(g: &Geometry) -> Vec<Rect> {
    g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

/// A combo prints its bars by the bar rule and its line's last value beside the last point,
/// each in its own unit; the line's value stands on no bar and no line runs through a value.
#[test]
fn a_combo_prints_its_bars_and_its_lines_last_value_each_in_its_unit() {
    let (spec, rows) = fixture("combo");
    for orientation in ["vertical", "horizontal"] {
        let mut spec = spec.clone();
        spec["style"]["orientation"] = json!(orientation);
        let g = draw_at(&spec, &rows, 1100.0);
        assert_eq!(
            texts(&g),
            ["27%", "$120", "$150", "$135", "$180"],
            "{orientation}: {:?}",
            g.dropped
        );
        assert!(value_drops(&g).is_empty(), "{orientation}: {:?}", g.dropped);
        let line = values(&g)[0];
        assert_eq!(line.series, Some(1), "{orientation}: the line's own series");
        assert!(
            line.bounds.inside(&g.panels[0].plot, 0.5),
            "{orientation}: {:?}",
            line.bounds
        );
        let near = line.bounds.inflate(2.0);
        assert!(
            bars(&g).iter().all(|bar| !near.overlaps(bar, 0.0)),
            "{orientation}: the line's value stands on no bar"
        );
        assert_eq!(lines_through_text(&g), 0, "{orientation}");
        contract(&spec, &rows);
    }
}

/// Where a combo's bar values stand: `Some(true)` when each is past its bar's end, `Some(false)`
/// when each is inside the foot of its bar on one line, `None` when they are mixed.
fn bar_values_place(g: &Geometry, horizontal: bool) -> Option<bool> {
    let panel = &g.panels[0];
    let placed: Vec<(Rect, Rect)> = values(g)
        .iter()
        .filter(|t| t.series == Some(0))
        .map(|t| {
            let bar = bars(g)
                .into_iter()
                .find(|bar| {
                    if horizontal {
                        (bar.center()[1] - t.bounds.center()[1]).abs() < 1.0
                    } else {
                        (bar.center()[0] - t.bounds.center()[0]).abs() < 1.0
                    }
                })
                .unwrap_or_else(|| panic!("{} stands at its own bar", t.text));
            (t.bounds, bar)
        })
        .collect();
    assert!(!placed.is_empty());
    let _ = panel;
    let beyond = |(t, bar): &(Rect, Rect)| {
        if horizontal {
            (t.x - bar.right() - 4.0).abs() < 0.01
        } else {
            (bar.y - t.bottom() - 4.0).abs() < 1.0
        }
    };
    let foot = |(t, bar): &(Rect, Rect)| {
        if horizontal {
            (t.x - bar.x - 6.0).abs() < 0.01
        } else {
            (bar.bottom() - t.bottom() - 6.0).abs() < 1.0 && t.inside(bar, 0.0)
        }
    };
    if placed.iter().all(beyond) {
        Some(true)
    } else if placed.iter().all(foot) {
        Some(false)
    } else {
        None
    }
}

/// A combo's bar values stand in one place (chart design, 16 Nov 2026): all past their bars'
/// ends when the line crosses none of those places, otherwise all inside the foot of their bars
/// on one line, where a value the line crosses is dropped.
#[test]
fn a_combos_bar_values_all_stand_in_one_place() {
    let (spec, rows) = fixture("combo");
    for orientation in ["vertical", "horizontal"] {
        let mut spec = spec.clone();
        spec["style"]["orientation"] = json!(orientation);
        for width in [1100.0, 390.0] {
            let g = draw_at(&spec, &rows, width);
            let place = bar_values_place(&g, orientation == "horizontal");
            assert!(place.is_some(), "{orientation} at {width}: one place for all");
            assert_eq!(lines_through_text(&g), 0, "{orientation} at {width}");
        }
    }
    // A line that runs over the bars' ends sends every value to the foot of its bar.
    let (mut spec, mut rows) = fixture("combo");
    for row in &mut rows {
        row["margin"] = json!(27);
    }
    spec["axes"]["y2"]["min"] = json!(0);
    spec["axes"]["y2"]["max"] = json!(38);
    let g = draw_at(&spec, &rows, 1100.0);
    assert_eq!(bar_values_place(&g, false), Some(false), "{:?}", g.dropped);
    assert_eq!(lines_through_text(&g), 0);
    contract(&spec, &rows);
}

/// On a narrow combo the last point stands on its bar with the plot's edge beside it: the
/// line's value has no place, so it is recorded as dropped and the bars keep theirs.
#[test]
fn a_combo_line_value_with_no_place_is_recorded_and_the_bars_keep_theirs() {
    let (spec, rows) = fixture("combo");
    for orientation in ["vertical", "horizontal"] {
        let mut spec = spec.clone();
        spec["style"]["orientation"] = json!(orientation);
        let g = draw_at(&spec, &rows, 390.0);
        assert_eq!(texts(&g), ["$120", "$150", "$135", "$180"], "{orientation}");
        assert_eq!(value_drops(&g), [("27%", DropReason::Collision)], "{orientation}");
        assert_eq!(lines_through_text(&g), 0, "{orientation}");
        contract(&spec, &rows);
    }
}

/// `auto_annotations: false` prints neither the bars' values nor the line's, and records no
/// drop: the author chose it.
#[test]
fn the_author_switch_prints_no_combo_value() {
    let (mut spec, rows) = fixture("combo");
    spec["auto_annotations"] = json!(false);
    for width in [1100.0, 390.0] {
        let g = draw_at(&spec, &rows, width);
        assert!(texts(&g).is_empty(), "{width}");
        assert!(g.dropped.is_empty(), "{width}: {:?}", g.dropped);
    }
    contract(&spec, &rows);
}

/// A candlestick and an OHLC chart print one value, the last close, as a price tag: 6 px right
/// of the plot at the close's own height, in room the chart keeps for it at every width.
#[test]
fn a_financial_chart_prints_its_last_close_beside_the_plot() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        for width in [1100.0, 390.0] {
            spec["auto_annotations"] = json!(true);
            let g = draw_at(&spec, &rows, width);
            let panel = &g.panels[0];
            assert_eq!(texts(&g), ["$105.3"], "{name} at {width}: {:?}", g.dropped);
            assert!(value_drops(&g).is_empty(), "{name} at {width}");
            let tag = values(&g)[0];
            assert!(
                (tag.bounds.x - panel.plot.right() - 6.0).abs() < 0.01,
                "{name} at {width}"
            );
            assert!(
                tag.bounds.inside(&panel.frame, 0.5),
                "{name} at {width}: {:?}",
                tag.bounds
            );
            // The last period closed below its open: the close is the foot of a candle's body
            // and the right tick of an OHLC bar.
            let middle = tag.bounds.y + tag.bounds.h / 2.0;
            let last = panel.observations.len() - 1;
            let at_close = panel.items.iter().any(|item| match item {
                Item::Rect {
                    rect,
                    role: Role::Mark,
                    obs,
                    ..
                } => *obs == Some(last) && (rect.bottom() - middle).abs() < 0.6,
                Item::Line {
                    path,
                    role: Role::Mark,
                    obs,
                    ..
                } => {
                    *obs == Some(last)
                        && path.flatten(0.5).iter().any(|line| {
                            line.windows(2)
                                .any(|s| s[0][1] == s[1][1] && (s[0][1] - middle).abs() < 0.6)
                        })
                }
                _ => false,
            });
            assert!(at_close, "{name} at {width}: the value stands at the close's height");
            assert_eq!(tag.series, panel.observations[last].series, "{name}");
            assert_eq!(lines_through_text(&g), 0, "{name} at {width}");
            contract(&spec, &rows);
            // The author's switch prints nothing, records nothing and gives the plot the room.
            spec["auto_annotations"] = json!(false);
            let off = draw_at(&spec, &rows, width);
            assert!(texts(&off).is_empty(), "{name} at {width}");
            assert!(off.dropped.is_empty(), "{name} at {width}: {:?}", off.dropped);
            let gained = off.panels[0].plot.w - panel.plot.w;
            assert!((gained - tag.bounds.w).abs() < 0.01, "{name} at {width}: {gained}");
        }
    }
}

/// A horizontal financial chart keeps no room under its last row: where the plot has none, the
/// last close is recorded as dropped.
#[test]
fn a_horizontal_financial_chart_records_the_close_it_has_no_room_for() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        spec["style"]["orientation"] = json!("horizontal");
        let g = draw_at(&spec, &rows, 1100.0);
        assert!(texts(&g).is_empty(), "{name}");
        assert_eq!(value_drops(&g), [("$105.3", DropReason::OutOfBounds)], "{name}");
        contract(&spec, &rows);
    }
}

#[test]
fn the_author_switch_prints_no_sparse_value_and_keeps_the_axis_as_it_was() {
    let boxes = (box_spec(), box_rows(3));
    for (name, (mut spec, rows)) in [
        ("dumbbell", fixture("dumbbell")),
        ("bullet", fixture("bullet")),
        ("boxplot", boxes),
    ] {
        assert!(
            !texts(&draw_at(&spec, &rows, 1100.0)).is_empty(),
            "{name}: on by default"
        );
        spec["auto_annotations"] = json!(false);
        let off = draw_at(&spec, &rows, 1100.0);
        assert!(texts(&off).is_empty(), "{name}");
        assert!(off.dropped.is_empty(), "{name}: {:?}", off.dropped);
        contract(&spec, &rows);
    }
}
