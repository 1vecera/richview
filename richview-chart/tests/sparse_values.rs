//! Automatic values of the charts that printed none (FEA-CH-28 as chart design ruled on
//! 16 Nov 2026): a dumbbell's two ends, a bullet's measure, a box plot's medians, a treemap's
//! largest tiles, a combo's bars and last line value. Rows share the budget of five, three under
//! 480 px of plot width, and every value is printed or none. A financial chart's last close, high
//! and low follow their own ruling (chart design, 17 Nov 2026): the close always stays.

use super::common;

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

/// The tag box drawn behind a financial chart's last close.
fn tag_chip(g: &Geometry) -> Option<Rect> {
    g.panels[0].items.iter().find_map(|item| match item {
        Item::Rect {
            rect,
            role: Role::Annotation,
            ..
        } => Some(*rect),
        _ => None,
    })
}

/// The soft word set before a financial chart's high or low.
fn word<'a>(g: &'a Geometry, word: &str) -> Option<&'a Text> {
    g.panels[0].items.iter().find_map(|item| match item {
        Item::Text(t) if t.text == word && t.role == Role::Label && t.face == Face::Data => Some(t),
        _ => None,
    })
}

/// A candlestick and an OHLC chart print three values, each saying what it is (chart design,
/// 17 Nov 2026): the last close as a tag in the last period's colour, on the plot's right edge at
/// the close's own height and joined to its mark by a hairline, in room the chart keeps for it
/// at every width; the period high over its wick and the low under its wick, a soft word
/// before each number. The tag always stays; a high or low with no clear place is recorded.
#[test]
fn a_financial_chart_prints_its_last_close_as_a_tag_and_names_its_high_and_low() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        for width in [1100.0, 390.0] {
            spec["auto_annotations"] = json!(true);
            let g = draw_at(&spec, &rows, width);
            let panel = &g.panels[0];
            let tag = *values(&g)
                .iter()
                .find(|t| t.text == "$105.3")
                .unwrap_or_else(|| panic!("{name} at {width}: the close prints: {:?}", texts(&g)));
            let chip = tag_chip(&g).unwrap_or_else(|| panic!("{name} at {width}: the tag has its box"));
            assert!(tag.bounds.inside(&chip, 0.01), "{name} at {width}");
            assert!((chip.x - panel.plot.right()).abs() < 0.01, "{name} at {width}");
            assert!(chip.inside(&panel.frame, 0.5), "{name} at {width}: {chip:?}");
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
            assert!(at_close, "{name} at {width}: the tag stands at the close's height");
            assert_eq!(tag.series, panel.observations[last].series, "{name}");
            // The hairline runs from the last mark to the tag's near side, in the tag's colour.
            let hairline = panel.items.iter().find_map(|item| match item {
                Item::Line {
                    path,
                    role: Role::Leader,
                    obs,
                    stroke,
                    ..
                } if *obs == Some(last) => Some((path.flatten(0.5), stroke.color)),
                _ => None,
            });
            let (path, colour) = hairline.unwrap_or_else(|| panic!("{name} at {width}: a hairline"));
            let end = *path.last().and_then(|l| l.last()).expect("a point");
            assert!(
                (end[0] - chip.x).abs() < 0.01 && (end[1] - middle).abs() < 0.6,
                "{name} at {width}"
            );
            let fill = panel.items.iter().find_map(|item| match item {
                Item::Rect {
                    role: Role::Annotation,
                    fill,
                    ..
                } => Some(fill.color),
                _ => None,
            });
            assert_eq!(fill, Some(colour), "{name} at {width}");
            // The high and the low: each printed past its wick with its word, or recorded.
            let drops = value_drops(&g);
            for (word_text, number, above) in [("High", "$107.4", true), ("Low", "$99.5", false)] {
                match (word(&g, word_text), values(&g).into_iter().find(|t| t.text == number)) {
                    (Some(w), Some(n)) => {
                        assert!(w.bounds.right() < n.bounds.x, "{name} at {width}: {word_text} leads");
                        assert!((w.y - n.y).abs() < 0.01, "{name} at {width}: one line");
                        assert_ne!(w.color, n.color, "{name} at {width}: the word is soft");
                        let o = &panel.observations[panel
                            .observations
                            .iter()
                            .position(|o| {
                                o.readout
                                    .lines
                                    .iter()
                                    .any(|l| l.name == word_text && l.printed() == number)
                            })
                            .expect("its period")];
                        let wick = o.area.expect("a wick");
                        if above {
                            assert!(n.bounds.bottom() <= wick.y, "{name} at {width}: over the wick");
                        } else {
                            assert!(n.bounds.y >= wick.bottom(), "{name} at {width}: under the wick");
                        }
                    }
                    (None, None) => assert!(
                        drops.iter().any(|d| d.0 == format!("{word_text} {number}")),
                        "{name} at {width}: {number} printed or recorded: {drops:?}"
                    ),
                    other => panic!("{name} at {width}: {word_text} and its number go together: {other:?}"),
                }
            }
            assert!(
                !drops.iter().any(|d| d.0 == "$105.3"),
                "{name} at {width}: the tag stays"
            );
            // An upright chart with a plot of 480 px or more keeps a line of room past the
            // highest and the lowest wick, so both stand. A narrower one keeps none, so its marks
            // keep the plot's height: its value axis is the one the switch leaves.
            if width > 1000.0 {
                assert!(
                    word(&g, "High").is_some() && word(&g, "Low").is_some(),
                    "{name} at {width}: the high and the low have room: {drops:?}"
                );
            }
            let ticks = |g: &Geometry| -> Vec<String> {
                g.panels[0]
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Text(t) if t.role == Role::AxisLabel && t.text.starts_with('$') => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect()
            };
            let mut plain = spec.clone();
            plain["auto_annotations"] = json!(false);
            if width < 480.0 {
                assert_eq!(ticks(&g), ticks(&draw_at(&plain, &rows, width)), "{name} at {width}");
            }
            assert_eq!(lines_through_text(&g), 0, "{name} at {width}");
            contract(&spec, &rows);
            // The author's switch prints nothing, records nothing and gives the plot the room.
            spec["auto_annotations"] = json!(false);
            let off = draw_at(&spec, &rows, width);
            assert!(texts(&off).is_empty(), "{name} at {width}");
            assert!(
                word(&off, "High").is_none() && tag_chip(&off).is_none(),
                "{name} at {width}"
            );
            assert!(off.dropped.is_empty(), "{name} at {width}: {:?}", off.dropped);
            // The value axis keeps no room for a high and a low either, so its ticks, and with
            // them the plot's left edge, may differ; the room gained is read at the right edge.
            // It is the number's width alone: the tag's padding stands in the plot's own right
            // margin, so the tag costs the plot no more than the bare number did.
            let gained = off.panels[0].plot.right() - panel.plot.right();
            assert!((gained - tag.bounds.w).abs() < 0.01, "{name} at {width}: {gained}");
        }
    }
}

/// The high and the low come as a pair (lead, 19 Nov 2026): with room under the lowest wick and
/// none over the highest, a lone "Low" would leave the reader looking for the high, so neither
/// is printed and both are recorded; the tag stays.
#[test]
fn a_financial_high_and_low_are_printed_together_or_not_at_all() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        // The axis ends at the highest wick and runs well below the lowest.
        spec["axes"]["y"]["min"] = json!(90);
        spec["axes"]["y"]["max"] = json!(107.4);
        let mut g = draw_at(&spec, &rows, 1100.0);
        assert!(word(&g, "High").is_none(), "{name}: no room over the highest wick");
        assert!(word(&g, "Low").is_none(), "{name}: the low goes with the high");
        g.restore_value_drop_text();
        let mut drops: Vec<&str> = value_drops(&g).into_iter().map(|d| d.0).collect();
        drops.sort_unstable();
        assert_eq!(drops, ["High $107.4", "Low $99.5"], "{name}: both are recorded");
        assert!(texts(&g).contains(&"$105.3"), "{name}: the tag stays");
        // With room at both ends the pair stands.
        spec["axes"]["y"]["max"] = json!(120);
        let g = draw_at(&spec, &rows, 1100.0);
        assert!(word(&g, "High").is_some() && word(&g, "Low").is_some(), "{name}");
    }
}

/// The room for the high and the low is decided on one measure, the panel's width: an upright
/// chart whose panel is at least 480 px wide keeps a line of room past its highest and lowest
/// wick, and one a pixel narrower keeps dev's axis, whose ends are the wicks' ends.
#[test]
fn a_financial_chart_keeps_room_for_its_high_and_low_from_a_480_px_panel() {
    for name in ["candlestick", "ohlc"] {
        let (spec, rows) = fixture(name);
        for width in [390.0, 470.0, 479.0, 480.0, 481.0, 640.0] {
            let g = draw_at(&spec, &rows, width);
            let panel = &g.panels[0];
            let highest = panel
                .observations
                .iter()
                .filter_map(|o| o.area)
                .map(|wick| wick.y)
                .fold(f64::INFINITY, f64::min);
            let lowest = panel
                .observations
                .iter()
                .filter_map(|o| o.area)
                .map(|wick| wick.bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            let room = (highest - panel.plot.y).min(panel.plot.bottom() - lowest);
            let wide = panel.frame.w >= 480.0;
            assert_eq!(wide, width >= 480.0, "{name} at {width}: the panel is the frame");
            if wide {
                assert!(room >= 20.0, "{name} at {width}: a line of room at each end: {room}");
            } else {
                assert!(room < 2.0, "{name} at {width}: the axis ends at the wicks: {room}");
            }
        }
    }
}

/// WCAG 2 contrast of two gamma-encoded sRGB colours.
fn contrast(a: richview_chart::geometry::Rgba, b: richview_chart::geometry::Rgba) -> f64 {
    let luminance = |c: richview_chart::geometry::Rgba| {
        let linear = |v: f32| {
            let v = f64::from(v);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.0[0]) + 0.7152 * linear(c.0[1]) + 0.0722 * linear(c.0[2])
    };
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The tag's number reads at 4.5:1 on its box whatever the last period's colour, as every value
/// printed on a mark does: on a light or a dark colour it takes the surface or the ink, and on a
/// mid-tone where neither reaches the floor the box is the surface, outlined in that colour.
#[test]
fn a_financial_tags_number_reads_on_its_box_in_any_palette() {
    let (spec, rows) = fixture("candlestick");
    let block = richview_chart::block::ChartBlock::from_value(&spec).expect("a chart block");
    let mut outlined = 0;
    let mut filled = 0;
    for lightness in (20..=90).step_by(2) {
        let colour = format!("oklch({} 0.12 250)", f64::from(lightness) / 100.0);
        let theme = richview_chart::layout::ChartTheme {
            palette: vec![colour.clone(), colour.clone()],
            ..richview_chart::layout::ChartTheme::default()
        };
        let options = richview_chart::layout::Options {
            width: 1100.0,
            ..richview_chart::layout::Options::default()
        };
        let g = richview_chart::layout::layout(&block, &rows, &theme, &options);
        let (fill, stroke) = g.panels[0]
            .items
            .iter()
            .find_map(|item| match item {
                Item::Rect {
                    role: Role::Annotation,
                    fill,
                    stroke,
                    ..
                } => Some((fill.color, stroke.as_ref().map(|s| s.color))),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{colour}: a tag"));
        let number = *values(&g).iter().find(|t| t.text == "$105.3").expect("the close");
        let ratio = contrast(number.color, fill);
        assert!(ratio >= 4.5, "{colour}: the number reads on its box: {ratio:.2}");
        let hairline = g.panels[0]
            .items
            .iter()
            .find_map(|item| match item {
                Item::Line {
                    role: Role::Leader,
                    stroke,
                    ..
                } => Some(stroke.color),
                _ => None,
            })
            .expect("a hairline");
        match stroke {
            // Outlined: the box is not the period's colour, its outline and the hairline are.
            Some(outline) => {
                assert_eq!(outline, hairline, "{colour}");
                assert_ne!(fill, hairline, "{colour}");
                outlined += 1;
            }
            None => {
                assert_eq!(fill, hairline, "{colour}");
                filled += 1;
            }
        }
    }
    assert!(
        filled > 0 && outlined > 0,
        "both forms occur: {filled} filled, {outlined} outlined"
    );
}

/// A high or low with no place is recorded with its word and its own number, "Low $99.5", the
/// one its readout line gives back, not with the close of its period: the lines of a price
/// period's high and low belong to no series, and the record names them all the same.
#[test]
fn a_financial_high_or_low_with_no_place_is_recorded_with_its_own_number() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        // The author fixes the axis at the prices' own ends: no room past either wick.
        spec["axes"]["y"]["min"] = json!(99.5);
        spec["axes"]["y"]["max"] = json!(107.4);
        let mut g = draw_at(&spec, &rows, 1100.0);
        g.restore_value_drop_text();
        let mut drops: Vec<&str> = value_drops(&g).into_iter().map(|d| d.0).collect();
        drops.sort_unstable();
        assert_eq!(drops, ["High $107.4", "Low $99.5"], "{name}");
        assert!(texts(&g).contains(&"$105.3"), "{name}: the tag stays");
        assert!(word(&g, "High").is_none() && word(&g, "Low").is_none(), "{name}");
    }
}

/// A horizontal financial chart keeps no room under its last row: where the plot has none, the
/// last close is recorded as dropped and no tag is drawn.
#[test]
fn a_horizontal_financial_chart_records_the_close_it_has_no_room_for() {
    for name in ["candlestick", "ohlc"] {
        let (mut spec, rows) = fixture(name);
        spec["style"]["orientation"] = json!("horizontal");
        let g = draw_at(&spec, &rows, 1100.0);
        assert!(!texts(&g).contains(&"$105.3"), "{name}");
        assert!(tag_chip(&g).is_none(), "{name}");
        assert!(value_drops(&g).contains(&("$105.3", DropReason::OutOfBounds)), "{name}");
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
